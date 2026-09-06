//! Os eventos de domínio que o módulo de vendas publica na outbox
//! (`docs/modulos/vendas.md` §8). Nomes versionados — assinantes casam pelo nome, nunca pelo
//! tipo Rust.

use cardeal_kernel::{Dinheiro, Id};
use cardeal_storage::EventoDominio;
use serde::Serialize;

/// Um pedido foi confirmado (a reserva de estoque em si é feita pelo comando).
#[derive(Debug, Serialize)]
pub struct PedidoConfirmado {
    /// O pedido.
    pub pedido: Id,
}

impl EventoDominio for PedidoConfirmado {
    const TIPO: &'static str = "vendas.pedido_confirmado.v1";

    fn agregado(&self) -> Option<Id> {
        Some(self.pedido)
    }
}

/// Um pedido foi faturado.
#[derive(Debug, Serialize)]
pub struct PedidoFaturado {
    /// O pedido.
    pub pedido: Id,
    /// O cliente.
    pub cliente: Id,
    /// O total faturado.
    pub total: Dinheiro,
}

impl EventoDominio for PedidoFaturado {
    const TIPO: &'static str = "vendas.pedido_faturado.v1";

    fn agregado(&self) -> Option<Id> {
        Some(self.pedido)
    }
}

/// Um pedido foi cancelado antes de faturar.
#[derive(Debug, Serialize)]
pub struct PedidoCancelado {
    /// O pedido.
    pub pedido: Id,
}

impl EventoDominio for PedidoCancelado {
    const TIPO: &'static str = "vendas.pedido_cancelado.v1";

    fn agregado(&self) -> Option<Id> {
        Some(self.pedido)
    }
}
