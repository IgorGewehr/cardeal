//! O histórico das últimas compras de um produto: o que o fornecedor cobrou a cada entrada,
//! quanto variou sobre a compra anterior e a média das últimas três. Domínio puro — a consulta
//! (`consultas_compras.rs`) só lê as entradas do SQLite e entrega a lista a esta função.

use cardeal_kernel::{Arredondamento, Dinheiro, Instante, Percentual, Preco, Quantidade};
use serde::{Deserialize, Serialize};

/// Quantas compras o histórico mostra.
pub const COMPRAS_NO_HISTORICO: usize = 10;

/// Uma entrada de estoque como saiu do banco (a mais recente vem primeiro).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntradaBruta {
    /// Quando entrou.
    pub quando: Instante,
    /// Quanto entrou.
    pub quantidade: Quantidade,
    /// O custo unitário da entrada.
    pub custo: Preco,
    /// O módulo que originou a entrada (`compras`, `estoque`, `os`…).
    pub origem_modulo: String,
}

/// Uma compra do histórico.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompraDoHistorico {
    /// Quando entrou.
    pub quando: Instante,
    /// Quanto entrou.
    pub quantidade: Quantidade,
    /// O custo unitário.
    pub custo_unitario: Dinheiro,
    /// Variação do custo sobre a compra anterior; `None` na primeira compra conhecida.
    pub variacao: Option<Percentual>,
    /// O módulo que originou a entrada.
    pub origem_modulo: String,
}

/// O histórico e seus resumos.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumoDeCompras {
    /// As compras, da mais recente para a mais antiga.
    pub compras: Vec<CompraDoHistorico>,
    /// Média simples do custo das três compras mais recentes (menos, se houver menos).
    pub media_ultimas_3: Option<Dinheiro>,
    /// O custo da compra mais recente.
    pub ultimo_custo: Option<Dinheiro>,
}

fn em_dinheiro(p: Preco) -> Dinheiro {
    Dinheiro::de_total(Quantidade::unidades(1), p, Arredondamento::MeioAcima)
}

/// A variação de `atual` sobre `anterior`, em percentual (negativa = ficou mais barato).
/// `None` se a compra anterior não tem custo positivo.
#[must_use]
pub fn variacao_de_custo(atual: Preco, anterior: Preco) -> Option<Percentual> {
    let base = anterior.unidades_internas();
    (base > 0).then(|| Percentual::da_razao(atual.unidades_internas() - base, base))
}

/// Monta o histórico a partir das entradas, da mais recente para a mais antiga. Recebe uma
/// entrada a mais que o limite quando houver, para calcular a variação da última exibida.
#[must_use]
pub fn montar_historico(entradas: &[EntradaBruta]) -> ResumoDeCompras {
    let compras: Vec<CompraDoHistorico> = entradas
        .iter()
        .take(COMPRAS_NO_HISTORICO)
        .enumerate()
        .map(|(i, e)| CompraDoHistorico {
            quando: e.quando,
            quantidade: e.quantidade,
            custo_unitario: em_dinheiro(e.custo),
            variacao: entradas
                .get(i + 1)
                .and_then(|anterior| variacao_de_custo(e.custo, anterior.custo)),
            origem_modulo: e.origem_modulo.clone(),
        })
        .collect();
    let recentes: Vec<i64> = entradas
        .iter()
        .take(3)
        .map(|e| e.custo.unidades_internas())
        .collect();
    let media_ultimas_3 = (!recentes.is_empty()).then(|| {
        let soma: i128 = recentes.iter().map(|v| i128::from(*v)).sum();
        let n = recentes.len() as i128;
        let media = i64::try_from((soma * 2 + n) / (n * 2)).unwrap_or(i64::MAX);
        em_dinheiro(Preco::interna(media))
    });
    ResumoDeCompras {
        ultimo_custo: compras.first().map(|c| c.custo_unitario),
        media_ultimas_3,
        compras,
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn entrada(micros: i64, centavos: i64) -> EntradaBruta {
        EntradaBruta {
            quando: Instante::de_micros(micros),
            quantidade: Quantidade::unidades(1),
            custo: Preco::centavos(centavos),
            origem_modulo: "estoque".into(),
        }
    }

    #[test]
    fn variacao_e_sobre_a_compra_anterior() {
        let v = variacao_de_custo(Preco::centavos(5_500), Preco::centavos(5_000));
        assert_eq!(v, Some(Percentual::pontos(10)));
        let v = variacao_de_custo(Preco::centavos(4_500), Preco::centavos(5_000));
        assert_eq!(v, Some(Percentual::pontos(-10)));
        assert_eq!(
            variacao_de_custo(Preco::centavos(100), Preco::centavos(0)),
            None
        );
    }

    #[test]
    fn historico_vazio_nao_tem_resumo() {
        let r = montar_historico(&[]);
        assert!(r.compras.is_empty());
        assert_eq!((r.media_ultimas_3, r.ultimo_custo), (None, None));
    }

    #[test]
    fn primeira_compra_nao_tem_variacao_e_a_media_usa_as_tres_ultimas() {
        // da mais recente para a mais antiga: 60, 50, 40, 100 (a antiga não entra na média)
        let r = montar_historico(&[
            entrada(4, 6_000),
            entrada(3, 5_000),
            entrada(2, 4_000),
            entrada(1, 10_000),
        ]);
        assert_eq!(r.ultimo_custo, Some(Dinheiro::reais(60)));
        assert_eq!(r.media_ultimas_3, Some(Dinheiro::reais(50)));
        let v: Vec<_> = r.compras.iter().map(|c| c.variacao).collect();
        assert_eq!(
            v,
            [
                Some(Percentual::pontos(20)),
                Some(Percentual::pontos(25)),
                Some(Percentual::pontos(-60)),
                None
            ]
        );
    }

    #[test]
    fn limita_a_dez_mas_calcula_a_variacao_da_decima_com_a_onze() {
        let entradas: Vec<_> = (0..12).map(|i| entrada(100 - i, 1_000 + i * 10)).collect();
        let r = montar_historico(&entradas);
        assert_eq!(r.compras.len(), COMPRAS_NO_HISTORICO);
        assert!(r.compras[9].variacao.is_some());
    }
}
