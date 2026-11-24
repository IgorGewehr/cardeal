//! A projeção da reforma tributária (EC 132/2023, LC 214/2025) — **só informativa**, nunca muda
//! preço sozinha (spec §5.7). O cronograma:
//!
//! - 2026: CBS 0,9% e IBS 0,1% de teste, compensáveis — sem custo para o regime normal;
//! - 2027–2028: CBS cheia (menos 0,1 do IBS de teste), IBS 0,1%; fim do PIS/COFINS e do IPI
//!   (salvo Zona Franca);
//! - 2029–2032: ICMS a 90%, 80%, 70% e 60% da alíquota; IBS a 10%, 20%, 30% e 40% da referência;
//! - 2033: só IBS e CBS.
//!
//! IBS e CBS são **por fora** e, na transição, a base exclui o ICMS. As alíquotas de referência
//! ainda não foram fixadas pelo Senado: as daqui são **estimativas** (as da Fazenda na
//! regulamentação, 26,5% somadas) e cada termo diz isso. O Imposto Seletivo (bebidas, tabaco)
//! ainda não tem alíquota publicada e não entra. No Simples, IBS e CBS ficam dentro do DAS:
//! a projeção é a carga de hoje.

use cardeal_kernel::Percentual;

use crate::{carga, vezes, Carga, Imposto, Natureza, PerfilTributario, Regime, Termo};

/// A alíquota de referência da CBS — **estimada**.
pub const CBS_REFERENCIA_ESTIMADA: Percentual = Percentual::milesimos(8_800);
/// A alíquota de referência do IBS (estadual + municipal) — **estimada**.
pub const IBS_REFERENCIA_ESTIMADA: Percentual = Percentual::milesimos(17_700);
/// O IBS de teste de 2026 a 2028.
const IBS_TESTE: Percentual = Percentual::milesimos(100);

/// A carga do produto no ano, pelo cronograma da reforma.
///
/// # Errors
/// O mesmo de [`carga`].
pub fn carga_no_ano(regime: &Regime, perfil: &PerfilTributario, ano: u16) -> Result<Carga, String> {
    let hoje = carga(regime, perfil)?;
    let normal = matches!(regime, Regime::Presumido { .. } | Regime::Real { .. });
    if !normal || ano <= 2026 {
        return Ok(hoje);
    }
    // A parte do ICMS que sobra (em décimos) e a do IBS de referência.
    let (icms_decimos, ibs): (i64, Percentual) = match ano {
        2027 | 2028 => (10, IBS_TESTE),
        2029..=2032 => {
            let passo = i64::from(ano - 2028);
            (
                10 - passo,
                vezes(IBS_REFERENCIA_ESTIMADA, Percentual::pontos(10 * passo)),
            )
        }
        _ => (0, IBS_REFERENCIA_ESTIMADA),
    };
    let cbs = if ano <= 2028 {
        CBS_REFERENCIA_ESTIMADA - IBS_TESTE
    } else {
        CBS_REFERENCIA_ESTIMADA
    };
    let fator = Percentual::pontos(icms_decimos * 10);
    let mut c = Carga::default();
    for t in hoje.termos {
        match t.imposto {
            Imposto::Pis | Imposto::Cofins | Imposto::Ipi => {}
            Imposto::Icms | Imposto::Fcp | Imposto::IcmsSt | Imposto::Difal => {
                let coeficiente = vezes(t.coeficiente, fator);
                if !coeficiente.e_zero() {
                    c.termos.push(Termo {
                        coeficiente,
                        explicacao: format!(
                            "{} ({}% da alíquota em {ano})",
                            t.explicacao,
                            icms_decimos * 10
                        ),
                        ..t
                    });
                }
            }
            _ => c.termos.push(t),
        }
    }
    let sem_icms = c
        .termos
        .iter()
        .filter(|t| {
            matches!(t.imposto, Imposto::Icms | Imposto::Fcp) && t.natureza == Natureza::PorDentro
        })
        .fold(Percentual::ZERO, |a, t| a + t.coeficiente)
        .complemento();
    for (imposto, aliquota) in [(Imposto::Cbs, cbs), (Imposto::Ibs, ibs)] {
        c.termos.push(Termo {
            imposto,
            natureza: Natureza::PorFora,
            coeficiente: vezes(aliquota, sem_icms),
            explicacao: format!(
                "{} {} em {ano} sobre o preço sem o ICMS (alíquota de referência estimada)",
                imposto.nome(),
                aliquota.formatar_com_simbolo()
            ),
        });
    }
    Ok(c)
}
