//! Cancela a ordem de serviço antes de concluída: `Aberta`/`EmDiagnostico`/
//! `AguardandoAprovacao` → `Cancelada` (terminal). `docs/modulos/os.md` §4 e §11 regra 5.

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::carregar_ordem;
use crate::eventos::OrdemCancelada;
use crate::repositorio::RepositorioOs;

/// Cancela a ordem de serviço.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CancelarOrdemServico {
    /// A ordem de serviço.
    pub ordem_servico: Id,
}

impl Comando for CancelarOrdemServico {
    type Saida = ();
    const PERMISSAO: &'static str = "os.ordem.cancelar";
    const RISCO: Risco = Risco::Medio;

    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut os = carregar_ordem(uow, self.ordem_servico)?;

        // 2. Validar (domínio puro).
        os.cancelar().map_err(|e| Erro::de_dominio(&e))?;

        // 4. Persistir.
        RepositorioOs::novo(uow).atualizar_ordem(&os)?;

        // 5. Publicar.
        uow.publicar(OrdemCancelada {
            ordem_servico: os.id,
        })
        .map_err(|e| Erro::de_dominio(&e))?;

        Ok(())
    }
}
