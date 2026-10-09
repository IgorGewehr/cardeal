//! Define como o grupo é tributado no Simples: ICMS por substituição tributária e/ou
//! PIS/COFINS monofásico (cada um tira a sua parcela do DAS). `None` herda do grupo-pai.

use cardeal_kernel::Id;
#[cfg(feature = "sqlite")]
use cardeal_kernel::{Erro, Resultado};
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

/// Define o perfil fiscal do grupo.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DefinirPerfilFiscalDoGrupo {
    /// O grupo.
    pub grupo: Id,
    /// ICMS-ST; `None` herda.
    pub icms_st: Option<bool>,
    /// PIS/COFINS monofásico; `None` herda.
    pub monofasico: Option<bool>,
}

impl Comando for DefinirPerfilFiscalDoGrupo {
    type Saida = ();
    const PERMISSAO: &'static str = "estoque.tributario.editar";
    const RISCO: Risco = Risco::Alto;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let repo = RepositorioEstoque::novo(uow);
        if !sql::definir_perfil_fiscal_do_grupo(
            repo.conn(),
            ctx.organizacao,
            self.grupo,
            self.icms_st,
            self.monofasico,
        )? {
            return Err(Erro::nao_encontrado("grupo de produto"));
        }
        Ok(())
    }
}
