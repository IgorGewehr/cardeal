//! Inicia a execução de uma ordem de serviço aprovada: `Aprovada` → `EmExecucao`.

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::carregar_ordem;
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

    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let mut os = carregar_ordem(uow, self.ordem_servico)?;
        os.iniciar_execucao().map_err(|e| Erro::de_dominio(&e))?;
        RepositorioOs::novo(uow).atualizar_ordem(&os)?;
        Ok(())
    }
}
