//! Cancela um orçamento (a partir de `Rascunho`/`Enviado`/`Aprovado`).

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::carregar_orcamento;
use crate::repositorio::RepositorioOrcamentos;

/// Cancela um orçamento.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelarOrcamento {
    /// O orçamento.
    pub orcamento: Id,
}

impl Comando for CancelarOrcamento {
    type Saida = ();
    const PERMISSAO: &'static str = "orcamentos.orcamento.cancelar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let mut orcamento = carregar_orcamento(uow, self.orcamento)?;
        orcamento.cancelar().map_err(|e| Erro::de_dominio(&e))?;
        RepositorioOrcamentos::novo(uow).atualizar_orcamento(&orcamento)?;
        Ok(())
    }
}
