//! Cancela um compromisso — nunca apaga (`docs/modulos/agenda.md` §11.3).

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::carregar_compromisso;
use crate::eventos::CompromissoCancelado;
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
