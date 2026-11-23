//! "Como este produto é tributado": a tributação em palavras do dono, sem CST. Cada opção
//! grava os campos fiscais certos para o regime da empresa (CSOSN no Simples, CST no normal);
//! quem precisa de outra combinação usa "Avançado" (os campos completos, como antes).
//! Domínio puro — spec `2026-11-23-markup-por-carga-tributaria-design.md` §5.2.

use cardeal_kernel::Percentual;
use serde::{Deserialize, Serialize};

use crate::fiscal::CamposFiscais;

/// As opções prontas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpcaoTributacao {
    /// Mercadoria comprada para revender, tributada normalmente.
    RevendaComum,
    /// O ICMS já foi pago pelo fornecedor (substituição tributária).
    RevendaComSt,
    /// PIS/COFINS recolhidos pelo fabricante (bebidas, higiene, autopeças...).
    Monofasico,
    /// As duas coisas.
    StEMonofasico,
    /// Isento ou não tributado pelo ICMS.
    IsentoOuNaoTributado,
    /// Os campos fiscais completos.
    Avancado,
}

impl OpcaoTributacao {
    /// As opções na ordem da tela.
    pub const TODAS: [Self; 6] = [
        Self::RevendaComum,
        Self::RevendaComSt,
        Self::Monofasico,
        Self::StEMonofasico,
        Self::IsentoOuNaoTributado,
        Self::Avancado,
    ];

    /// O texto da tela.
    #[must_use]
    pub const fn rotulo(self) -> &'static str {
        match self {
            Self::RevendaComum => "Revenda comum",
            Self::RevendaComSt => "Revenda com ICMS já pago pelo fornecedor (ST)",
            Self::Monofasico => "PIS/COFINS monofásico (bebidas, higiene, autopeças…)",
            Self::StEMonofasico => "ICMS já pago (ST) e PIS/COFINS monofásico",
            Self::IsentoOuNaoTributado => "Isento ou não tributado",
            Self::Avancado => "Avançado (campos fiscais completos)",
        }
    }

    /// Se a opção pede a alíquota de ICMS no regime normal.
    #[must_use]
    pub const fn pede_aliquota(self, simples: bool) -> bool {
        !simples && matches!(self, Self::RevendaComum | Self::Monofasico)
    }
}

/// Os campos que a opção grava. No regime normal, a alíquota de ICMS só entra se confirmada
/// (`aliquota_icms_confirmada`): ela muda por produto (cesta básica, bebidas), então nunca é
/// gravada sozinha. `Avancado` devolve vazio — quem chama mantém o que já estava.
#[must_use]
pub fn campos_da_opcao(
    opcao: OpcaoTributacao,
    simples: bool,
    aliquota_icms_confirmada: Option<Percentual>,
) -> CamposFiscais {
    use OpcaoTributacao as O;
    let (st, mono) = match opcao {
        O::RevendaComum => (false, false),
        O::RevendaComSt => (true, false),
        O::Monofasico => (false, true),
        O::StEMonofasico => (true, true),
        O::IsentoOuNaoTributado => {
            return CamposFiscais {
                cst_icms: Some(if simples { "400" } else { "40" }.into()),
                ..CamposFiscais::default()
            }
        }
        O::Avancado => return CamposFiscais::default(),
    };
    let cst_icms = match (simples, st) {
        (true, false) => "102",
        (true, true) => "500",
        (false, false) => "00",
        (false, true) => "60",
    };
    let pis = if mono {
        Some("04".to_owned())
    } else if simples {
        None
    } else {
        Some("01".to_owned())
    };
    CamposFiscais {
        cst_icms: Some(cst_icms.into()),
        aliquota_icms: if opcao.pede_aliquota(simples) {
            aliquota_icms_confirmada
        } else {
            None
        },
        cst_pis: pis.clone(),
        cst_cofins: pis,
        ..CamposFiscais::default()
    }
}

/// Qual opção os campos representam (para mostrar o seletor já marcado); o que não casa com
/// nenhuma é `Avancado`.
#[must_use]
pub fn opcao_dos_campos(c: &CamposFiscais, simples: bool) -> OpcaoTributacao {
    use OpcaoTributacao as O;
    let mono = c.cst_pis.as_deref() == Some("04") && c.cst_cofins.as_deref() == Some("04");
    match (simples, c.cst_icms.as_deref(), mono) {
        (true, Some("102"), false) | (false, Some("00"), false) => O::RevendaComum,
        (true, Some("500"), false) | (false, Some("60"), false) => O::RevendaComSt,
        (true, Some("102"), true) | (false, Some("00"), true) => O::Monofasico,
        (true, Some("500"), true) | (false, Some("60"), true) => O::StEMonofasico,
        (true, Some("300" | "400"), _) | (false, Some("40" | "41"), _) => O::IsentoOuNaoTributado,
        _ => O::Avancado,
    }
}

/// A alíquota interna modal do ICMS da UF — só a **sugestão** que o usuário confirma no regime
/// normal; produtos com alíquota própria (cesta básica, bebidas, combustíveis) e o FCP ficam por
/// conta do perfil. Leis estaduais vigentes em 2025; **conferir com o contador** a cada ano.
#[must_use]
pub fn aliquota_interna(uf: &str) -> Option<Percentual> {
    let milesimos = match uf.trim().to_ascii_uppercase().as_str() {
        "ES" | "MS" | "MT" | "RS" | "SC" => 17_000,
        "AP" | "MG" | "SP" => 18_000,
        "AC" | "AL" | "GO" | "PA" => 19_000,
        "PR" | "RO" => 19_500,
        "AM" | "CE" | "DF" | "PB" | "RJ" | "RN" | "RR" | "SE" | "TO" => 20_000,
        "BA" | "PE" => 20_500,
        "PI" => 22_500,
        "MA" => 23_000,
        _ => return None,
    };
    Some(Percentual::milesimos(milesimos))
}
