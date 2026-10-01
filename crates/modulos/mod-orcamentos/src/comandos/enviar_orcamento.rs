//! `Rascunho → Enviado` — o orçamento vai para o cliente.

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

/// Envia o orçamento ao cliente.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnviarOrcamento {
    /// O orçamento.
    pub orcamento: Id,
}

impl Comando for EnviarOrcamento {
    type Saida = ();
    const PERMISSAO: &'static str = "orcamentos.orcamento.enviar";
    const RISCO: Risco = Risco::Baixo;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let mut orcamento = carregar_orcamento(uow, self.orcamento)?;
        let quantidade = RepositorioOrcamentos::novo(uow)
            .itens_do_orcamento(orcamento.id)?
            .len();

        orcamento
            .enviar(quantidade, ctx.hoje())
            .map_err(|e| Erro::de_dominio(&e))?;

        RepositorioOrcamentos::novo(uow).atualizar_orcamento(&orcamento)?;
        Ok(())
    }
}
