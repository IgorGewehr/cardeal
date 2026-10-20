//! Os eventos de domínio que o módulo de compras publica na outbox
//! (`docs/modulos/compras.md` §8). Nomes versionados — assinantes casam pelo nome, nunca
//! pelo tipo Rust.

use cardeal_kernel::{Dinheiro, Id};
#[cfg(feature = "sqlite")]
use cardeal_storage::EventoDominio;
use serde::Serialize;

/// Uma nota de entrada chegou e está aguardando conferência (todo item precisa de
/// casamento antes de confirmar).
#[derive(Debug, Serialize)]
pub struct EntradaAConferir {
    /// A nota.
    pub nota_entrada: Id,
    /// O fornecedor.
    pub fornecedor: Id,
    /// Quantos itens vieram na nota.
    pub total_itens: usize,
}

#[cfg(feature = "sqlite")]
impl EventoDominio for EntradaAConferir {
    const TIPO: &'static str = "compras.entrada_a_conferir.v1";

    fn agregado(&self) -> Option<Id> {
        Some(self.nota_entrada)
    }
}

/// Uma nota de entrada foi confirmada — estoque atualizado, título a pagar gerado se a
/// preferência mandar.
#[derive(Debug, Serialize)]
pub struct NotaConfirmada {
    /// A nota.
    pub nota_entrada: Id,
    /// O fornecedor.
    pub fornecedor: Id,
    /// O total da nota.
    pub valor_total: Dinheiro,
    /// O título a pagar gerado, se a preferência mandou gerar um.
    pub titulo: Option<Id>,
    /// Verdadeiro se `titulo` já nasceu com baixa completa (compra paga à vista, na hora —
    /// `ConfirmarEntrada::pago_no_ato`). Sempre `false` quando `titulo` é `None`.
    pub pago: bool,
}

#[cfg(feature = "sqlite")]
impl EventoDominio for NotaConfirmada {
    const TIPO: &'static str = "compras.nota_confirmada.v1";

    fn agregado(&self) -> Option<Id> {
        Some(self.nota_entrada)
    }
}

/// A entrada de uma nota foi estornada: a mercadoria saiu do estoque ao custo da nota, a conta
/// a pagar foi cancelada (e o pagamento estornado, se havia) e a nota voltou à conferência.
#[derive(Debug, Serialize)]
pub struct EntradaEstornada {
    /// A nota.
    pub nota_entrada: Id,
    /// O fornecedor.
    pub fornecedor: Id,
    /// O total da nota.
    pub valor_total: Dinheiro,
    /// O título a pagar cancelado, se havia.
    pub titulo: Option<Id>,
    /// O pagamento estornado junto (zero se não havia).
    pub pagamento_estornado: Dinheiro,
    /// Por quê.
    pub motivo: String,
}

#[cfg(feature = "sqlite")]
impl EventoDominio for EntradaEstornada {
    const TIPO: &'static str = "compras.entrada_estornada.v1";

    fn agregado(&self) -> Option<Id> {
        Some(self.nota_entrada)
    }
}
