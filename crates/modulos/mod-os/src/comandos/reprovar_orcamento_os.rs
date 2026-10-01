//! Registra a reprovação do orçamento pelo cliente: `AguardandoAprovacao` → `Reprovada`
//! (terminal).

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
use crate::comandos::carregar_ordem;
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioOs;

/// Registra a reprovação do orçamento.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ReprovarOrcamentoOs {
    /// A ordem de serviço.
    pub ordem_servico: Id,
}

#[cfg(feature = "sqlite")]
impl Comando for ReprovarOrcamentoOs {
    type Saida = ();
    const PERMISSAO: &'static str = "os.orcamento.aprovar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let mut os = carregar_ordem(uow, self.ordem_servico)?;
        os.reprovar().map_err(|e| Erro::de_dominio(&e))?;
        RepositorioOs::novo(uow).atualizar_ordem(&os)?;
        Ok(())
    }
}
