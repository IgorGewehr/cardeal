//! A venda para outro estado (spec §5.6) — só simulação: o preço sugerido é sempre o da venda
//! interna ao consumidor. Resolução SF 22/1989 (7% e 12%) e 13/2012 (4% do importado); DIFAL ao
//! não contribuinte pela EC 87/2015 e LC 190/2022, com a base dupla. No Simples o ICMS está no
//! DAS e o DIFAL do remetente do Simples não é cobrado (STF, ADI 5.464): a carga não muda.

use cardeal_kernel::Percentual;
use serde::{Deserialize, Serialize};

use crate::{carga, Carga, Imposto, Natureza, PerfilTributario, Regime};

/// Para onde vai a venda.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Destino {
    /// Dentro do estado, ao consumidor (o preço sugerido).
    InternoConsumidor,
    /// Para outro estado.
    Interestadual {
        /// A alíquota interestadual (ver [`aliquota_interestadual`]).
        interestadual: Percentual,
        /// A alíquota interna do destino quando o comprador **não** é contribuinte (DIFAL);
        /// `None` para a venda a empresa contribuinte.
        interna_destino: Option<Percentual>,
    },
}

const SUL_SUDESTE_SEM_ES: [&str; 6] = ["MG", "PR", "RJ", "RS", "SC", "SP"];

/// A alíquota interestadual: 4% para o importado (origem 1, 2, 3 ou 8); 7% do Sul e Sudeste
/// (menos o ES) para o Norte, Nordeste, Centro-Oeste e ES; 12% nas demais.
#[must_use]
pub fn aliquota_interestadual(
    origem_uf: &str,
    destino_uf: &str,
    origem_mercadoria: u8,
) -> Percentual {
    if matches!(origem_mercadoria, 1 | 2 | 3 | 8) {
        return Percentual::pontos(4);
    }
    let de = origem_uf.trim().to_ascii_uppercase();
    let para = destino_uf.trim().to_ascii_uppercase();
    if SUL_SUDESTE_SEM_ES.contains(&de.as_str()) && !SUL_SUDESTE_SEM_ES.contains(&para.as_str()) {
        Percentual::pontos(7)
    } else {
        Percentual::pontos(12)
    }
}

/// A carga da venda no destino.
///
/// # Errors
/// O mesmo de [`carga`].
pub fn carga_no_destino(
    regime: &Regime,
    perfil: &PerfilTributario,
    destino: &Destino,
) -> Result<Carga, String> {
    let Destino::Interestadual {
        interestadual,
        interna_destino,
    } = *destino
    else {
        return carga(regime, perfil);
    };
    let normal = matches!(regime, Regime::Presumido { .. } | Regime::Real { .. });
    let tributado = matches!(
        perfil.situacao_icms.as_deref(),
        Some("00" | "20" | "10" | "70")
    );
    if !normal || !tributado {
        return carga(regime, perfil);
    }
    let fora = PerfilTributario {
        aliquota_icms: Some(interestadual),
        reducao_base: None,
        aliquota_fcp: None,
        ..perfil.clone()
    };
    let mut c = carga(regime, &fora)?;
    if let Some(destino) = interna_destino.filter(|d| *d > interestadual) {
        // DIFAL por dentro, base dupla: base = P(1 − o) ÷ (1 − d); DIFAL = base × d − P × o.
        let un = i128::from(interestadual.complemento().unidades_internas())
            * i128::from(destino.unidades_internas())
            / i128::from(destino.complemento().unidades_internas());
        let difal = Percentual::unidades(i64::try_from(un).unwrap_or(0)) - interestadual;
        c.termos.push(crate::Termo {
            imposto: Imposto::Difal,
            natureza: Natureza::PorDentro,
            coeficiente: difal,
            explicacao: format!(
                "DIFAL ao consumidor de outro estado: interna de {} menos a interestadual de {}",
                destino.formatar_com_simbolo(),
                interestadual.formatar_com_simbolo()
            ),
        });
    }
    Ok(c)
}
