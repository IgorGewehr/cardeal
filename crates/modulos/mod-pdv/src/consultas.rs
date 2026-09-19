//! Consultas de leitura do PDV (`docs/modulos/pdv.md` §6).
//!
//! O preço vigente é regra de negócio de `vendas`: o PDV o pede pelas **portas públicas** dos
//! outros módulos (`TabelasDePreco`, `RegrasDaTabela`, `ProdutoPorId`,
//! `SaldoDisponivelDoProduto`) e aplica a mesma função pura, [`mod_vendas::preco_vigente`], que
//! `AdicionarItem` usa — assim a consulta de preço nunca diverge do preço que a venda cobra.

use cardeal_kernel::{Erro, Id, Preco, Quantidade, Resultado};
use cardeal_modkit::{Consulta, Ctx};
use mod_estoque::{ProdutoPorId, SaldoDisponivelDoProduto};
use mod_vendas::{preco_vigente, RegrasDaTabela, TabelasDePreco};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// Consulta o preço de um produto na tabela dada, **sem abrir venda** (`F10`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PrecoDoProduto {
    /// A tabela de preço a consultar.
    pub tabela_preco: Id,
    /// O produto.
    pub produto: Id,
    /// A quantidade — a regra de preço pode variar por faixa de quantidade.
    pub quantidade: Quantidade,
}

/// O que a consulta de preço devolve.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrecoConsultado {
    /// O nome do produto.
    pub nome: String,
    /// O código de barras, se houver.
    pub codigo_barras: Option<String>,
    /// O preço unitário vigente hoje para essa quantidade.
    pub preco: Preco,
    /// O saldo disponível somado entre os locais.
    pub disponivel: Quantidade,
}

impl Consulta for PrecoDoProduto {
    type Saida = PrecoConsultado;
    const PERMISSAO: &'static str = "pdv.preco.consultar";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let produto = ProdutoPorId {
            produto: self.produto,
        }
        .executar(ctx, conexao)?
        .ok_or_else(|| Erro::nao_encontrado("produto"))?;
        let tabela = TabelasDePreco
            .executar(ctx, conexao)?
            .into_iter()
            .find(|t| t.id == self.tabela_preco)
            .ok_or_else(|| Erro::nao_encontrado("tabela de preço"))?;
        let regras = RegrasDaTabela { tabela: tabela.id }.executar(ctx, conexao)?;

        let preco = preco_vigente(
            &tabela,
            &regras,
            produto.id,
            produto.grupo_produto,
            self.quantidade,
            ctx.hoje(),
        )
        .map_err(|e| Erro::de_dominio(&e))?;
        let disponivel = SaldoDisponivelDoProduto {
            produto: produto.id,
        }
        .executar(ctx, conexao)?;

        Ok(PrecoConsultado {
            nome: produto.nome,
            codigo_barras: produto.codigo_barras,
            preco,
            disponivel,
        })
    }
}
