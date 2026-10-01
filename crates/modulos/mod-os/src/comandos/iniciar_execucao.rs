//! Inicia a execução de uma ordem de serviço aprovada: `Aprovada` → `EmExecucao`.

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
use crate::comandos::carregar_ordem;
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioOs;

/// Inicia a execução.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct IniciarExecucao {
    /// A ordem de serviço.
    pub ordem_servico: Id,
}

impl Comando for IniciarExecucao {
    type Saida = ();
    const PERMISSAO: &'static str = "os.execucao.iniciar";
    const RISCO: Risco = Risco::Baixo;

    #[cfg(feature = "sqlite")]
    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let mut os = carregar_ordem(uow, self.ordem_servico)?;
        os.iniciar_execucao().map_err(|e| Erro::de_dominio(&e))?;
        RepositorioOs::novo(uow).atualizar_ordem(&os)?;
        Ok(())
    }
}
