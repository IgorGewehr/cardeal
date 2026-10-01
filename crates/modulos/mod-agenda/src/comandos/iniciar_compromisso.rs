//! Inicia um compromisso confirmado.

use cardeal_kernel::Id;
#[cfg(feature = "sqlite")]
use cardeal_kernel::{Erro, Resultado};
#[cfg(feature = "sqlite")]
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_modkit::{Comando, Ctx};
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::comandos::carregar_compromisso;
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioAgenda;

/// Inicia um compromisso `Confirmado`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct IniciarCompromisso {
    /// O compromisso.
    pub compromisso: Id,
}

#[cfg(feature = "sqlite")]
impl Comando for IniciarCompromisso {
    type Saida = ();
    const PERMISSAO: &'static str = "agenda.compromisso.confirmar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let mut c = carregar_compromisso(uow, self.compromisso)?;
        c.iniciar().map_err(|e| Erro::de_dominio(&e))?;
        RepositorioAgenda::novo(uow).atualizar_estado(&c)?;
        Ok(())
    }
}
