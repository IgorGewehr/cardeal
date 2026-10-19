//! Consultas de leitura do PDV (`docs/modulos/pdv.md` §6).
//!
//! O preço vigente é regra de negócio de `vendas`: o PDV o pede pelas **portas públicas** dos
//! outros módulos (`TabelasDePreco`, `RegrasDaTabela`, `ProdutoPorId`,
//! `SaldoDisponivelDoProduto`) e aplica a mesma função pura, [`mod_vendas::preco_vigente`], que
//! `AdicionarItem` usa — assim a consulta de preço nunca diverge do preço que a venda cobra.

#[cfg(feature = "sqlite")]
use cardeal_kernel::{Erro, Resultado};
use cardeal_kernel::{Id, Preco, Quantidade};
use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use mod_estoque::{ProdutoPorId, SaldoDisponivelDoProduto};
#[cfg(feature = "sqlite")]
use mod_vendas::{preco_vigente, RegrasDaTabela, TabelasDePreco};
#[cfg(feature = "sqlite")]
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

    #[cfg(feature = "sqlite")]
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

/// As vendas do balcão num período (pela abertura do cupom, no fuso da empresa): finalizadas e
/// estornadas, das mais novas para as mais antigas, até 1.000. É a lista "Vendas do dia" do PDV,
/// de onde se reimprime e se estorna.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct VendasDoPeriodo {
    /// O primeiro dia.
    pub de: cardeal_kernel::Data,
    /// O último dia (inclusive).
    pub ate: cardeal_kernel::Data,
}

/// O estorno de uma venda.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EstornoDaVenda {
    /// O motivo.
    pub motivo: String,
    /// Quem autorizou.
    pub autorizado_por: Id,
    /// Quando.
    pub em: cardeal_kernel::Instante,
}

/// Uma venda da lista.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VendaDoPeriodo {
    /// O cupom.
    pub cupom: Id,
    /// O número no terminal.
    pub numero: i64,
    /// O terminal.
    pub terminal: Id,
    /// Quando o cupom foi aberto.
    pub abertura: cardeal_kernel::Instante,
    /// O cliente identificado.
    pub cliente: Option<Id>,
    /// Quem operou.
    pub operador: Id,
    /// O total.
    pub total: cardeal_kernel::Dinheiro,
    /// Os pagamentos.
    pub pagamentos: Vec<crate::cupom::PagamentoCupom>,
    /// `Finalizado` ou `Estornado`.
    pub estado: crate::cupom::EstadoCupom,
    /// O estorno, quando houve.
    pub estorno: Option<EstornoDaVenda>,
}

impl Consulta for VendasDoPeriodo {
    type Saida = Vec<VendaDoPeriodo>;
    const PERMISSAO: &'static str = "pdv.venda.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        use crate::repositorio::{blob, id_de, id_opt_de, pagamento_de_linha, persist};
        use cardeal_kernel::{Dinheiro, Hora, Instante};

        let inicio = Instante::de_data_hora(self.de, Hora::MEIA_NOITE, ctx.fuso);
        let fim = Instante::de_data_hora(self.ate.mais_dias(1), Hora::MEIA_NOITE, ctx.fuso);
        let mut stmt = conexao
            .prepare(
                "SELECT c.id, c.numero_terminal, c.terminal, c.abertura, c.cliente, c.operador,
                        c.total, e.motivo, e.autorizado_por, e.estornado_em
                 FROM pdv_cupom c
                 LEFT JOIN pdv_estorno e ON e.cupom = c.id
                 WHERE c.empresa = ?1 AND c.estado = 'Finalizado'
                   AND c.abertura >= ?2 AND c.abertura < ?3
                 ORDER BY c.abertura DESC
                 LIMIT 1000",
            )
            .map_err(persist)?;
        let mut vendas = stmt
            .query_map(
                rusqlite::params![blob(ctx.empresa), inicio.em_micros(), fim.em_micros()],
                |r| {
                    let estorno = match r.get::<_, Option<String>>(7)? {
                        Some(motivo) => Some(EstornoDaVenda {
                            motivo,
                            autorizado_por: id_de(r.get::<_, Vec<u8>>(8)?),
                            em: Instante::de_micros(r.get::<_, i64>(9)?),
                        }),
                        None => None,
                    };
                    Ok(VendaDoPeriodo {
                        cupom: id_de(r.get::<_, Vec<u8>>(0)?),
                        numero: r.get(1)?,
                        terminal: id_de(r.get::<_, Vec<u8>>(2)?),
                        abertura: Instante::de_micros(r.get::<_, i64>(3)?),
                        cliente: id_opt_de(r.get::<_, Option<Vec<u8>>>(4)?),
                        operador: id_de(r.get::<_, Vec<u8>>(5)?),
                        total: Dinheiro::centavos(r.get::<_, i64>(6)?),
                        pagamentos: Vec::new(),
                        estado: if estorno.is_some() {
                            crate::cupom::EstadoCupom::Estornado
                        } else {
                            crate::cupom::EstadoCupom::Finalizado
                        },
                        estorno,
                    })
                },
            )
            .map_err(persist)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)?;
        let mut pagamentos = conexao
            .prepare(&format!(
                "SELECT {} FROM pdv_pagamento_cupom WHERE cupom = ?1 ORDER BY rowid",
                crate::repositorio::COLUNAS_PAGAMENTO
            ))
            .map_err(persist)?;
        for v in &mut vendas {
            v.pagamentos = pagamentos
                .query_map([blob(v.cupom)], pagamento_de_linha)
                .map_err(persist)?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(persist)?;
        }
        Ok(vendas)
    }
}
