//! Os eventos de domínio que o módulo de PDV publica na outbox (`docs/modulos/pdv.md` §8).
//! Nomes versionados — assinantes casam pelo nome, nunca pelo tipo Rust.

use cardeal_kernel::{Dinheiro, Id};
use cardeal_storage::EventoDominio;
use serde::Serialize;

/// Uma venda de balcão foi finalizada.
#[derive(Debug, Serialize)]
pub struct VendaFinalizada {
    /// O cupom.
    pub cupom: Id,
    /// O terminal.
    pub terminal: Id,
    /// O total pago.
    pub total: Dinheiro,
}

impl EventoDominio for VendaFinalizada {
    const TIPO: &'static str = "pdv.venda_finalizada.v1";

    fn agregado(&self) -> Option<Id> {
        Some(self.cupom)
    }
}

/// Um cupom foi cancelado antes de finalizar.
#[derive(Debug, Serialize)]
pub struct CupomCancelado {
    /// O cupom.
    pub cupom: Id,
    /// O motivo informado.
    pub motivo: String,
    /// Quem autorizou o cancelamento.
    pub autorizado_por: Id,
}

impl EventoDominio for CupomCancelado {
    const TIPO: &'static str = "pdv.cupom_cancelado.v1";

    fn agregado(&self) -> Option<Id> {
        Some(self.cupom)
    }
}

/// Um item foi cancelado (`F7`).
#[derive(Debug, Serialize)]
pub struct ItemCancelado {
    /// O cupom.
    pub cupom: Id,
    /// O item.
    pub item: Id,
    /// O motivo informado.
    pub motivo: String,
}

impl EventoDominio for ItemCancelado {
    const TIPO: &'static str = "pdv.item_cancelado.v1";

    fn agregado(&self) -> Option<Id> {
        Some(self.cupom)
    }
}
