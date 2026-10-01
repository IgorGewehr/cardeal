//! # cardeal-cliente
//!
//! O lado do cliente: como uma UI chega ao motor. Hoje, o modo monoposto — o motor embutido no
//! próprio processo ([`MotorLocal`], de `cardeal-motor`, ADR-0009) — e o autoatualizador.
//! O transporte remoto (`MotorRemoto`, HTTP + `postcard` contra `cardeal-server`) entra aqui, com
//! a mesma superfície, pela ADR-0016.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]
#![allow(clippy::result_large_err)] // `Erro` é grande de propósito (detalhes ao usuário)

/// Verificação/download/aplicação de atualização via GitHub Releases — ver `docs/build/atualizacao.md`.
pub mod atualizador;

pub use cardeal_motor::{
    EmpresaResumo, IdentidadeVisual, MotorLocal, PapelResumo, SessaoLocal, UsuarioResumo,
};
