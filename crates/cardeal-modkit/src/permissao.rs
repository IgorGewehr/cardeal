//! Permissões: o catálogo declarativo que cada módulo contribui.
//!
//! Ver `docs/08-seguranca-permissoes.md` §3.1. Nenhuma permissão é uma string solta digitada
//! num `match` em algum lugar do código — todas nascem aqui, no [`Manifesto`](crate::Manifesto)
//! do módulo, com descrição em português e nível de risco.

use std::fmt;

/// O quão sensível é uma ação, para orientar a interface (confirmação extra, autorização
/// de supervisor) e a auditoria (`docs/08-seguranca-permissoes.md` §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Risco {
    /// Consulta, navegação — não muda estado.
    Baixo,
    /// Operação rotineira que altera dado (criar, editar um cadastro comum).
    Medio,
    /// Ação com efeito financeiro ou irreversível relevante (baixar pagamento, cancelar
    /// venda). Normalmente auditada.
    Alto,
    /// Ação que reescreve o passado ou contorna um controle (estornar, reabrir período,
    /// exportação em massa). Sempre auditada e, em geral, notifica o administrador.
    Critico,
}

impl fmt::Display for Risco {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Baixo => "Baixo",
            Self::Medio => "Médio",
            Self::Alto => "Alto",
            Self::Critico => "Crítico",
        })
    }
}

/// Uma permissão declarada por um módulo.
///
/// A chave segue `docs/15-convencoes-codigo.md` §6: `"<modulo>.<recurso>.<acao>"`, ex.:
/// `"financeiro.receber.baixar"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Permissao {
    /// A chave estável, no formato `modulo.recurso.acao`.
    pub chave: &'static str,
    /// A descrição em português que aparece no cadastro de papéis — nunca a chave crua.
    pub descricao: &'static str,
    /// O nível de risco.
    pub risco: Risco,
    /// Se definido, esta permissão só faz sentido (e só aparece na tela de papéis) quando
    /// o submódulo indicado está ativo.
    pub requer_submodulo: Option<&'static str>,
}

impl Permissao {
    /// Declara uma permissão de risco [`Risco::Baixo`] ou [`Risco::Medio`], sem depender de
    /// submódulo. Para os demais casos, construa a struct diretamente com sintaxe de campo.
    #[must_use]
    pub const fn nova(chave: &'static str, descricao: &'static str, risco: Risco) -> Self {
        Self {
            chave,
            descricao,
            risco,
            requer_submodulo: None,
        }
    }

    /// O id do módulo dono, extraído da chave (a parte antes do primeiro ponto).
    ///
    /// ```
    /// # use cardeal_modkit::{Permissao, Risco};
    /// let p = Permissao::nova("financeiro.receber.baixar", "Dar baixa em recebimento", Risco::Medio);
    /// assert_eq!(p.modulo(), "financeiro");
    /// ```
    #[must_use]
    pub fn modulo(&self) -> &'static str {
        self.chave.split('.').next().unwrap_or(self.chave)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn extrai_o_modulo_da_chave() {
        let p = Permissao::nova(
            "estoque.inventario.confirmar",
            "Confirmar inventário",
            Risco::Alto,
        );
        assert_eq!(p.modulo(), "estoque");
    }

    #[test]
    fn risco_e_ordenavel() {
        assert!(Risco::Baixo < Risco::Medio);
        assert!(Risco::Medio < Risco::Alto);
        assert!(Risco::Alto < Risco::Critico);
    }
}
