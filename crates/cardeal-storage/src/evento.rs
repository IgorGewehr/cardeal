//! Eventos de domínio e registro de auditoria — o que a unidade de trabalho grava, além
//! das linhas de negócio.

use cardeal_kernel::Id;
use serde::Serialize;

/// Um evento de domínio publicável. Vai para `nucleo_outbox` **na mesma transação** do
/// fato — nenhum evento se perde nem é publicado sem o fato ter ocorrido
/// (`docs/07-persistencia-sqlite.md` §3).
pub trait EventoDominio: Serialize {
    /// O nome versionado do evento, ex.: `"vendas.pedido_faturado.v1"`.
    const TIPO: &'static str;

    /// O agregado a que o evento se refere, quando aplicável.
    fn agregado(&self) -> Option<Id> {
        None
    }
}

/// Uma entrada da auditoria encadeada por hash (`docs/06-modelo-de-dados.md` §2.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistroAuditoria {
    /// A ação, ex.: `"financeiro.pagar.baixar"`.
    pub acao: String,
    /// A entidade afetada, ex.: `"financeiro_parcela"`.
    pub entidade: String,
    /// O id da entidade, quando houver.
    pub entidade_id: Option<Id>,
    /// O estado anterior (só os campos alterados), como JSON.
    pub antes: Option<serde_json::Value>,
    /// O estado posterior, como JSON.
    pub depois: Option<serde_json::Value>,
}

impl RegistroAuditoria {
    /// Um registro de auditoria de uma ação sobre uma entidade.
    #[must_use]
    pub fn nova(acao: impl Into<String>, entidade: impl Into<String>, entidade_id: Id) -> Self {
        Self {
            acao: acao.into(),
            entidade: entidade.into(),
            entidade_id: Some(entidade_id),
            antes: None,
            depois: None,
        }
    }

    /// Anexa o antes/depois em JSON.
    #[must_use]
    pub fn com_diff(
        mut self,
        antes: Option<serde_json::Value>,
        depois: Option<serde_json::Value>,
    ) -> Self {
        self.antes = antes;
        self.depois = depois;
        self
    }
}
