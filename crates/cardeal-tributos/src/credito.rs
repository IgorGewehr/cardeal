//! O crédito da compra (spec §5.3): o imposto pago na compra que volta na apuração do regime
//! normal — sai do custo da mercadoria e vira "impostos a recuperar". Simples e MEI não creditam.
//!
//! - ICMS: o destacado na nota (ou o crédito que o fornecedor do Simples permite, `vCredICMSSN`),
//!   só se a **saída** do produto é tributada (CST 00, 10, 20, 70). Revendido com ST ou isento,
//!   o crédito não existe (ou teria de ser estornado).
//! - PIS/COFINS: só no não cumulativo (Lucro Real), 1,65% + 7,6% sobre o valor do item **sem o
//!   ICMS destacado** (Lei 14.592/2023), e nunca no monofásico.

use cardeal_kernel::{Arredondamento, Dinheiro, Percentual};
use serde::{Deserialize, Serialize};

use crate::{e_monofasico, PerfilTributario, Regime};

/// O que a nota de compra diz do item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CompraDoItem {
    /// O valor do item (produto − desconto): a base do crédito de PIS/COFINS.
    pub valor: Dinheiro,
    /// O ICMS próprio destacado (`vICMS`).
    pub icms_destacado: Dinheiro,
    /// O crédito do fornecedor do Simples (`vCredICMSSN`).
    pub credito_sn: Dinheiro,
}

/// O crédito de um item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CreditoDoItem {
    /// ICMS a recuperar.
    pub icms: Dinheiro,
    /// PIS a recuperar.
    pub pis: Dinheiro,
    /// COFINS a recuperar.
    pub cofins: Dinheiro,
}

impl CreditoDoItem {
    /// O total.
    #[must_use]
    pub fn total(&self) -> Dinheiro {
        self.icms + self.pis + self.cofins
    }
}

/// O crédito do item no regime da empresa, pelo perfil de **saída** do produto.
#[must_use]
pub fn credito_do_item(
    regime: &Regime,
    saida: &PerfilTributario,
    compra: &CompraDoItem,
) -> CreditoDoItem {
    let (normal, nao_cumulativo) = match *regime {
        Regime::Presumido { .. } => (true, false),
        Regime::Real {
            pis_cofins_nao_cumulativo,
        } => (true, pis_cofins_nao_cumulativo),
        Regime::Mei | Regime::Simples { .. } | Regime::Manual(_) => (false, false),
    };
    if !normal {
        return CreditoDoItem::default();
    }
    let tributada = matches!(
        saida.situacao_icms.as_deref(),
        Some("00" | "10" | "20" | "70")
    );
    let icms = if tributada {
        compra.icms_destacado + compra.credito_sn
    } else {
        Dinheiro::ZERO
    };
    let (pis, cofins) = if nao_cumulativo && !e_monofasico(saida) {
        // A base é o valor do item sem o ICMS destacado (Lei 14.592/2023).
        let base = compra.valor - compra.icms_destacado;
        let parte = |p: Percentual| base.aplicar(p, Arredondamento::MeioAcima);
        (
            parte(Percentual::milesimos(1_650)),
            parte(Percentual::milesimos(7_600)),
        )
    } else {
        (Dinheiro::ZERO, Dinheiro::ZERO)
    };
    CreditoDoItem { icms, pis, cofins }
}
