//! Confirma um compromisso agendado.

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::carregar_compromisso;
use crate::eventos::CompromissoConfirmado;
use crate::repositorio::RepositorioAgenda;

/// Confirma um compromisso `Agendado`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ConfirmarCompromisso {
    /// O compromisso.
    pub compromisso: Id,
}

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
