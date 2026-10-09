//! Formação de preço pelo **markup divisor** e a carga tributária que entra nele.
//!
//! ```text
//! divisor = 100% − (custo operacional + margem líquida + imposto sobre a venda
//!                   + comissão + taxa de pagamento)
//! preço   = custo ÷ divisor
//! ```
//!
//! Tudo aritmética exata ([`Dinheiro`]/[`Percentual`], sem ponto flutuante). O módulo é
//! puro: quem lê o custo, a margem herdada do grupo e o faturamento dos últimos 12 meses
//! (RBT12) é a consulta; aqui só se calcula.
//!
//! # Carga tributária
//!
//! * **MEI** paga o DAS fixo — não incide percentual sobre a venda; o valor fixo é custo da
//!   empresa e entra no custo operacional.
//! * **Simples Nacional**: alíquota efetiva `(RBT12 × alíquota nominal − parcela a deduzir)
//!   ÷ RBT12` pelas tabelas da LC 123/2006 (Anexos I a V). No Anexo I (comércio) o produto
//!   com **ICMS-ST** ou **monofásico** (PIS/COFINS) não paga a parcela respectiva dentro do
//!   DAS: ela é descontada da alíquota efetiva pela repartição da faixa.
//! * **Presumido/Real**: o percentual é informado (ICMS + PIS + COFINS + ISS variam por
//!   produto, UF e apuração); o sistema não adivinha.
//!
//! As tabelas são as vigentes do Simples; **confirmar com o contador** antes de confiar nelas
//! para um negócio real, e revisar quando a reforma tributária (IBS/CBS) entrar em vigor.

use cardeal_kernel::{Arredondamento, Dinheiro, Percentual, Preco, Quantidade};
use serde::{Deserialize, Serialize};

// ───────────────────────────── markup divisor ─────────────────────────────

/// Os percentuais que incidem sobre o preço de venda.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Componentes {
    /// Custo operacional (despesas fixas e variáveis sobre o faturamento).
    pub custo_operacional: Percentual,
    /// Margem líquida desejada.
    pub margem_liquida: Percentual,
    /// Impostos sobre a venda.
    pub imposto: Percentual,
    /// Comissão de vendedor.
    pub comissao: Percentual,
    /// Taxa de cartão/PIX/boleto.
    pub taxa_pagamento: Percentual,
}

impl Componentes {
    /// A soma de tudo que incide sobre o preço.
    #[must_use]
    pub fn soma(&self) -> Percentual {
        self.custo_operacional
            + self.margem_liquida
            + self.imposto
            + self.comissao
            + self.taxa_pagamento
    }

    /// O mesmo conjunto sem margem — o que o preço precisa cobrir só para empatar.
    #[must_use]
    pub fn sem_margem(&self) -> Self {
        Self {
            margem_liquida: Percentual::ZERO,
            ..*self
        }
    }
}

/// Por que um preço não pôde ser calculado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroPreco {
    /// Os percentuais somam 100% ou mais: nenhum preço cobre os custos.
    SomaNaoFecha {
        /// A soma encontrada.
        soma: Percentual,
    },
    /// Algum percentual é negativo.
    PercentualNegativo,
    /// Custo zero ou negativo.
    CustoInvalido,
}

impl std::fmt::Display for ErroPreco {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SomaNaoFecha { soma } => write!(
                f,
                "custo operacional, margem, impostos, comissão e taxas somam {}: não sobra nada para o custo do produto",
                soma.formatar_com_simbolo()
            ),
            Self::PercentualNegativo => write!(f, "os percentuais não podem ser negativos"),
            Self::CustoInvalido => write!(f, "o custo do produto precisa ser maior que zero"),
        }
    }
}

/// O divisor (como percentual, `52%` = 0,52). Erro se a soma chega a 100%.
///
/// # Errors
/// [`ErroPreco::PercentualNegativo`] ou [`ErroPreco::SomaNaoFecha`].
pub fn divisor(c: &Componentes) -> Result<Percentual, ErroPreco> {
    let todos = [
        c.custo_operacional,
        c.margem_liquida,
        c.imposto,
        c.comissao,
        c.taxa_pagamento,
    ];
    if todos.iter().any(|p| p.unidades_internas() < 0) {
        return Err(ErroPreco::PercentualNegativo);
    }
    let soma = c.soma();
    let d = soma.complemento();
    if d.unidades_internas() <= 0 {
        return Err(ErroPreco::SomaNaoFecha { soma });
    }
    Ok(d)
}

/// O preço decomposto: para onde vai cada real cobrado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrecoCalculado {
    /// O custo usado.
    pub custo: Dinheiro,
    /// O divisor aplicado.
    pub divisor: Percentual,
    /// O preço de venda.
    pub preco: Dinheiro,
    /// Imposto embutido no preço.
    pub imposto: Dinheiro,
    /// Custo operacional embutido.
    pub custo_operacional: Dinheiro,
    /// Comissão embutida.
    pub comissao: Dinheiro,
    /// Taxa de pagamento embutida.
    pub taxa_pagamento: Dinheiro,
    /// O que sobra de lucro líquido (absorve o arredondamento dos demais).
    pub margem: Dinheiro,
}

impl PrecoCalculado {
    /// Margem líquida efetiva sobre o preço.
    #[must_use]
    pub fn margem_percentual(&self) -> Percentual {
        Percentual::da_razao(self.margem.em_centavos(), self.preco.em_centavos())
    }
}

/// Divide `custo` por `divisor`, arredondando meio-acima ao centavo.
fn dividir(custo: Dinheiro, divisor: Percentual) -> Dinheiro {
    let num = i128::from(custo.em_centavos()) * i128::from(Percentual::CEM.unidades_internas());
    let den = i128::from(divisor.unidades_internas());
    let q = (num * 2 + den) / (den * 2); // meio-acima, custo > 0
    Dinheiro::centavos(i64::try_from(q).unwrap_or(i64::MAX))
}

/// O preço de venda sugerido para o custo, pelo markup divisor.
///
/// # Errors
/// [`ErroPreco`] conforme o motivo.
pub fn preco_sugerido(custo: Dinheiro, c: &Componentes) -> Result<PrecoCalculado, ErroPreco> {
    if !custo.e_positivo() {
        return Err(ErroPreco::CustoInvalido);
    }
    let d = divisor(c)?;
    let preco = dividir(custo, d);
    let parte = |p: Percentual| preco.aplicar(p, Arredondamento::MeioAcima);
    let (imposto, opex, comissao, taxa) = (
        parte(c.imposto),
        parte(c.custo_operacional),
        parte(c.comissao),
        parte(c.taxa_pagamento),
    );
    Ok(PrecoCalculado {
        custo,
        divisor: d,
        preco,
        imposto,
        custo_operacional: opex,
        comissao,
        taxa_pagamento: taxa,
        margem: preco - custo - imposto - opex - comissao - taxa,
    })
}

/// O preço de empate: cobre custo, operação, impostos, comissão e taxa, sem lucro. Abaixo
/// disso a venda dá prejuízo.
///
/// # Errors
/// [`ErroPreco`] conforme o motivo.
pub fn preco_minimo(custo: Dinheiro, c: &Componentes) -> Result<PrecoCalculado, ErroPreco> {
    preco_sugerido(custo, &c.sem_margem())
}

/// A margem líquida que um preço praticado deixa, sobre o próprio preço. Negativa = prejuízo.
/// `None` com preço zero.
#[must_use]
pub fn margem_do_preco(preco: Dinheiro, custo: Dinheiro, c: &Componentes) -> Option<Percentual> {
    if !preco.e_positivo() {
        return None;
    }
    let sobre_preco = c.sem_margem().soma();
    let encargos = preco.aplicar(sobre_preco, Arredondamento::MeioAcima);
    let sobra = preco - encargos - custo;
    Some(Percentual::da_razao(
        sobra.em_centavos(),
        preco.em_centavos(),
    ))
}

// ───────────────────────────── painel de margem ─────────────────────────────

/// Um item de venda/orçamento para o painel de margem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemDeMargem {
    /// O texto que identifica o item (nome do produto).
    pub rotulo: String,
    /// A quantidade.
    pub quantidade: Quantidade,
    /// O que o cliente paga pelo item, já com desconto.
    pub receita: Dinheiro,
    /// O custo unitário atual; `None` = produto sem custo conhecido.
    pub custo_unitario: Option<Dinheiro>,
    /// Tudo que incide sobre a receita: custo operacional + imposto + comissão + taxas.
    pub encargos: Percentual,
}

/// A margem de um item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MargemDoItem {
    /// O texto do item.
    pub rotulo: String,
    /// A receita.
    pub receita: Dinheiro,
    /// O custo da quantidade vendida; `None` sem custo conhecido.
    pub custo: Option<Dinheiro>,
    /// A parte da receita que vai para operação, impostos, comissão e taxas.
    pub encargos: Dinheiro,
    /// Receita − custo (antes dos encargos).
    pub margem_bruta: Option<Dinheiro>,
    /// Receita − encargos − custo: o que sobra de lucro.
    pub margem_liquida: Option<Dinheiro>,
    /// A margem líquida sobre a receita.
    pub margem_liquida_pct: Option<Percentual>,
}

/// O painel de margem de um pedido ou orçamento.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PainelDeMargem {
    /// Os itens.
    pub itens: Vec<MargemDoItem>,
    /// A receita total.
    pub receita: Dinheiro,
    /// O custo dos itens que têm custo conhecido.
    pub custo: Dinheiro,
    /// Receita − custo, só dos itens com custo conhecido.
    pub margem_bruta: Dinheiro,
    /// O lucro líquido, só dos itens com custo conhecido.
    pub margem_liquida: Dinheiro,
    /// A margem líquida sobre a receita dos itens com custo conhecido.
    pub margem_liquida_pct: Option<Percentual>,
    /// Itens sem custo: ficam fora das margens (nunca contam como lucro de 100%).
    pub itens_sem_custo: u32,
}

/// Calcula a margem de cada item e do conjunto.
#[must_use]
pub fn painel_de_margem(itens: &[ItemDeMargem]) -> PainelDeMargem {
    let mut painel = PainelDeMargem {
        itens: Vec::with_capacity(itens.len()),
        receita: Dinheiro::ZERO,
        custo: Dinheiro::ZERO,
        margem_bruta: Dinheiro::ZERO,
        margem_liquida: Dinheiro::ZERO,
        margem_liquida_pct: None,
        itens_sem_custo: 0,
    };
    let mut receita_com_custo = Dinheiro::ZERO;
    for i in itens {
        let encargos = i.receita.aplicar(i.encargos, Arredondamento::MeioAcima);
        let custo = i.custo_unitario.map(|c| {
            Dinheiro::de_total(
                i.quantidade,
                Preco::centavos(c.em_centavos()),
                Arredondamento::MeioAcima,
            )
        });
        let margem_bruta = custo.map(|c| i.receita - c);
        let margem_liquida = custo.map(|c| i.receita - encargos - c);
        painel.receita += i.receita;
        match (custo, margem_bruta, margem_liquida) {
            (Some(c), Some(b), Some(l)) => {
                painel.custo += c;
                painel.margem_bruta += b;
                painel.margem_liquida += l;
                receita_com_custo += i.receita;
            }
            _ => painel.itens_sem_custo += 1,
        }
        painel.itens.push(MargemDoItem {
            rotulo: i.rotulo.clone(),
            receita: i.receita,
            custo,
            encargos,
            margem_bruta,
            margem_liquida,
            margem_liquida_pct: margem_liquida
                .filter(|_| i.receita.e_positivo())
                .map(|l| Percentual::da_razao(l.em_centavos(), i.receita.em_centavos())),
        });
    }
    if receita_com_custo.e_positivo() {
        painel.margem_liquida_pct = Some(Percentual::da_razao(
            painel.margem_liquida.em_centavos(),
            receita_com_custo.em_centavos(),
        ));
    }
    painel
}

/// Como arredondar o preço sugerido para o preço de prateleira. Sempre **para cima**, para
/// nunca ficar abaixo da margem calculada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ArredondamentoDePreco {
    /// O preço calculado, ao centavo.
    #[default]
    Nenhum,
    /// Termina em ,90 (R$ 96,86 → R$ 96,90; R$ 96,91 → R$ 97,90).
    FinalNoventa,
    /// Próximo real inteiro (R$ 96,10 → R$ 97,00; R$ 97,00 fica).
    InteiroAcima,
}

/// Arredonda o preço de prateleira conforme o modo.
#[must_use]
pub fn arredondar_preco(preco: Dinheiro, modo: ArredondamentoDePreco) -> Dinheiro {
    let c = preco.em_centavos();
    let novo = match modo {
        ArredondamentoDePreco::Nenhum => c,
        ArredondamentoDePreco::InteiroAcima => (c + 99) / 100 * 100,
        ArredondamentoDePreco::FinalNoventa => {
            let reais = c / 100;
            if c % 100 <= 90 {
                reais * 100 + 90
            } else {
                (reais + 1) * 100 + 90
            }
        }
    };
    Dinheiro::centavos(novo)
}

// ───────────────────────────── carga tributária ─────────────────────────────

/// Os anexos do Simples Nacional.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AnexoSimples {
    /// Comércio.
    I,
    /// Indústria.
    II,
    /// Serviços (instalação, reparos, manutenção…).
    III,
    /// Serviços (construção, vigilância, limpeza…).
    IV,
    /// Serviços intelectuais.
    V,
}

impl AnexoSimples {
    /// O nome curto ("Anexo I").
    #[must_use]
    pub const fn nome(self) -> &'static str {
        match self {
            Self::I => "Anexo I",
            Self::II => "Anexo II",
            Self::III => "Anexo III",
            Self::IV => "Anexo IV",
            Self::V => "Anexo V",
        }
    }
}

/// (teto da faixa em reais, alíquota nominal em centésimos de ponto, parcela a deduzir em reais)
type Faixa = (i64, i64, i64);

const TETOS: [i64; 6] = [180_000, 360_000, 720_000, 1_800_000, 3_600_000, 4_800_000];

const fn tabela(a: AnexoSimples) -> [Faixa; 6] {
    // alíquota em centésimos de ponto percentual: 4,00% = 400.
    match a {
        AnexoSimples::I => [
            (TETOS[0], 400, 0),
            (TETOS[1], 730, 5_940),
            (TETOS[2], 950, 13_860),
            (TETOS[3], 1_070, 22_500),
            (TETOS[4], 1_430, 87_300),
            (TETOS[5], 1_900, 378_000),
        ],
        AnexoSimples::II => [
            (TETOS[0], 450, 0),
            (TETOS[1], 780, 5_940),
            (TETOS[2], 1_000, 13_860),
            (TETOS[3], 1_120, 22_500),
            (TETOS[4], 1_470, 85_500),
            (TETOS[5], 3_000, 720_000),
        ],
        AnexoSimples::III => [
            (TETOS[0], 600, 0),
            (TETOS[1], 1_120, 9_360),
            (TETOS[2], 1_350, 17_640),
            (TETOS[3], 1_600, 35_640),
            (TETOS[4], 2_100, 125_640),
            (TETOS[5], 3_300, 648_000),
        ],
        AnexoSimples::IV => [
            (TETOS[0], 450, 0),
            (TETOS[1], 900, 8_100),
            (TETOS[2], 1_020, 12_420),
            (TETOS[3], 1_400, 39_780),
            (TETOS[4], 2_200, 183_780),
            (TETOS[5], 3_300, 828_000),
        ],
        AnexoSimples::V => [
            (TETOS[0], 1_550, 0),
            (TETOS[1], 1_800, 4_500),
            (TETOS[2], 1_950, 9_900),
            (TETOS[3], 2_050, 17_100),
            (TETOS[4], 2_300, 62_100),
            (TETOS[5], 3_050, 540_000),
        ],
    }
}

/// O índice (0..=5) da faixa para a receita dos 12 meses; acima do teto de R$ 4,8 mi
/// continua na última (o negócio já passou do limite do Simples — o sistema avisa à parte).
fn indice_da_faixa(a: AnexoSimples, rbt12: Dinheiro) -> usize {
    let reais = rbt12.em_centavos() / 100;
    tabela(a)
        .iter()
        .position(|(teto, _, _)| reais <= *teto)
        .unwrap_or(5)
}

/// A alíquota efetiva do Simples para o faturamento dos últimos 12 meses.
///
/// Sem histórico (`rbt12` zero, empresa nova) vale a alíquota nominal da primeira faixa.
#[must_use]
pub fn aliquota_efetiva(anexo: AnexoSimples, rbt12: Dinheiro) -> Percentual {
    let (_, nominal, deduzir) = tabela(anexo)[indice_da_faixa(anexo, rbt12)];
    let nominal_un = nominal * 10_000; // centésimos de ponto → 1e-6 ponto
    if rbt12.em_centavos() <= 0 || deduzir == 0 {
        return Percentual::unidades(nominal_un);
    }
    // efetiva = nominal − deduzir ÷ rbt12  (em pontos percentuais)
    let desconto = i128::from(deduzir) * 100 * 100 * 1_000_000 / i128::from(rbt12.em_centavos());
    Percentual::unidades(nominal_un - i64::try_from(desconto).unwrap_or(0))
}

/// Repartição do Anexo I: parte da alíquota efetiva que é ICMS e que é PIS+COFINS, em
/// centésimos de ponto sobre 100% (34,00% = 3 400), por faixa.
const fn reparticao_anexo_i(indice: usize) -> (i64, i64) {
    match indice {
        0 | 1 => (3_400, 1_550),
        2..=4 => (3_350, 1_550),
        _ => (0, 3_440),
    }
}

/// Como o produto é tributado dentro do Simples.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PerfilFiscal {
    /// ICMS já recolhido por substituição tributária: sai da parcela de ICMS do DAS.
    pub icms_st: bool,
    /// PIS/COFINS monofásico: sai da parcela de PIS/COFINS do DAS.
    pub monofasico: bool,
}

/// O regime que decide a carga sobre a venda.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegimeImposto {
    /// MEI: DAS fixo, sem percentual sobre a venda.
    Mei,
    /// Simples Nacional, pelo anexo.
    Simples(AnexoSimples),
    /// Percentual informado (Lucro Presumido/Real).
    Manual(Percentual),
}

/// A carga tributária percentual sobre a venda de um produto.
///
/// A dedução de ST/monofásico só existe para o Anexo I (comércio); nos demais anexos os
/// campos do perfil são ignorados (a repartição dos outros anexos não foi carregada).
#[must_use]
pub fn carga_sobre_venda(
    regime: RegimeImposto,
    rbt12: Dinheiro,
    perfil: PerfilFiscal,
) -> Percentual {
    match regime {
        RegimeImposto::Mei => Percentual::ZERO,
        RegimeImposto::Manual(p) => p,
        RegimeImposto::Simples(anexo) => {
            let efetiva = aliquota_efetiva(anexo, rbt12);
            if anexo != AnexoSimples::I {
                return efetiva;
            }
            let (icms, pis_cofins) = reparticao_anexo_i(indice_da_faixa(anexo, rbt12));
            let mut retirar = 0;
            if perfil.icms_st {
                retirar += icms;
            }
            if perfil.monofasico {
                retirar += pis_cofins;
            }
            // efetiva × (1 − retirar/10000), exato em i128.
            let un =
                i128::from(efetiva.unidades_internas()) * i128::from(10_000 - retirar) / 10_000;
            Percentual::unidades(i64::try_from(un).unwrap_or(0))
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn pct(p: i64) -> Percentual {
        Percentual::pontos(p)
    }

    #[test]
    fn exemplo_do_lunno_50_reais_viram_96_15() {
        // opex 25 + margem 15 + imposto 8 = 48 → divisor 0,52 → 50 / 0,52 = 96,15
        let c = Componentes {
            custo_operacional: pct(25),
            margem_liquida: pct(15),
            imposto: pct(8),
            ..Componentes::default()
        };
        assert_eq!(divisor(&c).unwrap(), pct(52));
        let p = preco_sugerido(Dinheiro::reais(50), &c).unwrap();
        assert_eq!(p.preco, Dinheiro::centavos(9615));
        // O que cada real cobrado paga: bate com o preço sem sobra nem falta.
        assert_eq!(
            p.custo + p.imposto + p.custo_operacional + p.comissao + p.taxa_pagamento + p.margem,
            p.preco
        );
        assert_eq!(p.imposto, Dinheiro::centavos(769)); // 8% de 96,15
        assert_eq!(p.margem.em_centavos(), 1_442); // 15% de 96,15 = 14,42 (absorve o arredondamento)
    }

    #[test]
    fn comissao_e_taxa_de_cartao_encarecem_o_preco() {
        let base = Componentes {
            custo_operacional: pct(20),
            margem_liquida: pct(10),
            imposto: pct(8),
            ..Componentes::default()
        };
        let com_extras = Componentes {
            comissao: pct(3),
            taxa_pagamento: Percentual::milesimos(2_500),
            ..base
        };
        let a = preco_sugerido(Dinheiro::reais(100), &base).unwrap().preco;
        let b = preco_sugerido(Dinheiro::reais(100), &com_extras)
            .unwrap()
            .preco;
        assert!(b > a);
    }

    #[test]
    fn painel_de_margem_separa_bruta_de_liquida_e_nao_chama_item_sem_custo_de_lucro() {
        let item = |rotulo: &str, q: i64, receita: i64, custo: Option<i64>| ItemDeMargem {
            rotulo: rotulo.into(),
            quantidade: Quantidade::unidades(q),
            receita: Dinheiro::reais(receita),
            custo_unitario: custo.map(Dinheiro::reais),
            encargos: pct(33), // 25 de operação + 8 de imposto
        };
        let p = painel_de_margem(&[
            item("A", 2, 200, Some(50)), // custo 100 → bruta 100, encargos 66 → líquida 34
            item("B", 1, 100, Some(90)), // bruta 10, encargos 33 → líquida −23: prejuízo
            item("C", 1, 80, None),      // sem custo: fora das margens
        ]);
        assert_eq!(p.itens[0].margem_bruta, Some(Dinheiro::reais(100)));
        assert_eq!(p.itens[0].margem_liquida, Some(Dinheiro::reais(34)));
        assert_eq!(p.itens[0].margem_liquida_pct.unwrap().formatar(0), "17");
        assert_eq!(p.itens[1].margem_liquida, Some(Dinheiro::reais(-23)));
        assert_eq!(p.itens[2].margem_liquida, None);
        assert_eq!(p.itens_sem_custo, 1);
        assert_eq!(p.receita, Dinheiro::reais(380));
        assert_eq!(p.margem_bruta, Dinheiro::reais(110));
        assert_eq!(p.margem_liquida, Dinheiro::reais(11));
        // 11 sobre os R$ 300 que têm custo (o item C não entra).
        assert_eq!(p.margem_liquida_pct.unwrap().formatar(1), "3,6");
        assert_eq!(painel_de_margem(&[]).margem_liquida_pct, None);
    }

    #[test]
    fn arredondamento_de_prateleira_sobe_e_nunca_desce() {
        use ArredondamentoDePreco::{FinalNoventa, InteiroAcima, Nenhum};
        let c = Dinheiro::centavos;
        assert_eq!(arredondar_preco(c(9_686), Nenhum), c(9_686));
        assert_eq!(arredondar_preco(c(9_686), FinalNoventa), c(9_690));
        assert_eq!(arredondar_preco(c(9_690), FinalNoventa), c(9_690));
        assert_eq!(arredondar_preco(c(9_691), FinalNoventa), c(9_790));
        assert_eq!(arredondar_preco(c(9_610), InteiroAcima), c(9_700));
        assert_eq!(arredondar_preco(c(9_700), InteiroAcima), c(9_700));
        for centavos in [1, 99, 100, 1_234, 9_999] {
            for modo in [Nenhum, FinalNoventa, InteiroAcima] {
                assert!(arredondar_preco(c(centavos), modo) >= c(centavos));
            }
        }
    }

    #[test]
    fn soma_de_100_por_cento_ou_mais_e_recusada() {
        let c = Componentes {
            custo_operacional: pct(60),
            margem_liquida: pct(30),
            imposto: pct(10),
            ..Componentes::default()
        };
        assert!(matches!(divisor(&c), Err(ErroPreco::SomaNaoFecha { .. })));
        assert!(preco_sugerido(Dinheiro::reais(10), &c)
            .unwrap_err()
            .to_string()
            .contains("somam 100"));
        let neg = Componentes {
            imposto: pct(-1),
            ..Componentes::default()
        };
        assert_eq!(divisor(&neg), Err(ErroPreco::PercentualNegativo));
        assert_eq!(
            preco_sugerido(Dinheiro::ZERO, &Componentes::default()),
            Err(ErroPreco::CustoInvalido)
        );
    }

    #[test]
    fn preco_minimo_nao_tem_lucro_e_margem_do_preco_confirma() {
        let c = Componentes {
            custo_operacional: pct(25),
            margem_liquida: pct(15),
            imposto: pct(8),
            ..Componentes::default()
        };
        let minimo = preco_minimo(Dinheiro::reais(50), &c).unwrap();
        // divisor 0,67 → 74,63
        assert_eq!(minimo.preco, Dinheiro::centavos(7463));
        assert!(
            minimo.margem.em_centavos().abs() <= 1,
            "empate, só o arredondamento sobra"
        );
        // Vender abaixo do mínimo dá margem negativa; no sugerido, ~15%.
        let abaixo = margem_do_preco(Dinheiro::reais(60), Dinheiro::reais(50), &c).unwrap();
        assert!(abaixo.unidades_internas() < 0);
        let no_sugerido =
            margem_do_preco(Dinheiro::centavos(9615), Dinheiro::reais(50), &c).unwrap();
        let desvio = (no_sugerido.unidades_internas() - pct(15).unidades_internas()).abs();
        assert!(
            desvio < 50_000,
            "margem no preço sugerido ≈ 15% (arredondamento ao centavo)"
        );
        assert_eq!(
            margem_do_preco(Dinheiro::ZERO, Dinheiro::reais(1), &c),
            None
        );
    }

    #[test]
    fn simples_anexo_i_faixa_1_e_a_nominal() {
        assert_eq!(
            aliquota_efetiva(AnexoSimples::I, Dinheiro::reais(100_000)),
            pct(4)
        );
        assert_eq!(aliquota_efetiva(AnexoSimples::I, Dinheiro::ZERO), pct(4));
        assert_eq!(
            aliquota_efetiva(AnexoSimples::I, Dinheiro::reais(180_000)),
            pct(4)
        );
    }

    #[test]
    fn simples_anexo_i_faixa_2_desconta_a_parcela_a_deduzir() {
        // RBT12 = 300.000 → (300.000 × 7,3% − 5.940) ÷ 300.000 = 5,32%
        assert_eq!(
            aliquota_efetiva(AnexoSimples::I, Dinheiro::reais(300_000)).formatar(2),
            "5,32"
        );
        // Anexo III, RBT12 = 300.000 → (300.000 × 11,2% − 9.360) ÷ 300.000 = 8,08%
        assert_eq!(
            aliquota_efetiva(AnexoSimples::III, Dinheiro::reais(300_000)).formatar(2),
            "8,08"
        );
    }

    #[test]
    fn efetiva_cresce_com_a_faixa_e_nunca_passa_da_nominal() {
        for a in [
            AnexoSimples::I,
            AnexoSimples::II,
            AnexoSimples::III,
            AnexoSimples::IV,
            AnexoSimples::V,
        ] {
            let mut anterior = Percentual::ZERO;
            // Só até a 5ª faixa: na 6ª o ICMS/ISS sai do DAS e a alíquota efetiva cai (ver doc).
            for reais in [50_000, 200_000, 500_000, 1_000_000, 2_500_000, 3_500_000] {
                let e = aliquota_efetiva(a, Dinheiro::reais(reais));
                assert!(e >= anterior, "{a:?} em {reais}");
                let (_, nominal, _) = tabela(a)[indice_da_faixa(a, Dinheiro::reais(reais))];
                assert!(e.unidades_internas() <= nominal * 10_000);
                anterior = e;
            }
        }
    }

    #[test]
    fn st_e_monofasico_tiram_a_parcela_da_reparticao() {
        let rbt = Dinheiro::reais(300_000); // faixa 2: ICMS 34,00%, PIS/COFINS 15,50%
        let normal = carga_sobre_venda(
            RegimeImposto::Simples(AnexoSimples::I),
            rbt,
            PerfilFiscal::default(),
        );
        let st = carga_sobre_venda(
            RegimeImposto::Simples(AnexoSimples::I),
            rbt,
            PerfilFiscal {
                icms_st: true,
                monofasico: false,
            },
        );
        let ambos = carga_sobre_venda(
            RegimeImposto::Simples(AnexoSimples::I),
            rbt,
            PerfilFiscal {
                icms_st: true,
                monofasico: true,
            },
        );
        assert_eq!(normal.formatar(2), "5,32");
        assert_eq!(st.formatar(2), "3,51"); // 5,32 × 66%
        assert_eq!(ambos, Percentual::unidades(2_686_600)); // 5,32 × 50,5%
        assert!(ambos < st && st < normal);
    }

    #[test]
    fn mei_nao_tem_percentual_e_manual_vale_o_informado() {
        let perfil = PerfilFiscal::default();
        assert_eq!(
            carga_sobre_venda(RegimeImposto::Mei, Dinheiro::reais(70_000), perfil),
            Percentual::ZERO
        );
        assert_eq!(
            carga_sobre_venda(RegimeImposto::Manual(pct(11)), Dinheiro::ZERO, perfil),
            pct(11)
        );
    }

    #[test]
    fn simples_servico_ignora_flags_de_st() {
        let r = RegimeImposto::Simples(AnexoSimples::III);
        let rbt = Dinheiro::reais(300_000);
        assert_eq!(
            carga_sobre_venda(
                r,
                rbt,
                PerfilFiscal {
                    icms_st: true,
                    monofasico: true
                }
            ),
            carga_sobre_venda(r, rbt, PerfilFiscal::default())
        );
    }
}
