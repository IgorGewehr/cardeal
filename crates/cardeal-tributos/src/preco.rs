//! O preço que acerta a margem sobre a carga, em forma fechada (spec §5.1):
//!
//! ```text
//! m'      = margem ÷ (1 − sobre o lucro)
//! divisor = 1 − (por dentro + operação + comissão + taxa + m')
//! preço   = custo ÷ divisor                (centavo, meio-acima)
//! cobrado = preço + Σ por fora × preço     (o que vai na nota)
//! ```
//!
//! O lucro é o resto (absorve os arredondamentos): `custo + Σ parcelas + lucro = preço` sempre.

use cardeal_kernel::{Arredondamento, Dinheiro, Percentual};
use serde::{Deserialize, Serialize};

use crate::{Carga, Natureza};

/// As despesas sobre a venda que não são imposto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Despesas {
    /// Custo operacional (despesas fixas e variáveis sobre o faturamento).
    pub operacional: Percentual,
    /// Comissão de vendedor.
    pub comissao: Percentual,
    /// Taxa de cartão/PIX/boleto.
    pub taxa_pagamento: Percentual,
}

/// Uma parte do preço: para onde vai aquele pedaço de cada real.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParcelaDoPreco {
    /// O nome curto ("ICMS", "Operação", "Cartão").
    pub rotulo: String,
    /// Quanto.
    pub valor: Dinheiro,
    /// De onde veio.
    pub explicacao: String,
}

/// O preço resolvido e decomposto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrecoResolvido {
    /// O custo usado (já sem os créditos recuperáveis).
    pub custo: Dinheiro,
    /// O preço de venda.
    pub preco: Dinheiro,
    /// O que o cliente paga: o preço mais os impostos por fora.
    pub cobrado: Dinheiro,
    /// Para onde vai o preço, além do custo e do lucro: impostos por dentro e sobre o lucro,
    /// operação, comissão e taxa.
    pub parcelas: Vec<ParcelaDoPreco>,
    /// Os impostos por fora (somados ao preço no `cobrado`).
    pub por_fora: Vec<ParcelaDoPreco>,
    /// O lucro líquido.
    pub lucro: Dinheiro,
    /// O preço de empate (lucro zero).
    pub empate: Dinheiro,
}

fn dividir(custo: Dinheiro, divisor: Percentual) -> Dinheiro {
    let num = i128::from(custo.em_centavos()) * i128::from(Percentual::CEM.unidades_internas());
    let den = i128::from(divisor.unidades_internas());
    let q = (num * 2 + den) / (den * 2);
    Dinheiro::centavos(i64::try_from(q).unwrap_or(i64::MAX))
}

/// `margem ÷ (1 − ir)`: a margem antes do imposto sobre o lucro.
fn antes_do_ir(margem: Percentual, ir: Percentual) -> Percentual {
    if ir.e_zero() {
        return margem;
    }
    let un = i128::from(margem.unidades_internas())
        * i128::from(Percentual::CEM.unidades_internas())
        / i128::from(ir.complemento().unidades_internas());
    Percentual::unidades(i64::try_from(un).unwrap_or(i64::MAX))
}

/// O preço que deixa `margem` de lucro líquido sobre o preço, depois de toda a carga.
///
/// # Errors
/// Custo zero/negativo, ou a soma de impostos, despesas e margem chegando a 100%.
pub fn resolver_preco(
    custo_liquido: Dinheiro,
    carga: &Carga,
    despesas: &Despesas,
    margem: Percentual,
) -> Result<PrecoResolvido, String> {
    if !custo_liquido.e_positivo() {
        return Err(
            "o produto ainda não tem custo: dê entrada no estoque ou informe o custo".into(),
        );
    }
    let ir = carga.sobre_o_lucro();
    if ir.unidades_internas() >= Percentual::CEM.unidades_internas() {
        return Err("o imposto sobre o lucro não pode chegar a 100%".into());
    }
    let margem_bruta = antes_do_ir(margem, ir);
    let sem_margem =
        carga.por_dentro() + despesas.operacional + despesas.comissao + despesas.taxa_pagamento;
    let soma = sem_margem + margem_bruta;
    if soma.unidades_internas() >= Percentual::CEM.unidades_internas() {
        return Err(format!(
            "impostos, despesas e margem somam {} — chegando a 100% não há preço possível; \
             reduza a margem ou revise os percentuais",
            soma.formatar_com_simbolo()
        ));
    }
    let preco = dividir(custo_liquido, soma.complemento());
    let empate = dividir(custo_liquido, sem_margem.complemento());
    let parte = |p: Percentual| preco.aplicar(p, Arredondamento::MeioAcima);

    let mut parcelas: Vec<ParcelaDoPreco> = carga
        .termos
        .iter()
        .filter(|t| t.natureza == Natureza::PorDentro)
        .map(|t| ParcelaDoPreco {
            rotulo: t.imposto.nome().to_owned(),
            valor: parte(t.coeficiente),
            explicacao: t.explicacao.clone(),
        })
        .collect();
    for (rotulo, p, explicacao) in [
        (
            "Operação",
            despesas.operacional,
            "custo operacional sobre o faturamento",
        ),
        ("Comissão", despesas.comissao, "comissão de vendedor"),
        (
            "Cartão",
            despesas.taxa_pagamento,
            "taxa do meio de pagamento",
        ),
    ] {
        if !p.e_zero() {
            parcelas.push(ParcelaDoPreco {
                rotulo: rotulo.to_owned(),
                valor: parte(p),
                explicacao: format!("{} de {explicacao}", p.formatar_com_simbolo()),
            });
        }
    }
    let antes_do_lucro = parcelas.iter().fold(custo_liquido, |a, p| a + p.valor);
    let lucro_bruto = preco - antes_do_lucro;
    for t in carga
        .termos
        .iter()
        .filter(|t| t.natureza == Natureza::SobreOLucro)
    {
        let base = if lucro_bruto.e_positivo() {
            lucro_bruto
        } else {
            Dinheiro::ZERO
        };
        parcelas.push(ParcelaDoPreco {
            rotulo: t.imposto.nome().to_owned(),
            valor: base.aplicar(t.coeficiente, Arredondamento::MeioAcima),
            explicacao: t.explicacao.clone(),
        });
    }
    let lucro = parcelas
        .iter()
        .fold(preco - custo_liquido, |a, p| a - p.valor);
    let por_fora: Vec<ParcelaDoPreco> = carga
        .termos
        .iter()
        .filter(|t| t.natureza == Natureza::PorFora)
        .map(|t| ParcelaDoPreco {
            rotulo: t.imposto.nome().to_owned(),
            valor: parte(t.coeficiente),
            explicacao: t.explicacao.clone(),
        })
        .collect();
    let cobrado = por_fora.iter().fold(preco, |a, p| a + p.valor);
    Ok(PrecoResolvido {
        custo: custo_liquido,
        preco,
        cobrado,
        parcelas,
        por_fora,
        lucro,
        empate,
    })
}
