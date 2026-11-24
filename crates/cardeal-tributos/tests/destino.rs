//! A venda para outro estado: alíquota interestadual e o DIFAL ao consumidor de lá.

use cardeal_kernel::{Dinheiro, Percentual};
use cardeal_tributos::destino::{aliquota_interestadual, carga_no_destino, Destino};
use cardeal_tributos::{Atividade, Imposto, PerfilTributario, Regime};

fn pct(p: i64) -> Percentual {
    Percentual::pontos(p)
}

#[test]
fn importado_vai_a_4() {
    assert_eq!(aliquota_interestadual("SP", "BA", 1), pct(4));
    assert_eq!(aliquota_interestadual("SC", "SP", 8), pct(4));
}

#[test]
fn sul_sudeste_para_norte_nordeste_centro_oeste_e_es_e_7() {
    assert_eq!(aliquota_interestadual("SC", "BA", 0), pct(7));
    assert_eq!(aliquota_interestadual("SP", "ES", 0), pct(7));
    assert_eq!(aliquota_interestadual("SC", "SP", 0), pct(12));
    assert_eq!(aliquota_interestadual("BA", "SP", 0), pct(12));
    assert_eq!(aliquota_interestadual("ES", "BA", 0), pct(12));
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
        aliquota_icms: Some(pct(17)),
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
fn para_contribuinte_vale_a_interestadual() {
    let c = carga_no_destino(
        &presumido(),
        &tributado(),
        &Destino::Interestadual {
            interestadual: pct(12),
            interna_destino: None,
        },
    )
    .unwrap();
    assert_eq!(termo(&c, Imposto::Icms), pct(12));
    // PIS sobre a receita sem o ICMS de 12%: 0,65% × 0,88.
    assert_eq!(termo(&c, Imposto::Pis), Percentual::unidades(572_000));
}

#[test]
fn difal_ao_consumidor_de_outra_uf() {
    // 12% na origem, 20% no destino, base dupla:
    // P × [(1 − 0,12) × 0,20 ÷ (1 − 0,20) − 0,12] = P × 0,10.
    let c = carga_no_destino(
        &presumido(),
        &tributado(),
        &Destino::Interestadual {
            interestadual: pct(12),
            interna_destino: Some(pct(20)),
        },
    )
    .unwrap();
    assert_eq!(termo(&c, Imposto::Icms), pct(12));
    let difal = c
        .termos
        .iter()
        .find(|t| t.explicacao.contains("DIFAL"))
        .expect("DIFAL");
    assert_eq!(difal.coeficiente, pct(10));
}

#[test]
fn simples_nao_muda_e_interno_e_a_carga_de_sempre() {
    let s = Regime::Simples {
        anexo: cardeal_tributos::AnexoSimples::I,
        rbt12: Dinheiro::reais(100_000),
    };
    let p = PerfilTributario {
        situacao_icms: Some("102".into()),
        ..PerfilTributario::default()
    };
    let fora = carga_no_destino(
        &s,
        &p,
        &Destino::Interestadual {
            interestadual: pct(12),
            interna_destino: Some(pct(20)),
        },
    )
    .unwrap();
    assert_eq!(fora, cardeal_tributos::carga(&s, &p).unwrap());
    assert_eq!(
        carga_no_destino(&presumido(), &tributado(), &Destino::InternoConsumidor).unwrap(),
        cardeal_tributos::carga(&presumido(), &tributado()).unwrap()
    );
}
