//! O preço resolvido sobre a carga: mesma margem, mesmo lucro, qualquer que seja o imposto.

use cardeal_kernel::{Arredondamento, Dinheiro, Percentual};
use cardeal_tributos::{
    carga, resolver_preco, AnexoSimples, Atividade, Despesas, PerfilTributario, PrecoResolvido,
    Regime,
};

fn pct(p: i64) -> Percentual {
    Percentual::pontos(p)
}

fn perfil(situacao: &str) -> PerfilTributario {
    PerfilTributario {
        situacao_icms: Some(situacao.to_owned()),
        ..PerfilTributario::default()
    }
}

fn despesas(op: i64) -> Despesas {
    Despesas {
        operacional: pct(op),
        comissao: Percentual::ZERO,
        taxa_pagamento: Percentual::ZERO,
    }
}

/// |lucro − margem × preço| em centavos.
fn desvio(p: &PrecoResolvido, margem: Percentual) -> i64 {
    let alvo = p.preco.aplicar(margem, Arredondamento::MeioAcima);
    (p.lucro.em_centavos() - alvo.em_centavos()).abs()
}

fn fecha(p: &PrecoResolvido) -> bool {
    let soma = p
        .parcelas
        .iter()
        .fold(p.custo + p.lucro, |a, x| a + x.valor);
    soma == p.preco
}

#[test]
fn mesmo_custo_mesma_margem_lucro_igual_com_cargas_diferentes() {
    let r = Regime::Simples {
        anexo: AnexoSimples::I,
        rbt12: Dinheiro::reais(500_000),
    };
    let custo = Dinheiro::reais(15);
    let a = resolver_preco(
        custo,
        &carga(&r, &perfil("102")).unwrap(),
        &despesas(10),
        pct(15),
    )
    .unwrap();
    let b = resolver_preco(
        custo,
        &carga(&r, &perfil("500")).unwrap(),
        &despesas(10),
        pct(15),
    )
    .unwrap();
    assert!(a.preco > b.preco, "quem paga mais imposto sai mais caro");
    assert!(desvio(&a, pct(15)) <= 1 && desvio(&b, pct(15)) <= 1);
    assert!(fecha(&a) && fecha(&b));
    // 10 + 15 + 6,728 = 31,728 → 15 / 0,68272 = 21,97
    assert_eq!(a.preco, Dinheiro::centavos(2197));
}

#[test]
fn soma_100_por_cento_vira_mensagem() {
    let c = carga(&Regime::Manual(pct(40)), &perfil("102")).unwrap();
    let e = resolver_preco(Dinheiro::reais(15), &c, &despesas(30), pct(30)).unwrap_err();
    assert!(e.contains("100%"), "{e}");
    let e = resolver_preco(Dinheiro::ZERO, &c, &despesas(10), pct(10)).unwrap_err();
    assert!(e.contains("custo"), "{e}");
}

#[test]
fn decomposicao_fecha_no_centavo() {
    let regimes = [
        Regime::Simples {
            anexo: AnexoSimples::I,
            rbt12: Dinheiro::reais(900_000),
        },
        Regime::Presumido {
            atividade: Atividade::Comercio,
            receita_mensal: Dinheiro::reais(50_000),
        },
        Regime::Real {
            pis_cofins_nao_cumulativo: true,
        },
    ];
    let mut casos = 0;
    for (i, r) in regimes.iter().enumerate() {
        let p = PerfilTributario {
            aliquota_icms: Some(pct(17)),
            ..perfil(if i == 0 { "102" } else { "00" })
        };
        let c = carga(r, &p).unwrap();
        for custo in (1..=400).map(|k| Dinheiro::centavos(k * 137 + 3)) {
            let m = Percentual::milesimos(5_000 + (custo.em_centavos() % 20) * 1_000);
            let d = Despesas {
                operacional: pct(8),
                comissao: Percentual::milesimos(1_500),
                taxa_pagamento: Percentual::milesimos(2_490),
            };
            let x = resolver_preco(custo, &c, &d, m).unwrap();
            assert!(fecha(&x), "{r:?} {custo:?}");
            assert!(desvio(&x, m) <= 3, "{r:?} {custo:?}: {}", desvio(&x, m));
            assert!(x.empate < x.preco);
            casos += 1;
        }
    }
    assert_eq!(casos, 1_200);
}

#[test]
fn real_lucro_depois_do_ir_e_a_margem_pedida() {
    let p = PerfilTributario {
        aliquota_icms: Some(pct(17)),
        ..perfil("00")
    };
    let c = carga(
        &Regime::Real {
            pis_cofins_nao_cumulativo: true,
        },
        &p,
    )
    .unwrap();
    let x = resolver_preco(Dinheiro::reais(100), &c, &despesas(0), pct(15)).unwrap();
    assert!(desvio(&x, pct(15)) <= 1, "{}", desvio(&x, pct(15)));
    assert!(x.parcelas.iter().any(|p| p.rotulo == "IRPJ"));
}

#[test]
fn ipi_soma_ao_cobrado_e_nao_ao_preco() {
    let p = PerfilTributario {
        aliquota_icms: Some(pct(17)),
        aliquota_ipi: Some(pct(10)),
        ..perfil("00")
    };
    let c = carga(
        &Regime::Presumido {
            atividade: Atividade::Industria,
            receita_mensal: Dinheiro::reais(50_000),
        },
        &p,
    )
    .unwrap();
    let x = resolver_preco(Dinheiro::reais(15), &c, &despesas(10), pct(15)).unwrap();
    assert_eq!(
        x.cobrado,
        x.preco + x.preco.aplicar(pct(10), Arredondamento::MeioAcima)
    );
    assert!(fecha(&x));
    assert_eq!(x.por_fora.len(), 1);
}

#[test]
fn imposto_e_a_soma_das_parcelas_de_imposto() {
    let p = PerfilTributario {
        aliquota_icms: Some(pct(17)),
        ..perfil("00")
    };
    let c = carga(
        &Regime::Presumido {
            atividade: Atividade::Comercio,
            receita_mensal: Dinheiro::reais(50_000),
        },
        &p,
    )
    .unwrap();
    let x = resolver_preco(Dinheiro::reais(15), &c, &despesas(10), pct(15)).unwrap();
    let so_impostos = x
        .parcelas
        .iter()
        .filter(|p| p.rotulo != "Operação")
        .fold(Dinheiro::ZERO, |a, p| a + p.valor);
    assert_eq!(x.imposto, so_impostos);
    assert!(x.imposto.e_positivo());
}
