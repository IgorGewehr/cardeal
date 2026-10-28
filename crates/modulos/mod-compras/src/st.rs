//! A **estimativa do ICMS-ST pela MVA** (onda 5): o produto sujeito à substituição tributária
//! (o perfil fiscal tem a MVA-ST e a alíquota interna) que chega numa nota **sem** o ST — a compra
//! de outro estado sem convênio, o fornecedor que não é substituto — deixa a antecipação para
//! quem compra, e ela é custo. A conta (Convênio ICMS 52/2017):
//! - base do ST = valor da operação (produto − desconto + frete, seguro e outras + IPI) ×
//!   (1 + MVA); na compra interestadual, a MVA **ajustada**:
//!   `(1 + MVA) × (1 − alíquota interestadual) / (1 − alíquota interna) − 1`;
//! - ST = base do ST × alíquota interna − o ICMS da operação de origem (o destacado na nota, ou o
//!   valor × a alíquota de origem).
//!
//! É estimativa: a nota com ela não se confirma sozinha, a conferência mostra item a item e quem
//! confirma decide se ela entra no custo (com a guia a pagar ao estado). A lição do gestao-raiz:
//! ST por MVA se detecta e vai para revisão humana, não se calcula de improviso.

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
use cardeal_kernel::{Arredondamento, Dinheiro, Id, Percentual};
use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// A estimativa do ST de um item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EstimativaSt {
    /// O item da nota.
    pub item: Id,
    /// A descrição do fornecedor.
    pub descricao: String,
    /// O valor da operação.
    pub valor_operacao: Dinheiro,
    /// A MVA aplicada (a ajustada, na compra interestadual).
    pub mva: Percentual,
    /// A base do ST.
    pub base_st: Dinheiro,
    /// A alíquota interna.
    pub aliquota_interna: Percentual,
    /// O ICMS da operação de origem (o crédito).
    pub credito: Dinheiro,
    /// O ST estimado.
    pub valor: Dinheiro,
}

/// A MVA ajustada da compra interestadual (Convênio ICMS 52/2017), com duas casas, como as
/// tabelas publicam.
#[must_use]
pub fn mva_ajustada(mva: Percentual, interestadual: Percentual, interna: Percentual) -> Percentual {
    // Unidades internas: 100% = `Percentual::CEM`.
    let um = i128::from(Percentual::CEM.unidades_internas());
    let (m, ai, an) = (
        i128::from(mva.unidades_internas()),
        i128::from(interestadual.unidades_internas()),
        i128::from(interna.unidades_internas()),
    );
    if an >= um {
        return mva;
    }
    // ((1 + m) × (1 − ai) / (1 − an) − 1), arredondado a 0,01 ponto.
    let num = (um + m) * (um - ai);
    let den = um - an;
    let exata = (2 * num + den) / (2 * den) - um;
    let passo = um / 10_000; // 0,01%
    let arredondada = (2 * exata + passo) / (2 * passo) * passo;
    Percentual::unidades(i64::try_from(arredondada).unwrap_or(0))
}

/// O ST de um valor de operação: `(mva aplicada, base, crédito, ST)`. `origem`: a alíquota e o
/// ICMS da operação de origem, quando a nota os destaca. `interestadual`: a compra é de outro
/// estado (a MVA é ajustada).
#[must_use]
pub fn estimar_st(
    valor_operacao: Dinheiro,
    mva: Percentual,
    aliquota_interna: Percentual,
    origem: (Option<Percentual>, Option<Dinheiro>),
    interestadual: bool,
) -> (Percentual, Dinheiro, Dinheiro, Dinheiro) {
    let (aliquota_origem, icms_origem) = origem;
    let aliquota_origem = aliquota_origem.unwrap_or(aliquota_interna);
    let mva = if interestadual && aliquota_origem < aliquota_interna {
        mva_ajustada(mva, aliquota_origem, aliquota_interna)
    } else {
        mva
    };
    let base = valor_operacao + valor_operacao.aplicar(mva, Arredondamento::MeioAcima);
    let credito = icms_origem
        .filter(|c| c.e_positivo())
        .unwrap_or_else(|| valor_operacao.aplicar(aliquota_origem, Arredondamento::MeioAcima));
    let bruto = base.aplicar(aliquota_interna, Arredondamento::MeioAcima);
    let st = (bruto - credito).nao_negativo();
    (mva, base, credito, st)
}

/// As estimativas de ST dos itens de uma nota (vazio: nenhum item pede).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EstimativaDeStDaNota {
    /// A nota de entrada.
    pub nota_entrada: Id,
}

impl Consulta for EstimativaDeStDaNota {
    type Saida = Vec<EstimativaSt>;
    const PERMISSAO: &'static str = "compras.entrada.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Vec<EstimativaSt>> {
        sqlite::estimativas(c, ctx, self.nota_entrada)
    }
}

#[cfg(feature = "sqlite")]
pub(crate) mod sqlite {
    use cardeal_kernel::{Dinheiro, Id, Resultado};
    use cardeal_modkit::Ctx;
    use rusqlite::Connection;

    use super::{estimar_st, EstimativaSt};

    /// As situações (CST/CSOSN) em que o ST já foi pago antes ou vem na nota.
    const COM_ST: &[&str] = &["10", "30", "60", "70", "201", "202", "203", "500"];

    /// As estimativas da nota: os itens casados com produto que tem MVA-ST e alíquota, e que
    /// vieram sem ST (nem cobrado nesta nota, nem retido antes).
    pub(crate) fn estimativas(c: &Connection, ctx: &Ctx, nota: Id) -> Resultado<Vec<EstimativaSt>> {
        let itens = crate::repositorio::itens_da_nota_por_conexao(c, nota)?;
        let produtos: Vec<Id> = itens.iter().filter_map(|i| i.produto_casado).collect();
        if produtos.is_empty() {
            return Ok(Vec::new());
        }
        let perfis = mod_estoque::produtos_para_nota(c, ctx.organizacao, &produtos)?;
        let xml = crate::repositorio::xml_da_nota_por_conexao(c, nota)?
            .and_then(|x| cardeal_fiscal::interpretar(&x).ok());
        // A compra é de outro estado: a UF da chave (os dois primeiros dígitos) não é a nossa.
        // Sem os dados fiscais (ou a UF), a compra conta como de dentro do estado.
        let uf_da_empresa = mod_empresa::sql::dados_fiscais(c, ctx.empresa)
            .ok()
            .and_then(|d| d.normalizado().ok())
            .and_then(|f| f.uf())
            .map(cardeal_kernel::Uf::codigo_ibge);
        let interestadual = match (&xml, uf_da_empresa) {
            (Some(x), Some(uf)) => {
                x.chave_acesso
                    .como_str()
                    .get(..2)
                    .and_then(|c| c.parse::<u8>().ok())
                    != Some(uf)
            }
            _ => false,
        };
        let mut v = Vec::new();
        for (i, item) in itens.iter().enumerate() {
            let Some(cadastro) = item.produto_casado.and_then(|p| perfis.get(&p)) else {
                continue;
            };
            let (Some(mva), Some(interna)) =
                (cadastro.fiscal.mva_st, cadastro.fiscal.aliquota_icms)
            else {
                continue;
            };
            if mva.unidades_internas() <= 0 {
                continue;
            }
            let do_xml = xml.as_ref().and_then(|x| {
                x.itens
                    .get(i)
                    .filter(|xi| xi.codigo_fornecedor == item.codigo_fornecedor)
            });
            let (ipi, origem) = match do_xml {
                Some(xi) => {
                    let t = &xi.tributos;
                    let veio_st = (t.valor_st + t.valor_fcp_st).e_positivo()
                        || t.situacao.as_deref().is_some_and(|s| COM_ST.contains(&s));
                    if veio_st {
                        continue;
                    }
                    (
                        xi.valor_ipi,
                        (
                            t.aliquota_icms,
                            Some(t.valor_icms).filter(|v| v.e_positivo()),
                        ),
                    )
                }
                // A nota lançada à mão: sem ST informado é o que ela diz.
                None if item.valor_impostos.e_positivo() => continue,
                None => (Dinheiro::ZERO, (None, None)),
            };
            let valor_operacao = item.valor_bruto() - item.valor_desconto + item.valor_rateio + ipi;
            let (mva_aplicada, base_st, credito, valor) =
                estimar_st(valor_operacao, mva, interna, origem, interestadual);
            if valor.e_positivo() {
                v.push(EstimativaSt {
                    item: item.id,
                    descricao: item.descricao_fornecedor.clone(),
                    valor_operacao,
                    mva: mva_aplicada,
                    base_st,
                    aliquota_interna: interna,
                    credito,
                    valor,
                });
            }
        }
        Ok(v)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn p(t: &str) -> Percentual {
        Percentual::de_str(t).unwrap()
    }

    #[test]
    fn dentro_do_estado_a_mva_original() {
        // R$ 100 a 40% de MVA, interna 17%, origem 17% (ICMS R$ 17): base 140, ST 23,80 − 17.
        let (mva, base, credito, st) = estimar_st(
            Dinheiro::reais(100),
            p("40"),
            p("17"),
            (Some(p("17")), None),
            false,
        );
        assert_eq!(mva, p("40"));
        assert_eq!(base, Dinheiro::reais(140));
        assert_eq!(credito, Dinheiro::reais(17));
        assert_eq!(st, Dinheiro::centavos(680));
    }

    #[test]
    fn de_outro_estado_a_mva_ajustada() {
        // MVA 40%, interestadual 12%, interna 17%: (1,4 × 0,88 / 0,83) − 1 = 48,43%.
        assert_eq!(mva_ajustada(p("40"), p("12"), p("17")), p("48,43"));
        let (mva, base, credito, st) = estimar_st(
            Dinheiro::reais(100),
            p("40"),
            p("17"),
            (Some(p("12")), Some(Dinheiro::reais(12))),
            true,
        );
        assert_eq!(mva, p("48,43"));
        assert_eq!(base, Dinheiro::centavos(14_843));
        assert_eq!(credito, Dinheiro::reais(12));
        // 148,43 × 17% = 25,23 − 12 = 13,23
        assert_eq!(st, Dinheiro::centavos(1_323));
    }

    #[test]
    fn nunca_negativo() {
        let (_, _, _, st) = estimar_st(
            Dinheiro::reais(100),
            p("1"),
            p("4"),
            (Some(p("17")), None),
            false,
        );
        assert_eq!(st, Dinheiro::ZERO);
    }
}
