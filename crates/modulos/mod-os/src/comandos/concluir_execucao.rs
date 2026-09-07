//! Conclui a execução: `EmExecucao` → `Concluida`.
//!
//! `docs/modulos/os.md` §5 e §11.5: recusa se ainda houver peça do orçamento pendente de
//! aplicação — nunca deixa um consumo órfão sem contrapartida.

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::carregar_ordem;
use crate::erros::ErroOs;
use crate::repositorio::RepositorioOs;

/// Conclui a execução.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ConcluirExecucao {
    /// A ordem de serviço.
    pub ordem_servico: Id,
}

impl Comando for ConcluirExecucao {
    type Saida = ();
    const PERMISSAO: &'static str = "os.execucao.concluir";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let mut os = carregar_ordem(uow, self.ordem_servico)?;
        os.exigir_em_execucao().map_err(|e| Erro::de_dominio(&e))?;

        let pendentes = RepositorioOs::novo(uow).contar_pecas_nao_aplicadas(os.id)?;
        if pendentes > 0 {
            return Err(Erro::de_dominio(&ErroOs::PecaPendenteDeAplicacao(
                pendentes,
            )));
        }

        os.concluir_execucao().map_err(|e| Erro::de_dominio(&e))?;
        RepositorioOs::novo(uow).atualizar_ordem(&os)?;
        Ok(())
    }
}
