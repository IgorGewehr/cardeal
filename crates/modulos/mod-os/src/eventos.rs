//! Os eventos de domínio que o módulo de OS publica na outbox (`docs/modulos/os.md` §8).
//! Nomes versionados — assinantes casam pelo nome, nunca pelo tipo Rust.

use cardeal_kernel::{Dinheiro, Id};
use cardeal_storage::EventoDominio;
use serde::Serialize;

/// Uma ordem de serviço foi aberta.
#[derive(Debug, Serialize)]
pub struct OrdemAberta {
    /// A ordem criada.
    pub ordem_servico: Id,
    /// O cliente.
    pub cliente: Id,
}

impl EventoDominio for OrdemAberta {
    const TIPO: &'static str = "os.ordem_aberta.v1";

    fn agregado(&self) -> Option<Id> {
        Some(self.ordem_servico)
    }
}

/// O cliente aprovou o orçamento.
#[derive(Debug, Serialize)]
pub struct OrcamentoAprovado {
    /// A ordem de serviço.
    pub ordem_servico: Id,
    /// O total aprovado.
    pub valor_total: Dinheiro,
}

impl EventoDominio for OrcamentoAprovado {
    const TIPO: &'static str = "os.orcamento_aprovado.v1";

    fn agregado(&self) -> Option<Id> {
        Some(self.ordem_servico)
    }
}

/// Uma ordem de serviço foi faturada.
#[derive(Debug, Serialize)]
pub struct OrdemFaturada {
    /// A ordem de serviço.
    pub ordem_servico: Id,
    /// O cliente.
    pub cliente: Id,
    /// O total faturado.
    pub valor_total: Dinheiro,
    /// O lançamento gerado no razão, se houve algo a lançar.
    pub lancamento: Option<Id>,
    /// O título a receber gerado no financeiro, se houve cobrança.
    pub titulo: Option<Id>,
}

impl EventoDominio for OrdemFaturada {
    const TIPO: &'static str = "os.ordem_faturada.v1";

    fn agregado(&self) -> Option<Id> {
        Some(self.ordem_servico)
    }
}

/// Uma ordem de serviço foi cancelada antes de concluída.
#[derive(Debug, Serialize)]
pub struct OrdemCancelada {
    /// A ordem de serviço.
    pub ordem_servico: Id,
}

impl EventoDominio for OrdemCancelada {
    const TIPO: &'static str = "os.ordem_cancelada.v1";

    fn agregado(&self) -> Option<Id> {
        Some(self.ordem_servico)
    }
}
