//! Confirma o pedido: `Rascunho → Confirmado`. A reserva de estoque em si é responsabilidade
//! de um comando de `estoque` — este comando só faz a transição (§11.2).

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
use crate::comandos::carregar_pedido;
#[cfg(feature = "sqlite")]
use crate::eventos::PedidoConfirmado;
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioVendas;

/// Confirma um pedido em `Rascunho`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ConfirmarPedido {
    /// O pedido.
    pub pedido: Id,
}

impl Comando for ConfirmarPedido {
    type Saida = ();
    const PERMISSAO: &'static str = "vendas.pedido.confirmar";
    const RISCO: Risco = Risco::Medio;

    #[cfg(feature = "sqlite")]
    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut pedido = carregar_pedido(uow, self.pedido)?;

        // 2. Validar (domínio puro).
        pedido.confirmar().map_err(|e| Erro::de_dominio(&e))?;

        // 4. Persistir.
        RepositorioVendas::novo(uow).atualizar_pedido(&pedido)?;

        // 5. Publicar.
        uow.publicar(PedidoConfirmado { pedido: pedido.id })
            .map_err(|e| Erro::de_dominio(&e))?;

        Ok(())
    }
}
