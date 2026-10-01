//! Cancela um orçamento (a partir de `Rascunho`/`Enviado`/`Aprovado`).

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
use crate::comandos::carregar_orcamento;
#[cfg(feature = "sqlite")]
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

    #[cfg(feature = "sqlite")]
    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let mut orcamento = carregar_orcamento(uow, self.orcamento)?;
        orcamento.cancelar().map_err(|e| Erro::de_dominio(&e))?;
        RepositorioOrcamentos::novo(uow).atualizar_orcamento(&orcamento)?;
        Ok(())
    }
}
