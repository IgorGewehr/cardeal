//! A projeção da reforma tributária (LC 214/2025): a carga do mesmo produto ano a ano.

use cardeal_kernel::{Dinheiro, Percentual};
use cardeal_tributos::reforma::{carga_no_ano, CBS_REFERENCIA_ESTIMADA, IBS_REFERENCIA_ESTIMADA};
use cardeal_tributos::{
    carga, AnexoSimples, Atividade, Imposto, Natureza, PerfilTributario, Regime,
};

fn un(u: i64) -> Percentual {
    Percentual::unidades(u)
}

fn presumido() -> Regime {
    Regime::Presumido {
        atividade: Atividade::Comercio,
        receita_mensal: Dinheiro::reais(50_000),
    }
}

fn tributado() -> PerfilTributario {
    PerfilTributario {
        situacao_icms: Some("00".into()),
        aliquota_icms: Some(Percentual::pontos(17)),
        ..PerfilTributario::default()
    }
}

fn termo(c: &cardeal_tributos::Carga, i: Imposto) -> Percentual {
    c.termos
        .iter()
        .filter(|t| t.imposto == i)
        .fold(Percentual::ZERO, |a, t| a + t.coeficiente)
}

#[test]
fn as_referencias_sao_as_estimadas() {
    assert_eq!(CBS_REFERENCIA_ESTIMADA, Percentual::milesimos(8_800));
    assert_eq!(IBS_REFERENCIA_ESTIMADA, Percentual::milesimos(17_700));
}

#[test]
fn em_2026_o_teste_nao_custa_ao_regime_normal() {
    assert_eq!(
        carga_no_ano(&presumido(), &tributado(), 2026).unwrap(),
        carga(&presumido(), &tributado()).unwrap()
    );
}

#[test]
fn em_2027_sai_o_pis_cofins_e_entra_a_cbs_sem_o_icms_na_base() {
    let c = carga_no_ano(&presumido(), &tributado(), 2027).unwrap();
    assert_eq!(termo(&c, Imposto::Pis), Percentual::ZERO);
    assert_eq!(termo(&c, Imposto::Cofins), Percentual::ZERO);
    assert_eq!(termo(&c, Imposto::Icms), Percentual::pontos(17));
    // CBS 8,8% − 0,1 de teste do IBS = 8,7% × (1 − 17%); IBS 0,1% × 0,83.
    assert_eq!(termo(&c, Imposto::Cbs), un(7_221_000));
    assert_eq!(termo(&c, Imposto::Ibs), un(83_000));
    assert!(c
        .termos
        .iter()
        .filter(|t| matches!(t.imposto, Imposto::Cbs | Imposto::Ibs))
        .all(|t| t.natureza == Natureza::PorFora && t.explicacao.contains("estimad")));
}

#[test]
fn de_2029_a_2032_o_icms_cai_e_o_ibs_sobe() {
    let c = carga_no_ano(&presumido(), &tributado(), 2029).unwrap();
    // ICMS a 90%: 15,3%; IBS a 10% da referência: 1,77% × (1 − 15,3%).
    assert_eq!(termo(&c, Imposto::Icms), un(15_300_000));
    assert_eq!(termo(&c, Imposto::Ibs), un(1_499_190));
    let c = carga_no_ano(&presumido(), &tributado(), 2032).unwrap();
    assert_eq!(termo(&c, Imposto::Icms), un(10_200_000));
}

#[test]
fn em_2033_so_ibs_e_cbs() {
    let c = carga_no_ano(&presumido(), &tributado(), 2033).unwrap();
    assert_eq!(termo(&c, Imposto::Icms), Percentual::ZERO);
    assert_eq!(termo(&c, Imposto::Cbs), CBS_REFERENCIA_ESTIMADA);
    assert_eq!(termo(&c, Imposto::Ibs), IBS_REFERENCIA_ESTIMADA);
}

#[test]
fn simples_continua_no_das() {
    let s = Regime::Simples {
        anexo: AnexoSimples::I,
        rbt12: Dinheiro::reais(100_000),
    };
    let p = PerfilTributario {
        situacao_icms: Some("102".into()),
        ..PerfilTributario::default()
    };
    assert_eq!(carga_no_ano(&s, &p, 2033).unwrap(), carga(&s, &p).unwrap());
}
