//! Conclui a execução: `EmExecucao` → `Concluida`.
//!
//! `docs/modulos/os.md` §5 e §11.5: nunca deixa peça orçada sem consumo. Desde 2026-09-25 a
//! peça pendente é aplicada aqui mesmo (antes a conclusão era recusada e o técnico tinha de
//! voltar e clicar "Aplicar").

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

/// Conclui a execução.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ConcluirExecucao {
    /// A ordem de serviço.
    pub ordem_servico: Id,
}

#[cfg(feature = "sqlite")]
impl Comando for ConcluirExecucao {
    type Saida = ();
    const PERMISSAO: &'static str = "os.execucao.concluir";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let mut os = carregar_ordem(uow, self.ordem_servico)?;
        os.exigir_em_execucao().map_err(|e| Erro::de_dominio(&e))?;

        // Peça orçada que ninguém aplicou sai do estoque agora — o técnico não precisa de um
        // passo separado para isso (antes a conclusão era recusada).
        super::aplicar_peca::aplicar_pendentes_automaticamente(os.id, ctx, uow)?;

        os.concluir_execucao().map_err(|e| Erro::de_dominio(&e))?;
        RepositorioOs::novo(uow).atualizar_ordem(&os)?;
        Ok(())
    }
}
