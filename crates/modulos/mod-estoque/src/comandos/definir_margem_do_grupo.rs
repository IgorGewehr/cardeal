//! Define a margem líquida desejada de um grupo (herdada por subgrupos e produtos que não
//! tenham a sua). `None` volta a herdar do grupo-pai.

#[cfg(feature = "sqlite")]
use cardeal_kernel::{CodigoErro, Erro, Resultado};
use cardeal_kernel::{Id, Percentual};
use cardeal_modkit::Comando;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioEstoque;
#[cfg(feature = "sqlite")]
use crate::repositorio_preco as sql;

/// Define a margem do grupo.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DefinirMargemDoGrupo {
    /// O grupo.
    pub grupo: Id,
    /// A margem; `None` limpa.
    pub margem: Option<Percentual>,
}

impl Comando for DefinirMargemDoGrupo {
    type Saida = ();
    const PERMISSAO: &'static str = "estoque.produto.editar";
    const RISCO: Risco = Risco::Medio;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        validar_margem(self.margem)?;
        let repo = RepositorioEstoque::novo(uow);
        if !sql::definir_margem_do_grupo(repo.conn(), ctx.organizacao, self.grupo, self.margem)? {
            return Err(Erro::nao_encontrado("grupo de produto"));
        }
        Ok(())
    }
}

/// Margem líquida tem que caber entre 0% e 100% (exclusive: 100% não deixa preço possível).
#[cfg(feature = "sqlite")]
pub(crate) fn validar_margem(m: Option<Percentual>) -> Resultado<()> {
    match m {
        Some(p) if !p.e_fracao_valida() || p == Percentual::CEM => Err(Erro::novo(
            CodigoErro::FORA_DA_FAIXA,
            "a margem desejada precisa estar entre 0% e 100%",
        )),
        _ => Ok(()),
    }
}
