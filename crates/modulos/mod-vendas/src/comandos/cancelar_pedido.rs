//! Cancela um pedido ainda não faturado. `docs/modulos/vendas.md` §5 e §11.3 — pedido
//! faturado não se cancela (a correção é devolução); o domínio já recusa isso.

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
use crate::comandos::carregar_pedido;
#[cfg(feature = "sqlite")]
use crate::eventos::PedidoCancelado;
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioVendas;

/// Cancela um pedido `Rascunho`/`Confirmado`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CancelarPedido {
    /// O pedido.
    pub pedido: Id,
}

#[cfg(feature = "sqlite")]
impl Comando for CancelarPedido {
    type Saida = ();
    const PERMISSAO: &'static str = "vendas.pedido.cancelar";
    const RISCO: Risco = Risco::Medio;

    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut pedido = carregar_pedido(uow, self.pedido)?;

        // 2. Validar (domínio puro).
        pedido.cancelar().map_err(|e| Erro::de_dominio(&e))?;

        // 4. Persistir.
        RepositorioVendas::novo(uow).atualizar_pedido(&pedido)?;

        // 5. Publicar.
        uow.publicar(PedidoCancelado { pedido: pedido.id })
            .map_err(|e| Erro::de_dominio(&e))?;

        Ok(())
    }
}
