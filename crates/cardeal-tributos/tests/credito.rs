//! O crédito da compra: o que volta como imposto a recuperar e sai do custo da mercadoria.

use cardeal_kernel::{Dinheiro, Percentual};
use cardeal_tributos::{
    credito_do_item, AnexoSimples, Atividade, CompraDoItem, PerfilTributario, Regime,
};

fn perfil(situacao: &str, pis: Option<&str>) -> PerfilTributario {
    PerfilTributario {
        situacao_icms: Some(situacao.into()),
        aliquota_icms: Some(Percentual::pontos(17)),
        cst_pis: pis.map(str::to_owned),
        cst_cofins: pis.map(str::to_owned),
        ..PerfilTributario::default()
    }
}

fn compra() -> CompraDoItem {
    CompraDoItem {
        valor: Dinheiro::reais(60),
        icms_destacado: Dinheiro::centavos(1_020),
        credito_sn: Dinheiro::ZERO,
    }
}

const PRESUMIDO: Regime = Regime::Presumido {
    atividade: Atividade::Comercio,
    receita_mensal: Dinheiro::ZERO,
};

#[test]
fn presumido_credita_so_o_icms_da_saida_tributada() {
    let c = credito_do_item(&PRESUMIDO, &perfil("00", None), &compra());
    assert_eq!(c.icms, Dinheiro::centavos(1_020));
    assert_eq!(c.pis + c.cofins, Dinheiro::ZERO, "cumulativo não credita");
    // Revendido com ST (CST 60) ou isento: sem crédito de ICMS.
    assert_eq!(
        credito_do_item(&PRESUMIDO, &perfil("60", None), &compra()).total(),
        Dinheiro::ZERO
    );
    assert_eq!(
        credito_do_item(&PRESUMIDO, &perfil("40", None), &compra()).total(),
        Dinheiro::ZERO
    );
}

#[test]
fn real_nao_cumulativo_credita_pis_cofins_menos_monofasico() {
    let real = Regime::Real {
        pis_cofins_nao_cumulativo: true,
    };
    let c = credito_do_item(&real, &perfil("00", None), &compra());
    // 60 × 1,65% = 0,99; 60 × 7,6% = 4,56.
    assert_eq!(
        (c.pis, c.cofins),
        (Dinheiro::centavos(99), Dinheiro::centavos(456))
    );
    let mono = credito_do_item(&real, &perfil("00", Some("04")), &compra());
    assert_eq!(mono.pis + mono.cofins, Dinheiro::ZERO);
}

#[test]
fn simples_e_mei_nao_creditam_mas_o_credito_do_simples_vale_no_normal() {
    let s = Regime::Simples {
        anexo: AnexoSimples::I,
        rbt12: Dinheiro::reais(100_000),
    };
    assert_eq!(
        credito_do_item(&s, &perfil("102", None), &compra()).total(),
        Dinheiro::ZERO
    );
    assert_eq!(
        credito_do_item(&Regime::Mei, &perfil("102", None), &compra()).total(),
        Dinheiro::ZERO
    );
    let do_simples = CompraDoItem {
        icms_destacado: Dinheiro::ZERO,
        credito_sn: Dinheiro::centavos(75),
        ..compra()
    };
    assert_eq!(
        credito_do_item(&PRESUMIDO, &perfil("00", None), &do_simples).icms,
        Dinheiro::centavos(75)
    );
}
