//! Cancela um pedido ainda não faturado. `docs/modulos/vendas.md` §5 e §11.3 — pedido
//! faturado não se cancela (a correção é devolução); o domínio já recusa isso.

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::carregar_pedido;
use crate::eventos::PedidoCancelado;
use crate::repositorio::RepositorioVendas;

/// Cancela um pedido `Rascunho`/`Confirmado`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CancelarPedido {
    /// O pedido.
    pub pedido: Id,
}

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
