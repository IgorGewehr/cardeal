//! Análise do estoque: curva ABC, cobertura em dias e giro, a partir do que realmente saiu
//! numa janela de dias. Responde "o que compensa ter sempre na prateleira", "quanto dura o que
//! tenho" e "o que está parado". Domínio puro sobre os números já agregados; a consulta soma as
//! saídas de `estoque_movimento` (venda, PDV e OS passam todas por lá).
//!
//! A curva é por **valor de saída ao custo** (o que o estoque entregou): o estoque não conhece o
//! preço de venda. A classe A vai até 80% do valor acumulado, a B até 95% e a C o resto.

use cardeal_kernel::{Dinheiro, Id, Percentual, Quantidade};
use serde::{Deserialize, Serialize};

/// A cobertura abaixo disto (em dias) é risco de ruptura.
pub const DIAS_DE_RISCO_DE_RUPTURA: u32 = 7;

/// A classe na curva ABC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClasseAbc {
    /// Os poucos itens que respondem por 80% do valor.
    A,
    /// Os seguintes, até 95%.
    B,
    /// O resto, com saída.
    C,
    /// Nada saiu na janela.
    SemGiro,
}

/// Os números de um produto, já somados.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntradaDeAnalise {
    /// O produto.
    pub produto: Id,
    /// O nome.
    pub nome: String,
    /// O que saiu na janela.
    pub saidas: Quantidade,
    /// O valor das saídas ao custo.
    pub valor_saidas: Dinheiro,
    /// O saldo disponível hoje (todos os locais).
    pub saldo: Quantidade,
    /// O custo médio atual.
    pub custo: Dinheiro,
}

/// A linha da análise.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemDeAnalise {
    /// O produto.
    pub produto: Id,
    /// O nome.
    pub nome: String,
    /// A classe.
    pub classe: ClasseAbc,
    /// O que saiu na janela.
    pub saidas: Quantidade,
    /// O valor das saídas ao custo.
    pub valor_saidas: Dinheiro,
    /// A parte deste produto no valor total das saídas.
    pub participacao: Percentual,
    /// A participação acumulada até este produto (na ordem da curva).
    pub acumulada: Percentual,
    /// O saldo hoje.
    pub saldo: Quantidade,
    /// O valor parado em estoque (saldo × custo).
    pub valor_em_estoque: Dinheiro,
    /// Quantos dias o saldo dura no ritmo da janela; `None` sem saída.
    pub cobertura_dias: Option<u32>,
    /// O valor das saídas sobre o valor em estoque (200% = girou duas vezes na janela);
    /// `None` sem estoque ou sem saída.
    pub giro: Option<Percentual>,
    /// A cobertura é menor que [`DIAS_DE_RISCO_DE_RUPTURA`].
    pub risco_de_ruptura: bool,
}

/// Monta a análise: da maior para a menor saída em valor; sem giro por último.
#[must_use]
pub fn analisar(entradas: &[EntradaDeAnalise], dias: u32) -> Vec<ItemDeAnalise> {
    let dias = dias.max(1);
    let mut ordem: Vec<&EntradaDeAnalise> = entradas.iter().collect();
    ordem.sort_by(|a, b| {
        b.valor_saidas
            .cmp(&a.valor_saidas)
            .then_with(|| a.nome.cmp(&b.nome))
    });
    let total: i64 = ordem
        .iter()
        .map(|e| e.valor_saidas.em_centavos().max(0))
        .sum();
    let mut acumulado = 0_i64;
    ordem
        .into_iter()
        .map(|e| {
            let valor = e.valor_saidas.em_centavos().max(0);
            let antes = acumulado;
            acumulado += valor;
            let sem_giro = valor == 0 || !e.saidas.e_positiva();
            // A classe olha o acumulado ANTES do item: o primeiro sempre é A, e quem cruza o
            // limite ainda pertence à faixa em que começou.
            let classe = if sem_giro || total == 0 {
                ClasseAbc::SemGiro
            } else if antes * 100 < total * 80 {
                ClasseAbc::A
            } else if antes * 100 < total * 95 {
                ClasseAbc::B
            } else {
                ClasseAbc::C
            };
            let cobertura_dias = (!sem_giro && e.saldo.e_positiva()).then(|| {
                let d = i128::from(e.saldo.unidades_internas()) * i128::from(dias)
                    / i128::from(e.saidas.unidades_internas());
                u32::try_from(d).unwrap_or(u32::MAX)
            });
            let valor_em_estoque = Dinheiro::de_total(
                e.saldo.nao_negativa(),
                cardeal_kernel::Preco::centavos(e.custo.em_centavos()),
                cardeal_kernel::Arredondamento::MeioAcima,
            );
            ItemDeAnalise {
                produto: e.produto,
                nome: e.nome.clone(),
                classe,
                saidas: e.saidas,
                valor_saidas: e.valor_saidas,
                participacao: Percentual::da_razao(valor, total),
                acumulada: Percentual::da_razao(acumulado, total),
                saldo: e.saldo,
                valor_em_estoque,
                risco_de_ruptura: cobertura_dias.is_some_and(|c| c < DIAS_DE_RISCO_DE_RUPTURA),
                cobertura_dias,
                giro: (!sem_giro && valor_em_estoque.e_positivo())
                    .then(|| Percentual::da_razao(valor, valor_em_estoque.em_centavos())),
            }
        })
        .collect()
}

#[cfg(test)]
mod testes {
    use super::*;

    fn e(nome: &str, saidas: i64, valor: i64, saldo: i64) -> EntradaDeAnalise {
        EntradaDeAnalise {
            produto: Id::novo(),
            nome: nome.into(),
            saidas: Quantidade::unidades(saidas),
            valor_saidas: Dinheiro::reais(valor),
            saldo: Quantidade::unidades(saldo),
            custo: Dinheiro::reais(10),
        }
    }

    #[test]
    fn curva_abc_corta_em_80_e_95_pelo_acumulado_anterior() {
        // Total 1.000: 700 (A), 150 (A começou aos 70% <80), 100 (B: começou aos 85%),
        // 40 (C: começou aos 95%), 10 (C), 0 (sem giro).
        let a = analisar(
            &[
                e("c", 10, 100, 5),
                e("a", 70, 700, 5),
                e("b", 15, 150, 5),
                e("d", 4, 40, 5),
                e("e", 1, 10, 5),
                e("parado", 0, 0, 50),
            ],
            30,
        );
        let classes: Vec<(&str, ClasseAbc)> =
            a.iter().map(|i| (i.nome.as_str(), i.classe)).collect();
        assert_eq!(
            classes,
            [
                ("a", ClasseAbc::A),
                ("b", ClasseAbc::A),
                ("c", ClasseAbc::B),
                ("d", ClasseAbc::C),
                ("e", ClasseAbc::C),
                ("parado", ClasseAbc::SemGiro)
            ]
        );
        assert_eq!(a[0].participacao.formatar(0), "70");
        assert_eq!(a[2].acumulada.formatar(0), "95");
        assert_eq!(a[4].acumulada.formatar(0), "100");
    }

    #[test]
    fn cobertura_e_giro_seguem_o_ritmo_da_janela() {
        // 30 saídas em 30 dias = 1/dia; saldo 5 → 5 dias (risco); custo 10 → R$ 50 parados.
        let a = analisar(&[e("x", 30, 300, 5), e("y", 30, 300, 300)], 30);
        let x = a.iter().find(|i| i.nome == "x").unwrap();
        assert_eq!(x.cobertura_dias, Some(5));
        assert!(x.risco_de_ruptura);
        assert_eq!(x.valor_em_estoque, Dinheiro::reais(50));
        assert_eq!(x.giro.unwrap().formatar(0), "600"); // girou 6x na janela
        let y = a.iter().find(|i| i.nome == "y").unwrap();
        assert_eq!(y.cobertura_dias, Some(300));
        assert!(!y.risco_de_ruptura);
    }

    #[test]
    fn sem_movimento_nenhum_tudo_e_sem_giro_e_nao_divide_por_zero() {
        let a = analisar(&[e("p", 0, 0, 10), e("q", 0, 0, 0)], 30);
        assert!(a.iter().all(|i| i.classe == ClasseAbc::SemGiro));
        assert!(a
            .iter()
            .all(|i| i.cobertura_dias.is_none() && i.giro.is_none()));
        assert!(analisar(&[], 30).is_empty());
    }
}
