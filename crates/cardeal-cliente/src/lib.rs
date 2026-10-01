//! # cardeal-cliente
//!
//! Como uma interface chega ao motor. As telas usam só [`Motor`] e [`Sessao`]:
//!
//! - **Local** (feature `local`, padrão no desktop): o motor embutido no próprio processo
//!   ([`cardeal_motor::MotorLocal`], ADR-0009).
//! - **Remoto**: um `cardeal-server` pela rede (ADR-0016) — [`remoto::Remoto`] sobre um
//!   [`remoto::Transporte`]; o protocolo em si ([`remoto::protocolo`]) não faz I/O e serve
//!   também ao cliente do navegador.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]
#![allow(clippy::result_large_err)] // `Erro` é grande de propósito (detalhes ao usuário)

/// Verificação/download/aplicação de atualização via GitHub Releases — ver `docs/build/atualizacao.md`.
#[cfg(not(target_arch = "wasm32"))]
pub mod atualizador;
mod motor;
pub mod remoto;

#[cfg(feature = "local")]
pub use cardeal_motor::{MotorLocal, SessaoLocal};
pub use mod_empresa::{EmpresaResumo, IdentidadeVisual, PapelResumo, UsuarioResumo};
pub use motor::{Motor, Sessao};
