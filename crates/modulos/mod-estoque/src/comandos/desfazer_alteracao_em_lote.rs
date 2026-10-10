//! Desfaz a última alteração em lote (até 24 horas depois). Restaura só os produtos que
//! ainda estão como a alteração os deixou: quem foi editado depois mantém a edição mais nova.

use cardeal_kernel::Id;
#[cfg(feature = "sqlite")]
use cardeal_kernel::{CodigoErro, Erro, Resultado};
use cardeal_modkit::Comando;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::alteracao_em_lote::pode_desfazer;
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioEstoque;
#[cfg(feature = "sqlite")]
use crate::repositorio_lote as sql;

/// Desfaz a alteração em lote.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DesfazerAlteracaoEmLote {
    /// A alteração.
    pub lote: Id,
}

/// O que foi restaurado.
#[derive(Debug, Serialize, Deserialize)]
pub struct AlteracaoDesfeita {
    /// Produtos que voltaram ao que eram.
    pub restaurados: u32,
    /// Produtos que foram editados depois e ficaram como estão.
    pub ignorados: u32,
}

impl Comando for DesfazerAlteracaoEmLote {
    type Saida = AlteracaoDesfeita;
    const PERMISSAO: &'static str = "estoque.produto.editar";
    const RISCO: Risco = Risco::Alto;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let repo = RepositorioEstoque::novo(uow);
        let conn = repo.conn();
        let registro = sql::lote_por_id(conn, ctx.organizacao, self.lote)?
            .ok_or_else(|| Erro::nao_encontrado("alteração em lote"))?;
        let e_a_ultima = sql::ultimo_lote_nao_desfeito(conn, ctx.organizacao)? == Some(self.lote);
        pode_desfazer(
            registro.criado_em,
            ctx.agora,
            registro.desfeito_em.is_some(),
            e_a_ultima,
        )
        .map_err(|m| Erro::novo(CodigoErro::REGRA_VIOLADA, m.to_string()))?;

        let (mut restaurados, mut ignorados) = (0_u32, 0_u32);
        for (produto, antes, depois) in sql::itens_do_lote(conn, self.lote)? {
            let atual = sql::produto_do_lote(conn, ctx.organizacao, produto)?;
            if let Some(a) = atual.filter(|a| a.valores == depois) {
                let mut restaurar = antes;
                if restaurar.ncm.is_empty() {
                    restaurar.ncm = a.valores.ncm;
                }
                sql::gravar_valores(conn, ctx.organizacao, produto, &restaurar)?;
                restaurados += 1;
            } else {
                ignorados += 1;
            }
        }
        sql::marcar_desfeito(conn, self.lote, ctx.agora)?;
        Ok(AlteracaoDesfeita {
            restaurados,
            ignorados,
        })
    }
}
