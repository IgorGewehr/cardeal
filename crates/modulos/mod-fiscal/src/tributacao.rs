//! Os impostos de um item da NFC-e, a partir do regime da empresa e do **perfil fiscal
//! efetivo** do produto (`mod-estoque`, herdado pela árvore de grupos). O `sefaz-api` não
//! calcula imposto — só confere `base × alíquota` —, então a conta é aqui (`docs/10` §6).
//!
//! O que esta versão emite (o resto vira um problema claro no item, nunca uma nota errada):
//!
//! - **Simples Nacional:** CSOSN 102, 103, 300, 400 e 500 (sem destaque de ICMS); sem CSOSN no
//!   perfil, 102. PIS/COFINS com o CST do perfil (padrão 49) e valores zerados — no Simples eles
//!   são recolhidos no DAS.
//! - **Regime normal:** CST 00 (base = valor do item), 20 (base reduzida), 40, 41, 50 e 60 (sem
//!   destaque). PIS/COFINS: CST 01/02 calculados pela alíquota do perfil; 04 a 09 sem valores;
//!   os demais (49, 99...) com valores zerados.
//! - **CFOP:** o de saída dentro da UF do perfil; sem ele, 5405 para ICMS já retido (CSOSN 500 /
//!   CST 60) e 5102 para o resto. NFC-e é sempre operação interna: CFOP fora do 5xxx é recusado.

use cardeal_fiscal::{ContribuicaoItem, Crt, IcmsItem, ImpostosItem};
use cardeal_kernel::{Arredondamento, Dinheiro, Percentual};
use mod_estoque::fiscal::CamposFiscais;

/// Os impostos e o CFOP de um item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TributacaoDoItem {
    /// Os impostos.
    pub impostos: ImpostosItem,
    /// O CFOP.
    pub cfop: String,
}

const CSOSN_SEM_DESTAQUE: [&str; 5] = ["102", "103", "300", "400", "500"];
const CST_SEM_DESTAQUE: [&str; 4] = ["40", "41", "50", "60"];

fn valor_de(base: Dinheiro, aliquota: Percentual) -> Dinheiro {
    base.aplicar(aliquota, Arredondamento::MeioAcima)
}

/// O ICMS do item e se ele já foi retido antes (ST): CSOSN no Simples, CST no regime normal.
fn icms(crt: Crt, perfil: &CamposFiscais, base: Dinheiro) -> Result<(IcmsItem, bool), String> {
    let origem = perfil.origem.unwrap_or(0);
    let situacao = perfil.cst_icms.clone().unwrap_or_default();
    Ok(if crt.e_simples() {
        let csosn = if situacao.is_empty() {
            "102".to_string()
        } else if situacao.len() == 3 {
            situacao
        } else {
            return Err(format!(
                "o perfil fiscal tem o CST {situacao} (regime normal), mas a empresa é do Simples — use um CSOSN (ex.: 102)"
            ));
        };
        if !CSOSN_SEM_DESTAQUE.contains(&csosn.as_str()) {
            return Err(format!(
                "o CSOSN {csosn} ainda não é emitido na NFC-e (use 102, 103, 300, 400 ou 500)"
            ));
        }
        let retido = csosn == "500";
        (
            IcmsItem {
                origem,
                cst: None,
                csosn: Some(csosn),
                base: None,
                aliquota: None,
                valor: None,
                reducao_base: None,
            },
            retido,
        )
    } else {
        if situacao.len() != 2 {
            return Err(if situacao.is_empty() {
                "falta o CST do ICMS no perfil fiscal".to_string()
            } else {
                format!("o perfil fiscal tem o CSOSN {situacao} (Simples), mas a empresa é do regime normal — use um CST")
            });
        }
        match situacao.as_str() {
            "00" | "20" => {
                let aliquota = perfil.aliquota_icms.ok_or_else(|| {
                    format!("o CST {situacao} pede a alíquota de ICMS no perfil fiscal")
                })?;
                let reducao = (situacao == "20").then(|| perfil.reducao_base.unwrap_or_default());
                if situacao == "20" && reducao.is_none_or(Percentual::e_zero) {
                    return Err("o CST 20 pede a redução da base no perfil fiscal".to_string());
                }
                let base_icms = reducao.map_or(base, |r| base - valor_de(base, r));
                (
                    IcmsItem {
                        origem,
                        cst: Some(situacao.clone()),
                        csosn: None,
                        base: Some(base_icms),
                        aliquota: Some(aliquota),
                        valor: Some(valor_de(base_icms, aliquota)),
                        reducao_base: reducao,
                    },
                    false,
                )
            }
            s if CST_SEM_DESTAQUE.contains(&s) => (
                IcmsItem {
                    origem,
                    cst: Some(situacao.clone()),
                    csosn: None,
                    base: None,
                    aliquota: None,
                    valor: None,
                    reducao_base: None,
                },
                s == "60",
            ),
            s => {
                return Err(format!(
                    "o CST {s} do ICMS ainda não é emitido na NFC-e (use 00, 20, 40, 41, 50 ou 60)"
                ))
            }
        }
    })
}

/// Calcula ICMS, PIS, COFINS e o CFOP de um item cujo valor líquido é `base`.
///
/// # Errors
/// A frase do problema (sem o nome do item — quem chama acrescenta).
pub fn tributar(
    crt: Crt,
    perfil: &CamposFiscais,
    base: Dinheiro,
) -> Result<TributacaoDoItem, String> {
    // O preço sugerido já conta o FCP (`cardeal-tributos`); a emissão ainda não o destaca.
    // Sair sem ele seria uma nota errada — melhor parar e dizer.
    if let Some(fcp) = perfil.aliquota_fcp.filter(|p| !p.e_zero()) {
        return Err(format!(
            "o perfil fiscal tem FCP de {}, que a emissão ainda não destaca — retire o FCP do \
             perfil até a nota suportá-lo",
            fcp.formatar_com_simbolo()
        ));
    }
    let (icms, icms_retido) = icms(crt, perfil, base)?;

    let contribuicao = |cst: &Option<String>, aliquota: Option<Percentual>, nome: &str| {
        let cst = cst.clone().unwrap_or_else(|| "49".to_string());
        if cst.len() != 2 {
            return Err(format!("CST do {nome} inválido no perfil fiscal: {cst}"));
        }
        let zerado = || ContribuicaoItem {
            cst: cst.clone(),
            base: Some(Dinheiro::ZERO),
            aliquota: Some(Percentual::ZERO),
            valor: Some(Dinheiro::ZERO),
        };
        Ok(match cst.as_str() {
            _ if crt.e_simples() => zerado(),
            "01" | "02" => {
                let a = aliquota.ok_or_else(|| {
                    format!("o CST {cst} do {nome} pede a alíquota no perfil fiscal")
                })?;
                ContribuicaoItem {
                    cst: cst.clone(),
                    base: Some(base),
                    aliquota: Some(a),
                    valor: Some(valor_de(base, a)),
                }
            }
            "04" | "05" | "06" | "07" | "08" | "09" => ContribuicaoItem {
                cst: cst.clone(),
                base: None,
                aliquota: None,
                valor: None,
            },
            _ => zerado(),
        })
    };
    let pis = contribuicao(&perfil.cst_pis, perfil.aliquota_pis, "PIS")?;
    let cofins = contribuicao(&perfil.cst_cofins, perfil.aliquota_cofins, "COFINS")?;

    let cfop = match &perfil.cfop_saida_dentro_uf {
        Some(c) => c.clone(),
        None if icms_retido => "5405".to_string(),
        None => "5102".to_string(),
    };
    if !(cfop.len() == 4 && cfop.starts_with('5')) {
        return Err(format!(
            "o CFOP {cfop} não é de venda dentro do estado (5xxx), como a NFC-e exige"
        ));
    }
    Ok(TributacaoDoItem {
        impostos: ImpostosItem { icms, pis, cofins },
        cfop,
    })
}

#[cfg(test)]
mod testes {
    use super::*;

    fn perfil() -> CamposFiscais {
        CamposFiscais::default()
    }

    #[test]
    fn simples_sem_perfil_sai_102_cst_49_zerado_e_5102() {
        let t = tributar(Crt::SimplesNacional, &perfil(), Dinheiro::reais(100)).unwrap();
        assert_eq!(t.impostos.icms.csosn.as_deref(), Some("102"));
        assert_eq!(t.impostos.icms.valor, None);
        assert_eq!(t.impostos.pis.cst, "49");
        assert_eq!(t.impostos.pis.valor, Some(Dinheiro::ZERO));
        assert_eq!(t.cfop, "5102");
    }

    #[test]
    fn simples_com_st_sai_5405_e_recusa_cst_de_regime_normal() {
        let mut p = perfil();
        p.cst_icms = Some("500".into());
        p.origem = Some(2);
        let t = tributar(Crt::SimplesNacional, &p, Dinheiro::reais(10)).unwrap();
        assert_eq!(t.cfop, "5405");
        assert_eq!(t.impostos.icms.origem, 2);
        p.cst_icms = Some("00".into());
        assert!(tributar(Crt::SimplesNacional, &p, Dinheiro::reais(10))
            .unwrap_err()
            .contains("CSOSN"));
        p.cst_icms = Some("101".into());
        assert!(tributar(Crt::SimplesNacional, &p, Dinheiro::reais(10)).is_err());
    }

    #[test]
    fn regime_normal_calcula_icms_pis_cofins() {
        let mut p = perfil();
        p.cst_icms = Some("00".into());
        p.aliquota_icms = Some(Percentual::pontos(17));
        p.cst_pis = Some("01".into());
        p.aliquota_pis = Some(Percentual::milesimos(650));
        p.cst_cofins = Some("01".into());
        p.aliquota_cofins = Some(Percentual::pontos(3));
        p.cfop_saida_dentro_uf = Some("5102".into());
        let t = tributar(Crt::RegimeNormal, &p, Dinheiro::centavos(12_345)).unwrap();
        let icms = &t.impostos.icms;
        assert_eq!(icms.base, Some(Dinheiro::centavos(12_345)));
        assert_eq!(icms.valor, Some(Dinheiro::centavos(2_099))); // 20,9865 → 20,99
        assert_eq!(t.impostos.pis.valor, Some(Dinheiro::centavos(80))); // 0,802 → 0,80
        assert_eq!(t.impostos.cofins.valor, Some(Dinheiro::centavos(370))); // 3,7035 → 3,70

        // CST 20: base reduzida.
        p.cst_icms = Some("20".into());
        p.reducao_base = Some(Percentual::pontos(40));
        let t = tributar(Crt::RegimeNormal, &p, Dinheiro::reais(100)).unwrap();
        assert_eq!(t.impostos.icms.base, Some(Dinheiro::reais(60)));
        assert_eq!(t.impostos.icms.valor, Some(Dinheiro::centavos(1_020)));
    }

    #[test]
    fn regime_normal_aponta_o_que_falta() {
        let mut p = perfil();
        assert!(tributar(Crt::RegimeNormal, &p, Dinheiro::reais(1))
            .unwrap_err()
            .contains("falta o CST"));
        p.cst_icms = Some("00".into());
        assert!(tributar(Crt::RegimeNormal, &p, Dinheiro::reais(1))
            .unwrap_err()
            .contains("alíquota"));
        p.cst_icms = Some("60".into());
        p.cst_pis = Some("01".into());
        assert!(tributar(Crt::RegimeNormal, &p, Dinheiro::reais(1))
            .unwrap_err()
            .contains("PIS"));
        p.cst_pis = Some("06".into());
        let t = tributar(Crt::RegimeNormal, &p, Dinheiro::reais(1)).unwrap();
        assert_eq!(t.cfop, "5405");
        assert_eq!(t.impostos.pis.base, None);
        p.cfop_saida_dentro_uf = Some("6102".into());
        assert!(tributar(Crt::RegimeNormal, &p, Dinheiro::reais(1))
            .unwrap_err()
            .contains("5xxx"));
    }

    /// O ICMS (+FCP) que a nota destaca a um preço P é o que o motor de carga previu para P
    /// (`cardeal-tributos`), em todo perfil que a NFC-e emite — o preço sugerido e a nota usam
    /// a mesma conta. (PIS/COFINS: a nota destaca sobre o valor do item, o motor calcula o
    /// custo real sem o ICMS na base — a exclusão é da apuração; DAS e IR não vão na nota.)
    #[test]
    fn imposto_emitido_e_o_previsto_para_todo_perfil() {
        use cardeal_tributos::{carga, Atividade, Imposto, Regime};
        let simples = Regime::Simples {
            anexo: cardeal_tributos::AnexoSimples::I,
            rbt12: Dinheiro::reais(100_000),
        };
        let normal = Regime::Presumido {
            atividade: Atividade::Comercio,
            receita_mensal: Dinheiro::reais(50_000),
        };
        let mut casos = Vec::new();
        for c in ["102", "103", "300", "400", "500"] {
            casos.push((Crt::SimplesNacional, simples, c, None, None));
        }
        for (c, aliq, red) in [
            ("00", Some(17), None),
            ("00", Some(12), None),
            ("20", Some(18), Some(50)),
            ("40", None, None),
            ("41", None, None),
            ("60", None, None),
        ] {
            casos.push((Crt::RegimeNormal, normal, c, aliq, red));
        }
        let mut conferidos = 0;
        for (crt, regime, cst, aliq, red) in casos {
            for pis in ["01", "04", "49"] {
                let p = CamposFiscais {
                    cst_icms: Some(cst.into()),
                    aliquota_icms: aliq.map(Percentual::pontos),
                    reducao_base: red.map(Percentual::pontos),
                    cst_pis: Some(pis.into()),
                    cst_cofins: Some(pis.into()),
                    aliquota_pis: Some(Percentual::milesimos(650)),
                    aliquota_cofins: Some(Percentual::pontos(3)),
                    ..CamposFiscais::default()
                };
                let previsto = carga(&regime, &p.perfil_tributario(None)).unwrap();
                let coef = previsto
                    .termos
                    .iter()
                    .filter(|t| matches!(t.imposto, Imposto::Icms | Imposto::Fcp))
                    .fold(Percentual::ZERO, |a, t| a + t.coeficiente);
                for preco in [
                    Dinheiro::centavos(100),
                    Dinheiro::reais(15),
                    Dinheiro::centavos(99_999),
                ] {
                    let t = tributar(crt, &p, preco).unwrap();
                    let emitido = t.impostos.icms.valor.unwrap_or(Dinheiro::ZERO);
                    assert_eq!(
                        emitido,
                        valor_de(preco, coef),
                        "{crt:?} CST {cst} PIS {pis} a {preco:?}"
                    );
                    conferidos += 1;
                }
            }
        }
        assert_eq!(conferidos, 11 * 3 * 3);
    }

    #[test]
    fn fcp_no_perfil_e_recusado_ate_a_nfce_destacar() {
        let p = CamposFiscais {
            cst_icms: Some("00".into()),
            aliquota_icms: Some(Percentual::pontos(18)),
            aliquota_fcp: Some(Percentual::pontos(2)),
            ..CamposFiscais::default()
        };
        let e = tributar(Crt::RegimeNormal, &p, Dinheiro::reais(10)).unwrap_err();
        assert!(e.contains("FCP"), "{e}");
    }
}
