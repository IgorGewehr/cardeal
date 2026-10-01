//! Conclui um compromisso em andamento.

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
use crate::comandos::carregar_compromisso;
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioAgenda;

/// Conclui um compromisso `EmAndamento`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ConcluirCompromisso {
    /// O compromisso.
    pub compromisso: Id,
}

impl Comando for ConcluirCompromisso {
    type Saida = ();
    const PERMISSAO: &'static str = "agenda.compromisso.confirmar";
    const RISCO: Risco = Risco::Baixo;

    #[cfg(feature = "sqlite")]
    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let mut c = carregar_compromisso(uow, self.compromisso)?;
        c.concluir().map_err(|e| Erro::de_dominio(&e))?;
        RepositorioAgenda::novo(uow).atualizar_estado(&c)?;
        Ok(())
    }
}
