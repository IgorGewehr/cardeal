//! Conclui um compromisso em andamento.

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::carregar_compromisso;
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

    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let mut c = carregar_compromisso(uow, self.compromisso)?;
        c.concluir().map_err(|e| Erro::de_dominio(&e))?;
        RepositorioAgenda::novo(uow).atualizar_estado(&c)?;
        Ok(())
    }
}
