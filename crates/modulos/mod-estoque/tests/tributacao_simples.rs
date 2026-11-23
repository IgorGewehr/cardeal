//! "Como este produto é tributado": as opções em palavras do dono e os campos que cada uma grava.

use cardeal_kernel::Percentual;
use mod_estoque::fiscal::CamposFiscais;
use mod_estoque::tributacao_simples::{
    aliquota_interna, campos_da_opcao, opcao_dos_campos, OpcaoTributacao,
};

const TODAS: [OpcaoTributacao; 5] = [
    OpcaoTributacao::RevendaComum,
    OpcaoTributacao::RevendaComSt,
    OpcaoTributacao::Monofasico,
    OpcaoTributacao::StEMonofasico,
    OpcaoTributacao::IsentoOuNaoTributado,
];

#[test]
fn ida_e_volta_nos_dois_regimes() {
    for simples in [true, false] {
        for o in TODAS {
            let c = campos_da_opcao(o, simples, Some(Percentual::pontos(17)));
            assert_eq!(opcao_dos_campos(&c, simples), o, "{o:?} simples={simples}");
            assert!(c.validar().is_ok());
        }
    }
}

#[test]
fn simples_grava_csosn_e_normal_grava_cst() {
    let st = campos_da_opcao(OpcaoTributacao::RevendaComSt, true, None);
    assert_eq!(st.cst_icms.as_deref(), Some("500"));
    let st = campos_da_opcao(OpcaoTributacao::RevendaComSt, false, None);
    assert_eq!(st.cst_icms.as_deref(), Some("60"));
    let m = campos_da_opcao(OpcaoTributacao::Monofasico, true, None);
    assert_eq!(m.cst_pis.as_deref(), Some("04"));
    assert_eq!(m.cst_cofins.as_deref(), Some("04"));
    // No Simples a alíquota de ICMS não é pedida nem gravada.
    let c = campos_da_opcao(
        OpcaoTributacao::RevendaComum,
        true,
        Some(Percentual::pontos(17)),
    );
    assert_eq!(c.aliquota_icms, None);
}

#[test]
fn revenda_comum_no_normal_nao_grava_aliquota_sem_confirmar() {
    let c = campos_da_opcao(OpcaoTributacao::RevendaComum, false, None);
    assert_eq!(c.cst_icms.as_deref(), Some("00"));
    assert_eq!(c.aliquota_icms, None);
    let c = campos_da_opcao(
        OpcaoTributacao::RevendaComum,
        false,
        Some(Percentual::pontos(17)),
    );
    assert_eq!(c.aliquota_icms, Some(Percentual::pontos(17)));
}

#[test]
fn campos_fora_das_opcoes_sao_avancado() {
    let c = CamposFiscais {
        cst_icms: Some("101".into()),
        ..CamposFiscais::default()
    };
    assert_eq!(opcao_dos_campos(&c, true), OpcaoTributacao::Avancado);
    assert_eq!(
        opcao_dos_campos(&CamposFiscais::default(), true),
        OpcaoTributacao::Avancado
    );
}

#[test]
fn aliquota_interna_cobre_as_27_ufs() {
    let ufs = [
        "AC", "AL", "AM", "AP", "BA", "CE", "DF", "ES", "GO", "MA", "MG", "MS", "MT", "PA", "PB",
        "PE", "PI", "PR", "RJ", "RN", "RO", "RR", "RS", "SC", "SE", "SP", "TO",
    ];
    for uf in ufs {
        let a = aliquota_interna(uf).unwrap_or_else(|| panic!("{uf}"));
        assert!(
            a >= Percentual::pontos(17) && a <= Percentual::pontos(23),
            "{uf}"
        );
    }
    assert_eq!(aliquota_interna("sc"), Some(Percentual::pontos(17)));
    assert_eq!(aliquota_interna("XX"), None);
}
