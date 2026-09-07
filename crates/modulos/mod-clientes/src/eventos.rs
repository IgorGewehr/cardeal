//! Os eventos de domínio que o módulo de clientes publica na outbox
//! (`docs/modulos/clientes.md` §8). Nomes versionados — assinantes casam pelo nome, nunca
//! pelo tipo Rust.

use cardeal_kernel::Id;
use cardeal_storage::EventoDominio;
use serde::Serialize;

/// Uma pessoa foi cadastrada.
#[derive(Debug, Serialize)]
pub struct PessoaCriada {
    /// A pessoa criada.
    pub pessoa: Id,
    /// `"Fisica"` ou `"Juridica"`.
    pub tipo: &'static str,
    /// O documento principal informado no cadastro.
    pub documento_principal: Id,
}

impl EventoDominio for PessoaCriada {
    const TIPO: &'static str = "clientes.pessoa_criada.v1";

    fn agregado(&self) -> Option<Id> {
        Some(self.pessoa)
    }
}

/// Um papel foi acrescentado a uma pessoa já cadastrada.
#[derive(Debug, Serialize)]
pub struct PapelAdicionado {
    /// A pessoa.
    pub pessoa: Id,
    /// O rótulo do papel acrescentado.
    pub papel: &'static str,
}

impl EventoDominio for PapelAdicionado {
    const TIPO: &'static str = "clientes.papel_adicionado.v1";

    fn agregado(&self) -> Option<Id> {
        Some(self.pessoa)
    }
}
