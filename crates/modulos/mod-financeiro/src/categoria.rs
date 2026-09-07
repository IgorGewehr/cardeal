//! Categoria financeira — um rótulo livre para agrupar títulos em relatórios/gráficos,
//! **fora** do plano de contas (`docs/modulos/financeiro.md` §11.9): "Aluguel", "Internet",
//! "Assinatura `SaaS` — Cliente X". Nunca participa da contabilização — a conta de resultado
//! continua resolvida por `PapelConta`, sempre. Existe só para alimentar agregações por
//! categoria e mês: quanto custa cada tipo de despesa recorrente, quanto cada projeto/cliente
//! de `SaaS` rende por mês, histórico para acompanhar churn. Domínio puro.

use cardeal_kernel::Id;
use serde::{Deserialize, Serialize};

use crate::erros::ErroFinanceiro;
use crate::titulo::EspecieTitulo;

/// Um rótulo de categoria para agrupar títulos em relatórios.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoriaFinanceira {
    /// Identidade.
    pub id: Id,
    /// A empresa.
    pub empresa: Id,
    /// O nome livre: "Aluguel", "Assinatura `SaaS` — Cliente X".
    pub nome: String,
    /// Restringe a categoria a uma espécie (custo ou receita); `None` serve para as duas.
    pub especie: Option<EspecieTitulo>,
    /// Categorias inativas somem das listas de seleção, mas os títulos antigos que as usam
    /// continuam com o vínculo — histórico nunca é reescrito.
    pub ativa: bool,
}

impl CategoriaFinanceira {
    /// Cria uma categoria nova, ativa.
    ///
    /// # Errors
    /// [`ErroFinanceiro::NomeDeCategoriaVazio`] se o nome, sem espaços nas pontas, for vazio.
    pub fn nova(
        empresa: Id,
        nome: impl Into<String>,
        especie: Option<EspecieTitulo>,
    ) -> Result<Self, ErroFinanceiro> {
        let nome = nome.into();
        if nome.trim().is_empty() {
            return Err(ErroFinanceiro::NomeDeCategoriaVazio);
        }
        Ok(Self {
            id: Id::novo(),
            empresa,
            nome,
            especie,
            ativa: true,
        })
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn nome_vazio_e_recusado() {
        assert_eq!(
            CategoriaFinanceira::nova(Id::novo(), "   ", None).unwrap_err(),
            ErroFinanceiro::NomeDeCategoriaVazio
        );
    }

    #[test]
    fn nasce_ativa() {
        let c =
            CategoriaFinanceira::nova(Id::novo(), "Aluguel", Some(EspecieTitulo::Pagar)).unwrap();
        assert!(c.ativa);
        assert_eq!(c.especie, Some(EspecieTitulo::Pagar));
    }
}
