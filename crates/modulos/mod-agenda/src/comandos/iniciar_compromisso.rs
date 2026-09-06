//! Inicia um compromisso confirmado.

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::carregar_compromisso;
use crate::repositorio::RepositorioAgenda;

/// Inicia um compromisso `Confirmado`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct IniciarCompromisso {
    /// O compromisso.
    pub compromisso: Id,
}

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
