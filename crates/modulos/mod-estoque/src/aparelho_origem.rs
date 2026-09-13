//! O aparelho usado de onde uma ou mais peças foram retiradas (trade-in) — o outro lado do
//! caso de uso do post-it: quando a peça não veio de compra nova, o técnico desmontou um
//! aparelho usado, e o custo desse aparelho (rateado ou não entre as peças retiradas dele) é
//! parte do que a consulta pelo código do [`crate::produto::Lote`] precisa devolver.
//!
//! `docs/modulos/estoque.md` §3 (nasce do pedido de negócio: "se veio de um aparelho usado,
//! o custo desse aparelho usado"). Um `AparelhoOrigem` pode originar vários [`crate::produto::Lote`]
//! (várias peças tiradas do mesmo aparelho) — por isso é uma entidade própria, não um campo
//! solto no lote.

use cardeal_kernel::{Data, Dinheiro, Id};
use serde::{Deserialize, Serialize};

use crate::erros::ErroEstoque;

/// Um aparelho usado adquirido para desmontar e reaproveitar peças.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AparelhoOrigem {
    /// Identidade.
    pub id: Id,
    /// A empresa.
    pub empresa: Id,
    /// Descrição livre ("iPhone 11 Pro — tela trincada, comprado para peças").
    pub descricao: String,
    /// Identificador livre do aparelho (IMEI/serial), quando anotado.
    pub identificador: Option<String>,
    /// Quanto custou adquirir o aparelho inteiro — o custo que se reparte,
    /// implícita ou explicitamente, entre as peças retiradas dele.
    pub custo_aquisicao: Dinheiro,
    /// Quando foi adquirido.
    pub adquirido_em: Data,
    /// Fornecedor/origem da aquisição, quando houver (pode ser um cliente que vendeu o
    /// aparelho usado, não necessariamente um fornecedor cadastrado).
    pub fornecedor: Option<Id>,
    /// Observações livres.
    pub observacoes: Option<String>,
}

impl AparelhoOrigem {
    /// Registra um aparelho de origem novo.
    ///
    /// # Errors
    /// [`ErroEstoque::DescricaoDoAparelhoVazia`] se `descricao` vier vazia.
    pub fn novo(
        empresa: Id,
        descricao: impl Into<String>,
        identificador: Option<String>,
        custo_aquisicao: Dinheiro,
        adquirido_em: Data,
        fornecedor: Option<Id>,
        observacoes: Option<String>,
    ) -> Result<Self, ErroEstoque> {
        let descricao = descricao.into().trim().to_string();
        if descricao.is_empty() {
            return Err(ErroEstoque::DescricaoDoAparelhoVazia);
        }
        let identificador = identificador
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let observacoes = observacoes
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        Ok(Self {
            id: Id::novo(),
            empresa,
            descricao,
            identificador,
            custo_aquisicao,
            adquirido_em,
            fornecedor,
            observacoes,
        })
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn descricao_vazia_e_recusada() {
        assert_eq!(
            AparelhoOrigem::novo(
                Id::novo(),
                "   ",
                None,
                Dinheiro::reais(300),
                Data::de_dias(20_000),
                None,
                None,
            )
            .unwrap_err(),
            ErroEstoque::DescricaoDoAparelhoVazia
        );
    }

    #[test]
    fn normaliza_identificador_e_observacoes_vazios() {
        let a = AparelhoOrigem::novo(
            Id::novo(),
            "iPhone 11 Pro - tela trincada",
            Some("  ".to_string()),
            Dinheiro::reais(300),
            Data::de_dias(20_000),
            None,
            Some(String::new()),
        )
        .unwrap();
        assert_eq!(a.identificador, None);
        assert_eq!(a.observacoes, None);
    }
}
