//! Cancela um compromisso — nunca apaga (`docs/modulos/agenda.md` §11.3).

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
use crate::eventos::CompromissoCancelado;
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioAgenda;

/// Cancela um compromisso ativo (`Agendado`, `Confirmado` ou `EmAndamento`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CancelarCompromisso {
    /// O compromisso.
    pub compromisso: Id,
}

impl Comando for CancelarCompromisso {
    type Saida = ();
    const PERMISSAO: &'static str = "agenda.compromisso.cancelar";
    const RISCO: Risco = Risco::Medio;

    #[cfg(feature = "sqlite")]
    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut c = carregar_compromisso(uow, self.compromisso)?;

        // 2. Validar (domínio puro): nunca apaga, só marca Cancelado.
        c.cancelar().map_err(|e| Erro::de_dominio(&e))?;

        // 3. Persistir.
        RepositorioAgenda::novo(uow).atualizar_estado(&c)?;

        // 4. Publicar.
        uow.publicar(CompromissoCancelado { compromisso: c.id })
            .map_err(|e| Erro::de_dominio(&e))?;

        Ok(())
    }
}
