//! Envia o orçamento montado para aprovação do cliente.

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::carregar_ordem;
use crate::repositorio::RepositorioOs;

/// Envia o orçamento para aprovação.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EnviarParaAprovacao {
    /// A ordem de serviço.
    pub ordem_servico: Id,
}

impl Comando for EnviarParaAprovacao {
    type Saida = ();
    const PERMISSAO: &'static str = "os.orcamento.enviar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let mut os = carregar_ordem(uow, self.ordem_servico)?;
        os.enviar_para_aprovacao()
            .map_err(|e| Erro::de_dominio(&e))?;
        RepositorioOs::novo(uow).atualizar_ordem(&os)?;
        Ok(())
    }
}
