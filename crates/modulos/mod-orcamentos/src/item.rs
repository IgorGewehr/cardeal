//! Uma linha de orçamento — descrição livre, quantidade, preço e desconto, com o total já
//! calculado e congelado no item (mesmo princípio de `mod_vendas::ItemVenda`).

use cardeal_kernel::{Arredondamento, Dinheiro, Id, Percentual, Preco, Quantidade};
use serde::{Deserialize, Serialize};

use crate::erros::ErroOrcamentos;

/// Uma linha do orçamento.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemOrcamento {
    /// Identidade.
    pub id: Id,
    /// O orçamento dono.
    pub orcamento: Id,
    /// Posição na lista (0-based), para manter a ordem digitada.
    pub ordem: u32,
    /// Descrição livre (serviço, peça, etapa…).
    pub descricao: String,
    /// Quantidade (> 0).
    pub quantidade: Quantidade,
    /// Unidade (`un`, `h`, `m²`…). Pode ser vazia.
    pub unidade: String,
    /// Preço unitário cobrado.
    pub preco_unitario: Preco,
    /// Desconto da linha, em pontos percentuais.
    pub desconto_percentual: Percentual,
    /// Total da linha: `quantidade × preço − desconto`, congelado.
    pub total: Dinheiro,
}

impl ItemOrcamento {
    /// Monta uma linha calculando o total.
    ///
    /// # Errors
    /// [`ErroOrcamentos::DescricaoDeItemVazia`], [`ErroOrcamentos::QuantidadeInvalida`].
    pub fn novo(
        orcamento: Id,
        ordem: u32,
        descricao: impl Into<String>,
        quantidade: Quantidade,
        unidade: impl Into<String>,
        preco_unitario: Preco,
        desconto_percentual: Percentual,
    ) -> Result<Self, ErroOrcamentos> {
        let descricao = descricao.into().trim().to_owned();
        if descricao.is_empty() {
            return Err(ErroOrcamentos::DescricaoDeItemVazia);
        }
        if !quantidade.e_positiva() {
            return Err(ErroOrcamentos::QuantidadeInvalida);
        }
        let bruto = Dinheiro::de_total(quantidade, preco_unitario, Arredondamento::MeioAcima);
        let desconto = bruto.aplicar(desconto_percentual, Arredondamento::MeioAcima);
        Ok(Self {
            id: Id::novo(),
            orcamento,
            ordem,
            descricao,
            unidade: unidade.into().trim().to_owned(),
            quantidade,
            preco_unitario,
            desconto_percentual,
            total: bruto - desconto,
        })
    }

    /// O total da linha antes do desconto.
    #[must_use]
    pub fn bruto(&self) -> Dinheiro {
        Dinheiro::de_total(self.quantidade, self.preco_unitario, Arredondamento::MeioAcima)
    }

    /// O valor do desconto da linha.
    #[must_use]
    pub fn desconto_valor(&self) -> Dinheiro {
        self.bruto() - self.total
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn calcula_total_com_desconto() {
        let i = ItemOrcamento::novo(
            Id::novo(),
            0,
            "Troca de rolamento",
            Quantidade::unidades(2),
            "un",
            Preco::reais(150),
            Percentual::pontos(10),
        )
        .unwrap();
        assert_eq!(i.bruto(), Dinheiro::reais(300));
        assert_eq!(i.desconto_valor(), Dinheiro::reais(30));
        assert_eq!(i.total, Dinheiro::reais(270));
    }

    #[test]
    fn descricao_vazia_e_quantidade_zero_sao_recusadas() {
        assert_eq!(
            ItemOrcamento::novo(
                Id::novo(),
                0,
                "  ",
                Quantidade::unidades(1),
                "un",
                Preco::reais(10),
                Percentual::ZERO,
            )
            .unwrap_err(),
            ErroOrcamentos::DescricaoDeItemVazia,
        );
        assert_eq!(
            ItemOrcamento::novo(
                Id::novo(),
                0,
                "Item",
                Quantidade::unidades(0),
                "un",
                Preco::reais(10),
                Percentual::ZERO,
            )
            .unwrap_err(),
            ErroOrcamentos::QuantidadeInvalida,
        );
    }
}
