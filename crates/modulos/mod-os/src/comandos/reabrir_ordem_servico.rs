//! Reabre uma ordem de serviço cancelada: `Cancelada` → `Aberta`.
//!
//! `docs/modulos/os.md` §4 e §5. Uma OS cancelada nunca chegou a ser faturada (`cancelar` só
//! é aceito antes de `Concluida`/`Faturada`), então não há financeiro para reverter aqui —
//! laudo/orçamento/itens já gravados continuam intactos, só o estado destrava de novo.

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
use crate::eventos::OrdemReaberta;
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioOs;

/// Reabre uma ordem de serviço cancelada.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ReabrirOrdemServico {
    /// A ordem de serviço.
    pub ordem_servico: Id,
}

impl Comando for ReabrirOrdemServico {
    type Saida = ();
    const PERMISSAO: &'static str = "os.ordem.reabrir";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut os = carregar_ordem(uow, self.ordem_servico)?;

        // 2. Validar (domínio puro): transiciona Cancelada -> Aberta.
        os.reabrir().map_err(|e| Erro::de_dominio(&e))?;

        // 3. Persistir.
        RepositorioOs::novo(uow).atualizar_ordem(&os)?;

        // 4. Publicar.
        uow.publicar(OrdemReaberta {
            ordem_servico: os.id,
        })
        .map_err(|e| Erro::de_dominio(&e))?;

        Ok(())
    }
}
