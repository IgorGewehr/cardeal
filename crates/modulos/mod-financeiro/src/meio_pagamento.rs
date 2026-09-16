//! O meio de pagamento escolhido numa baixa — decide se o dinheiro cai no Caixa físico ou na
//! conta bancária.
//!
//! Pedido explícito do usuário (2026-09-15): "se for dinheiro cai na conta caixa, se não cai
//! na conta bancária". Pix e cartão são tratados igual de propósito — o app não modela o
//! "valores em trânsito" do Pix nem o prazo de compensação da adquirente de cartão; a baixa
//! só acontece quando o dinheiro já está confirmado (baixa é sempre sobre dinheiro que já
//! entrou/saiu — títulos em aberto, sem baixa, nunca tocam Caixa/Bancos), então cai direto na
//! conta bancária. Não há coluna própria em `financeiro_baixa` para isso: a conta de fato
//! movimentada (Caixa ou Bancos) já fica registrada no lançamento do razão que a baixa gera,
//! então o meio de pagamento é só o critério de roteamento, não um dado novo a persistir.

use cardeal_ledger::PapelConta;
use serde::{Deserialize, Serialize};

/// Como o dinheiro de uma baixa entrou (recebimento) ou saiu (pagamento).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MeioPagamento {
    /// Papel-moeda — vai para o Caixa físico.
    Dinheiro,
    /// Pix — direto para a conta bancária.
    Pix,
    /// Cartão (débito ou crédito) — direto para a conta bancária, mesma simplificação do Pix.
    Cartao,
}

impl MeioPagamento {
    /// A conta do razão que recebe/paga por este meio.
    #[must_use]
    pub const fn papel(self) -> PapelConta {
        match self {
            Self::Dinheiro => PapelConta::Caixa,
            Self::Pix | Self::Cartao => PapelConta::Bancos,
        }
    }

    /// Rótulo em português, para a UI.
    #[must_use]
    pub const fn rotulo(self) -> &'static str {
        match self {
            Self::Dinheiro => "Dinheiro",
            Self::Pix => "Pix",
            Self::Cartao => "Cartão",
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn dinheiro_vai_pro_caixa_pix_e_cartao_vao_pro_banco() {
        assert_eq!(MeioPagamento::Dinheiro.papel(), PapelConta::Caixa);
        assert_eq!(MeioPagamento::Pix.papel(), PapelConta::Bancos);
        assert_eq!(MeioPagamento::Cartao.papel(), PapelConta::Bancos);
    }
}
