//! Registra a reprovação do orçamento pelo cliente: `AguardandoAprovacao` → `Reprovada`
//! (terminal).

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::carregar_ordem;
use crate::repositorio::RepositorioOs;

/// Registra a reprovação do orçamento.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ReprovarOrcamentoOs {
    /// A ordem de serviço.
    pub ordem_servico: Id,
}

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
