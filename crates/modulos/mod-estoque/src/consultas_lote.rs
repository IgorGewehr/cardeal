//! As consultas da alteração em lote: a lista para escolher, a prévia do que mudaria e o
//! histórico com o botão de desfazer.

use cardeal_kernel::{Id, Instante, Percentual};
use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::alteracao_em_lote::{Alteracao, ValoresProduto};
#[cfg(feature = "sqlite")]
use cardeal_kernel::{Erro, Resultado};

/// Um produto na lista de escolha do lote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProdutoParaLote {
    /// O produto.
    pub produto: Id,
    /// O nome.
    pub nome: String,
    /// O grupo.
    pub grupo: Id,
    /// A margem própria (`None` = herda).
    pub margem: Option<Percentual>,
    /// Ativo.
    pub ativo: bool,
}

/// Os produtos para escolher: a subárvore de um grupo (ou todos) e/ou o que casa com a busca.
/// Inclui os inativos — o lote também reativa.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProdutosParaLote {
    /// O grupo (a subárvore inteira entra); `None` = todos.
    pub grupo: Option<Id>,
    /// Texto no nome.
    pub busca: Option<String>,
}

impl Consulta for ProdutosParaLote {
    type Saida = Vec<ProdutoParaLote>;
    const PERMISSAO: &'static str = "estoque.produto.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        use crate::hierarquia::{Arvore, NoGrupo};
        let grupos = match self.grupo {
            None => None,
            Some(g) => {
                let nos: Vec<NoGrupo> =
                    crate::repositorio_preco::perfis_dos_grupos(conexao, ctx.organizacao)?
                        .iter()
                        .map(|p| NoGrupo {
                            id: p.id,
                            pai: p.pai,
                        })
                        .collect();
                let mut todos = Arvore::de_nos(&nos).descendentes(g);
                todos.push(g);
                Some(todos)
            }
        };
        Ok(crate::repositorio_lote::produtos_do_lote(
            conexao,
            ctx.organizacao,
            grupos.as_deref(),
            self.busca.as_deref(),
            crate::alteracao_em_lote::MAX_PRODUTOS,
        )?
        .into_iter()
        .map(|p| ProdutoParaLote {
            produto: p.produto,
            nome: p.nome,
            grupo: p.valores.grupo,
            margem: p.valores.margem,
            ativo: p.valores.ativo,
        })
        .collect())
    }
}

/// Uma linha da prévia: o produto antes e depois.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinhaDaPrevia {
    /// O produto.
    pub produto: Id,
    /// O nome.
    pub nome: String,
    /// Como está hoje.
    pub antes: ValoresProduto,
    /// Como ficaria.
    pub depois: ValoresProduto,
}

impl LinhaDaPrevia {
    /// A alteração muda este produto?
    #[must_use]
    pub fn muda(&self) -> bool {
        self.antes != self.depois
    }
}

/// O que mudaria, sem gravar nada.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviaDaAlteracaoEmLote {
    /// Os produtos.
    pub produtos: Vec<Id>,
    /// A alteração.
    pub alteracao: Alteracao,
}

impl Consulta for PreviaDaAlteracaoEmLote {
    type Saida = Vec<LinhaDaPrevia>;
    const PERMISSAO: &'static str = "estoque.produto.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        if self.produtos.len() > crate::alteracao_em_lote::MAX_PRODUTOS {
            return Err(Erro::novo(
                cardeal_kernel::CodigoErro::ENTRADA_INVALIDA,
                "produtos demais para uma alteração; divida por grupo",
            ));
        }
        let mut saida = Vec::with_capacity(self.produtos.len());
        for id in &self.produtos {
            let p = crate::repositorio_lote::produto_do_lote(conexao, ctx.organizacao, *id)?
                .ok_or_else(|| Erro::nao_encontrado("produto"))?;
            saida.push(LinhaDaPrevia {
                produto: p.produto,
                nome: p.nome,
                antes: p.valores,
                depois: self.alteracao.aplicar(p.valores),
            });
        }
        Ok(saida)
    }
}

/// Uma alteração já aplicada, para o histórico.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlteracaoRegistrada {
    /// A alteração.
    pub lote: Id,
    /// A frase do usuário.
    pub descricao: String,
    /// Os campos mexidos ("margem, grupo").
    pub campos: String,
    /// Quando foi aplicada.
    pub criado_em: Instante,
    /// Quem aplicou.
    pub usuario: Id,
    /// Quantos produtos mudaram.
    pub quantidade: u32,
    /// Quando foi desfeita, se foi.
    pub desfeita_em: Option<Instante>,
    /// Dá para desfazer agora.
    pub pode_desfazer: bool,
}

/// As últimas alterações em lote (até 50), da mais nova para a mais antiga.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct HistoricoDeAlteracoesEmLote;

impl Consulta for HistoricoDeAlteracoesEmLote {
    type Saida = Vec<AlteracaoRegistrada>;
    const PERMISSAO: &'static str = "estoque.produto.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        use crate::alteracao_em_lote::pode_desfazer;
        use crate::repositorio_lote as sql;
        let ultima = sql::ultimo_lote_nao_desfeito(conexao, ctx.organizacao)?;
        Ok(sql::lotes_recentes(conexao, ctx.organizacao, 50)?
            .into_iter()
            .map(|l| AlteracaoRegistrada {
                pode_desfazer: pode_desfazer(
                    l.criado_em,
                    ctx.agora,
                    l.desfeito_em.is_some(),
                    ultima == Some(l.id),
                )
                .is_ok(),
                lote: l.id,
                descricao: l.descricao,
                campos: l.campos,
                criado_em: l.criado_em,
                usuario: l.usuario,
                quantidade: l.quantidade,
                desfeita_em: l.desfeito_em,
            })
            .collect())
    }
}
