//! Confirma um compromisso agendado.

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
use crate::eventos::CompromissoConfirmado;
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioAgenda;

/// Confirma um compromisso `Agendado`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ConfirmarCompromisso {
    /// O compromisso.
    pub compromisso: Id,
}

#[cfg(feature = "sqlite")]
impl Comando for ConfirmarCompromisso {
    type Saida = ();
    const PERMISSAO: &'static str = "agenda.compromisso.confirmar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut c = carregar_compromisso(uow, self.compromisso)?;

        // 2. Validar (domínio puro): Agendado -> Confirmado.
        c.confirmar().map_err(|e| Erro::de_dominio(&e))?;

        // 3. Persistir.
        RepositorioAgenda::novo(uow).atualizar_estado(&c)?;

        // 4. Publicar.
        uow.publicar(CompromissoConfirmado { compromisso: c.id })
            .map_err(|e| Erro::de_dominio(&e))?;

        Ok(())
    }
}
