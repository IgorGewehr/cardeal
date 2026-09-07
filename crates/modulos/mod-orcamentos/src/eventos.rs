//! Os eventos de domínio que o módulo de orçamentos publica na outbox. Nomes versionados —
//! assinantes casam pelo nome, nunca pelo tipo Rust.

use cardeal_kernel::{Dinheiro, Id};
use cardeal_storage::EventoDominio;
use serde::Serialize;

/// O cliente aprovou um orçamento.
#[derive(Debug, Serialize)]
pub struct OrcamentoAprovado {
    /// O orçamento.
    pub orcamento: Id,
    /// O cliente, se cadastrado.
    pub cliente: Option<Id>,
    /// O total aprovado.
    pub total: Dinheiro,
}

impl EventoDominio for OrcamentoAprovado {
    const TIPO: &'static str = "orcamentos.orcamento_aprovado.v1";

    fn agregado(&self) -> Option<Id> {
        Some(self.orcamento)
    }
}

/// Um orçamento aprovado virou uma ordem de serviço.
#[derive(Debug, Serialize)]
pub struct OrcamentoConvertidoEmOs {
    /// O orçamento de origem.
    pub orcamento: Id,
    /// A ordem de serviço criada.
    pub ordem_servico: Id,
}

impl EventoDominio for OrcamentoConvertidoEmOs {
    const TIPO: &'static str = "orcamentos.orcamento_convertido.v1";

    fn agregado(&self) -> Option<Id> {
        Some(self.orcamento)
    }
}
