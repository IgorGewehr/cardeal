//! Finanças pessoais do usuário — a chave "Pessoal" do Financeiro: salário/pró-labore, vendas
//! por fora, contas do mês, faculdade, faturas do cartão com compras parceladas. **Fora da
//! contabilidade da empresa**: nada aqui toca o Razão nem os títulos; é um livro simples de
//! previsto × pago, privado (cada usuário só vê os seus), para projetar os próximos meses.
//! Domínio puro neste arquivo; SQL em [`consultas`]/[`comandos`].

pub mod comandos;
pub mod consultas;

use std::collections::BTreeMap;

use cardeal_kernel::{CodigoErro, Competencia, Data, Dinheiro, Erro, Id, Resultado};
use serde::{Deserialize, Serialize};

/// Entrada ou saída.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TipoPessoal {
    /// Salário, pró-labore, venda por fora.
    Receita,
    /// Conta, compra, fatura, mensalidade.
    Despesa,
}

/// Como o valor se repete.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Repeticao {
    /// Uma vez.
    Unica,
    /// O valor total dividido em `vezes` meses (compra parcelada no cartão).
    Parcelada {
        /// De 2 a 120.
        vezes: u16,
    },
    /// O valor cheio todo mês por `meses` meses (faculdade, aluguel, salário).
    Mensal {
        /// De 1 a 120.
        meses: u16,
    },
}

/// Um lançamento pessoal (uma parcela/ocorrência).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LancamentoPessoal {
    /// Identidade.
    pub id: Id,
    /// As parcelas/ocorrências de um mesmo lançamento compartilham o grupo.
    pub grupo: Id,
    /// Receita ou despesa.
    pub tipo: TipoPessoal,
    /// "Faculdade", "Notebook (Magalu)".
    pub descricao: String,
    /// Categoria livre: "Moradia", "Educação", "Cartão"…
    pub categoria: String,
    /// O valor desta parcela/ocorrência.
    pub valor: Dinheiro,
    /// Quando vence (ou entra).
    pub vencimento: Data,
    /// Esta é a parcela `parcela` de `parcelas`.
    pub parcela: u16,
    /// Total de parcelas/ocorrências do grupo.
    pub parcelas: u16,
    /// O cartão de crédito ("Nubank"), quando a despesa cai numa fatura.
    pub cartao: Option<String>,
    /// Quando foi pago/recebido; `None` = previsto.
    pub pago_em: Option<Data>,
}

impl LancamentoPessoal {
    /// Já pago/recebido.
    #[must_use]
    pub const fn pago(&self) -> bool {
        self.pago_em.is_some()
    }

    /// "3/10" quando o grupo tem mais de uma parcela.
    #[must_use]
    pub fn rotulo_parcela(&self) -> Option<String> {
        (self.parcelas > 1).then(|| format!("{}/{}", self.parcela, self.parcelas))
    }
}

/// O que o usuário preenche.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovoPessoal {
    /// Receita ou despesa.
    pub tipo: TipoPessoal,
    /// Descrição.
    pub descricao: String,
    /// Categoria livre.
    pub categoria: String,
    /// Valor total (parcelada) ou de cada ocorrência (única/mensal).
    pub valor: Dinheiro,
    /// O primeiro vencimento (na compra no cartão, o da primeira fatura).
    pub primeiro_vencimento: Data,
    /// Como se repete.
    pub repeticao: Repeticao,
    /// O cartão, se for no crédito.
    pub cartao: Option<String>,
    /// A primeira já está paga/recebida.
    pub primeira_paga: bool,
}

fn invalido(msg: &str) -> Erro {
    Erro::novo(CodigoErro::VALOR_INVALIDO, msg)
}

/// Gera as parcelas/ocorrências de um lançamento, mês a mês a partir do primeiro vencimento.
///
/// # Errors
/// Descrição vazia, valor não positivo ou quantidade de parcelas/meses fora de 1..=120.
pub fn gerar(n: &NovoPessoal, hoje: Data) -> Resultado<Vec<LancamentoPessoal>> {
    let descricao = n.descricao.trim();
    if descricao.is_empty() {
        return Err(invalido("informe a descrição").no_campo("descricao"));
    }
    if !n.valor.e_positivo() {
        return Err(invalido("o valor precisa ser maior que zero").no_campo("valor"));
    }
    let valores = match n.repeticao {
        Repeticao::Unica => vec![n.valor],
        Repeticao::Parcelada { vezes } if (2..=120).contains(&vezes) => {
            n.valor.ratear(usize::from(vezes))
        }
        Repeticao::Mensal { meses } if (1..=120).contains(&meses) => {
            vec![n.valor; usize::from(meses)]
        }
        _ => return Err(invalido("parcelas/meses: de 1 a 120").no_campo("repeticao")),
    };
    let total = u16::try_from(valores.len()).unwrap_or(u16::MAX);
    let grupo = Id::novo();
    let categoria = match n.categoria.trim() {
        "" => "Outros".to_owned(),
        c => c.to_owned(),
    };
    let cartao = n
        .cartao
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .map(str::to_owned);
    Ok(valores
        .into_iter()
        .enumerate()
        .map(|(i, valor)| {
            let i = u16::try_from(i).unwrap_or(u16::MAX);
            LancamentoPessoal {
                id: Id::novo(),
                grupo,
                tipo: n.tipo,
                descricao: descricao.to_owned(),
                categoria: categoria.clone(),
                valor,
                vencimento: n.primeiro_vencimento.mais_meses(i32::from(i)),
                parcela: i + 1,
                parcelas: total,
                cartao: cartao.clone(),
                pago_em: (i == 0 && n.primeira_paga).then_some(hoje),
            }
        })
        .collect())
}

/// Um mês da projeção pessoal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MesPessoal {
    /// O mês.
    pub competencia: Competencia,
    /// Tudo o que entra no mês.
    pub receitas: Dinheiro,
    /// Tudo o que sai no mês (cartão incluído).
    pub despesas: Dinheiro,
    /// A parte das despesas que é fatura de cartão.
    pub cartao: Dinheiro,
    /// Despesas do mês ainda não pagas.
    pub a_pagar: Dinheiro,
    /// Receitas do mês ainda não recebidas.
    pub a_receber: Dinheiro,
}

impl MesPessoal {
    /// Receitas − despesas.
    #[must_use]
    pub fn saldo(&self) -> Dinheiro {
        self.receitas - self.despesas
    }
}

/// Os `meses` meses a partir de `de`, somando os lançamentos que vencem em cada um (meses sem
/// nada aparecem zerados — a projeção não pula mês).
#[must_use]
pub fn projecao_mensal(
    lancamentos: &[LancamentoPessoal],
    de: Competencia,
    meses: u8,
) -> Vec<MesPessoal> {
    let mut v: Vec<MesPessoal> = Vec::with_capacity(usize::from(meses));
    let mut c = de;
    for _ in 0..meses {
        v.push(MesPessoal {
            competencia: c,
            ..MesPessoal::default()
        });
        c = c.proxima();
    }
    for l in lancamentos {
        let Some(m) = v
            .iter_mut()
            .find(|m| m.competencia == l.vencimento.competencia())
        else {
            continue;
        };
        match l.tipo {
            TipoPessoal::Receita => {
                m.receitas += l.valor;
                if !l.pago() {
                    m.a_receber += l.valor;
                }
            }
            TipoPessoal::Despesa => {
                m.despesas += l.valor;
                if l.cartao.is_some() {
                    m.cartao += l.valor;
                }
                if !l.pago() {
                    m.a_pagar += l.valor;
                }
            }
        }
    }
    v
}

/// As despesas de um mês por categoria, da maior para a menor.
#[must_use]
pub fn despesas_por_categoria(
    lancamentos: &[LancamentoPessoal],
    competencia: Competencia,
) -> Vec<(String, Dinheiro)> {
    let mut m: BTreeMap<&str, Dinheiro> = BTreeMap::new();
    for l in lancamentos
        .iter()
        .filter(|l| l.tipo == TipoPessoal::Despesa && l.vencimento.competencia() == competencia)
    {
        *m.entry(l.categoria.as_str()).or_insert(Dinheiro::ZERO) += l.valor;
    }
    let mut v: Vec<(String, Dinheiro)> = m.into_iter().map(|(k, d)| (k.to_owned(), d)).collect();
    v.sort_by_key(|c| std::cmp::Reverse(c.1));
    v
}

/// A fatura de cada cartão num mês: (cartão, total, quanto falta pagar).
#[must_use]
pub fn faturas_do_mes(
    lancamentos: &[LancamentoPessoal],
    competencia: Competencia,
) -> Vec<(String, Dinheiro, Dinheiro)> {
    let mut m: BTreeMap<&str, (Dinheiro, Dinheiro)> = BTreeMap::new();
    for l in lancamentos
        .iter()
        .filter(|l| l.vencimento.competencia() == competencia && l.tipo == TipoPessoal::Despesa)
    {
        let Some(cartao) = l.cartao.as_deref() else {
            continue;
        };
        let e = m.entry(cartao).or_insert((Dinheiro::ZERO, Dinheiro::ZERO));
        e.0 += l.valor;
        if !l.pago() {
            e.1 += l.valor;
        }
    }
    m.into_iter()
        .map(|(c, (total, aberto))| (c.to_owned(), total, aberto))
        .collect()
}

#[cfg(test)]
mod testes {
    use super::*;

    fn data(s: &str) -> Data {
        s.parse().expect("data")
    }

    fn novo(repeticao: Repeticao, valor: i64) -> NovoPessoal {
        NovoPessoal {
            tipo: TipoPessoal::Despesa,
            descricao: "Parcelamentos anteriores".to_owned(),
            categoria: "Cartão".to_owned(),
            valor: Dinheiro::reais(valor),
            primeiro_vencimento: data("10/10/2026"),
            repeticao,
            cartao: Some("Nubank".to_owned()),
            primeira_paga: false,
        }
    }

    #[test]
    fn parcelada_divide_o_total_e_mensal_repete_o_valor() {
        let hoje = data("30/09/2026");
        let p = gerar(&novo(Repeticao::Parcelada { vezes: 3 }, 100), hoje).expect("ok");
        assert_eq!(p.len(), 3);
        let soma = p.iter().fold(Dinheiro::ZERO, |a, l| a + l.valor);
        assert_eq!(soma, Dinheiro::reais(100));
        assert_eq!(p[2].vencimento, data("10/12/2026"));
        assert_eq!(p[1].rotulo_parcela().as_deref(), Some("2/3"));

        let m = gerar(&novo(Repeticao::Mensal { meses: 10 }, 800), hoje).expect("ok");
        assert_eq!(m.len(), 10);
        assert!(m.iter().all(|l| l.valor == Dinheiro::reais(800)));
        assert_eq!(m[9].vencimento, data("10/07/2027"));
    }

    #[test]
    fn invalidos_sao_recusados() {
        let hoje = data("30/09/2026");
        assert!(gerar(&novo(Repeticao::Parcelada { vezes: 1 }, 100), hoje).is_err());
        assert!(gerar(&novo(Repeticao::Unica, 0), hoje).is_err());
        let mut sem = novo(Repeticao::Unica, 10);
        sem.descricao = "  ".to_owned();
        assert!(gerar(&sem, hoje).is_err());
    }

    #[test]
    fn projecao_soma_por_mes_e_separa_cartao() {
        let hoje = data("30/09/2026");
        let mut l = gerar(&novo(Repeticao::Mensal { meses: 10 }, 800), hoje).expect("ok");
        let salario = NovoPessoal {
            tipo: TipoPessoal::Receita,
            descricao: "Pró-labore".to_owned(),
            categoria: "Pró-labore".to_owned(),
            valor: Dinheiro::reais(5000),
            primeiro_vencimento: data("05/10/2026"),
            repeticao: Repeticao::Mensal { meses: 12 },
            cartao: None,
            primeira_paga: true,
        };
        l.extend(gerar(&salario, hoje).expect("ok"));
        let p = projecao_mensal(&l, Competencia::nova(2026, 10), 12);
        assert_eq!(p.len(), 12);
        assert_eq!(p[0].receitas, Dinheiro::reais(5000));
        assert_eq!(p[0].a_receber, Dinheiro::ZERO);
        assert_eq!(p[0].cartao, Dinheiro::reais(800));
        assert_eq!(p[0].saldo(), Dinheiro::reais(4200));
        // O parcelamento acaba em julho/27: agosto só tem o salário.
        assert_eq!(p[10].despesas, Dinheiro::ZERO);
        let f = faturas_do_mes(&l, Competencia::nova(2026, 10));
        assert_eq!(
            f,
            vec![(
                "Nubank".to_owned(),
                Dinheiro::reais(800),
                Dinheiro::reais(800)
            )]
        );
        let c = despesas_por_categoria(&l, Competencia::nova(2026, 10));
        assert_eq!(c, vec![("Cartão".to_owned(), Dinheiro::reais(800))]);
    }
}
