//! A carga de um produto em cada regime, com os números conferidos à mão no comentário.

use cardeal_kernel::{Dinheiro, Percentual};
use cardeal_tributos::{carga, AnexoSimples, Atividade, Imposto, PerfilTributario, Regime};

fn un(pontos_milionesimos: i64) -> Percentual {
    Percentual::unidades(pontos_milionesimos)
}

fn pct(p: i64) -> Option<Percentual> {
    Some(Percentual::pontos(p))
}

fn perfil(situacao: &str) -> PerfilTributario {
    PerfilTributario {
        situacao_icms: Some(situacao.to_owned()),
        ..PerfilTributario::default()
    }
}

fn simples(anexo: AnexoSimples, rbt12_reais: i64) -> Regime {
    Regime::Simples {
        anexo,
        rbt12: Dinheiro::reais(rbt12_reais),
    }
}

fn presumido(receita_mensal_reais: i64) -> Regime {
    Regime::Presumido {
        atividade: Atividade::Comercio,
        receita_mensal: Dinheiro::reais(receita_mensal_reais),
    }
}

fn termo(c: &cardeal_tributos::Carga, imposto: Imposto) -> Percentual {
    c.termos
        .iter()
        .filter(|t| t.imposto == imposto)
        .fold(Percentual::ZERO, |a, t| a + t.coeficiente)
}

#[test]
fn simples_anexo_i_st_paga_menos_que_revenda_comum() {
    // RBT12 R$ 500 mil: 3ª faixa, 9,5% − 13.860 ÷ 500.000 = 6,728%.
    let r = simples(AnexoSimples::I, 500_000);
    let comum = carga(&r, &perfil("102")).unwrap();
    assert_eq!(comum.por_dentro(), un(6_728_000));
    // Substituído (CSOSN 500): sai a parcela de ICMS da faixa (33,5%): 6,728 × 0,665.
    let st = carga(&r, &perfil("500")).unwrap();
    assert_eq!(st.por_dentro(), un(4_474_120));
    assert!(
        st.termos[0].explicacao.contains("ICMS"),
        "a frase diz por quê"
    );
}

#[test]
fn presumido_pis_cofins_nao_incide_sobre_o_icms() {
    let p = PerfilTributario {
        aliquota_icms: pct(17),
        ..perfil("00")
    };
    let c = carga(&presumido(100_000), &p).unwrap();
    assert_eq!(termo(&c, Imposto::Icms), un(17_000_000));
    // 0,65% × (1 − 17%) e 3% × (1 − 17%).
    assert_eq!(termo(&c, Imposto::Pis), un(539_500));
    assert_eq!(termo(&c, Imposto::Cofins), un(2_490_000));
}

#[test]
fn presumido_inclui_irpj_csll_de_2_28() {
    let p = PerfilTributario {
        aliquota_icms: pct(17),
        ..perfil("00")
    };
    let c = carga(&presumido(100_000), &p).unwrap();
    // 8% × 15% = 1,2% e 12% × 9% = 1,08%.
    assert_eq!(termo(&c, Imposto::Irpj), un(1_200_000));
    assert_eq!(termo(&c, Imposto::Csll), un(1_080_000));
    assert_eq!(c.por_dentro(), un(22_309_500));
}

#[test]
fn presumido_liga_o_adicional_acima_de_20_mil_de_lucro_presumido() {
    let p = PerfilTributario {
        aliquota_icms: pct(17),
        ..perfil("00")
    };
    // R$ 300 mil × 8% = R$ 24 mil > R$ 20 mil: + 10% × 8% = 0,8% na margem.
    let c = carga(&presumido(300_000), &p).unwrap();
    assert_eq!(termo(&c, Imposto::Irpj), un(2_000_000));
    assert_eq!(c.por_dentro(), un(23_109_500));
}

#[test]
fn real_tem_ir_sobre_o_lucro_de_34() {
    let p = PerfilTributario {
        aliquota_icms: pct(17),
        ..perfil("00")
    };
    let c = carga(
        &Regime::Real {
            pis_cofins_nao_cumulativo: true,
        },
        &p,
    )
    .unwrap();
    // 1,65% × 0,83 = 1,3695%; 7,6% × 0,83 = 6,308%.
    assert_eq!(c.por_dentro(), un(17_000_000 + 1_369_500 + 6_308_000));
    assert_eq!(c.sobre_o_lucro(), un(34_000_000));
}

#[test]
fn industria_substituta_no_simples_nao_tira_o_icms() {
    let p = PerfilTributario {
        aliquota_icms: pct(17),
        mva_st: pct(40),
        ..perfil("201")
    };
    let c = carga(&simples(AnexoSimples::II, 0), &p).unwrap();
    // Sem faturamento: a nominal da 1ª faixa do Anexo II, 4,5%, inteira.
    assert_eq!(c.por_dentro(), un(4_500_000));
    // ST por fora: 140% × 17% − 17% = 6,8%.
    assert_eq!(c.por_fora(), un(6_800_000));
}

#[test]
fn anexo_ii_monofasico_tira_pis_cofins() {
    let p = PerfilTributario {
        cst_pis: Some("04".into()),
        cst_cofins: Some("04".into()),
        ..perfil("102")
    };
    let c = carga(&simples(AnexoSimples::II, 0), &p).unwrap();
    // PIS+COFINS são 14% da repartição do Anexo II: 4,5% × 0,86.
    assert_eq!(c.por_dentro(), un(3_870_000));
}

#[test]
fn acima_do_sublimite_icms_sai_do_das_e_entra_pelo_perfil() {
    // R$ 4 mi: 6ª faixa, 19% − 378.000 ÷ 4.000.000 = 9,55%, já sem ICMS no DAS.
    let r = simples(AnexoSimples::I, 4_000_000);
    let p = PerfilTributario {
        aliquota_icms: pct(17),
        ..perfil("102")
    };
    let c = carga(&r, &p).unwrap();
    assert_eq!(termo(&c, Imposto::Das), un(9_550_000));
    assert_eq!(termo(&c, Imposto::Icms), un(17_000_000));
    // Substituído: só o DAS.
    assert_eq!(
        carga(&r, &perfil("500")).unwrap().por_dentro(),
        un(9_550_000)
    );
    // Sem a alíquota não há como calcular: diz o que falta.
    let e = carga(&r, &perfil("102")).unwrap_err();
    assert!(e.contains("alíquota de ICMS"), "{e}");
}

#[test]
fn ipi_vai_por_fora() {
    let p = PerfilTributario {
        aliquota_icms: pct(17),
        aliquota_ipi: pct(10),
        ..perfil("00")
    };
    let c = carga(&presumido(100_000), &p).unwrap();
    assert_eq!(c.por_fora(), un(10_000_000));
    assert_eq!(termo(&c, Imposto::Ipi), un(10_000_000));
}

#[test]
fn fcp_soma_ao_icms_na_mesma_base() {
    let p = PerfilTributario {
        aliquota_icms: pct(18),
        aliquota_fcp: pct(2),
        reducao_base: pct(50),
        ..perfil("20")
    };
    let c = carga(&presumido(100_000), &p).unwrap();
    assert_eq!(termo(&c, Imposto::Icms), un(9_000_000));
    assert_eq!(termo(&c, Imposto::Fcp), un(1_000_000));
    // PIS sobre a receita sem o ICMS (o FCP é ICMS): 0,65% × 0,9.
    assert_eq!(termo(&c, Imposto::Pis), un(585_000));
}

#[test]
fn mei_nao_tem_termo() {
    assert!(carga(&Regime::Mei, &perfil("102"))
        .unwrap()
        .termos
        .is_empty());
}

#[test]
fn cst_normal_no_simples_e_erro_em_portugues() {
    let e = carga(&simples(AnexoSimples::I, 0), &perfil("00")).unwrap_err();
    assert!(e.contains("CSOSN"), "{e}");
    let e = carga(&presumido(1), &perfil("00")).unwrap_err();
    assert!(e.contains("alíquota de ICMS"), "{e}");
}
