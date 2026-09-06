//! Os eventos de domínio que a agenda publica na outbox (`docs/modulos/agenda.md` §8).
//!
//! `agenda.conflito_detectado.v1`/`agenda.lembrete_disparado.v1` estão declarados no
//! manifesto (`docs/modulos/agenda.md` §8 os já previa) mas ainda sem publicador: o primeiro
//! é do fluxo de reconciliação offline (`docs/modulos/agenda.md` §12, sem `cardeal-sync`
//! ainda) e o segundo depende do domínio `Lembrete` (ver `src/lib.rs`) — nenhum dos dois
//! existe nesta fatia.

use cardeal_kernel::{Id, Instante};
use cardeal_storage::EventoDominio;
use serde::Serialize;

/// Um compromisso foi criado.
#[derive(Debug, Serialize)]
pub struct CompromissoCriado {
    /// O compromisso.
    pub compromisso: Id,
    /// Início.
    pub inicio: Instante,
    /// Fim.
    pub fim: Instante,
    /// Os recursos reservados.
    pub recursos: Vec<Id>,
}

impl EventoDominio for CompromissoCriado {
    const TIPO: &'static str = "agenda.compromisso_criado.v1";

    fn agregado(&self) -> Option<Id> {
        Some(self.compromisso)
    }
}

/// Um compromisso foi confirmado.
#[derive(Debug, Serialize)]
pub struct CompromissoConfirmado {
    /// O compromisso.
    pub compromisso: Id,
}

impl EventoDominio for CompromissoConfirmado {
    const TIPO: &'static str = "agenda.compromisso_confirmado.v1";

    fn agregado(&self) -> Option<Id> {
        Some(self.compromisso)
    }
}

/// Um compromisso foi cancelado — nunca apagado (§11.3).
#[derive(Debug, Serialize)]
pub struct CompromissoCancelado {
    /// O compromisso.
    pub compromisso: Id,
}

impl EventoDominio for CompromissoCancelado {
    const TIPO: &'static str = "agenda.compromisso_cancelado.v1";

    fn agregado(&self) -> Option<Id> {
        Some(self.compromisso)
    }
}
