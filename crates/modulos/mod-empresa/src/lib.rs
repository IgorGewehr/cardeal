//! # mod-empresa
//!
//! A própria empresa como módulo de despacho: dados cadastrais, contato, logo, identidade
//! visual (o que vai no cabeçalho de todo PDF), usuários e papéis.
//!
//! As tabelas são do núcleo (`nucleo_empresa`, `nucleo_configuracao`, `nucleo_usuario`,
//! `nucleo_papel`) — este módulo não tem migração própria. Existe para que essas operações
//! passem **pelo despacho**, com permissão, como qualquer outra: é o que permite o cliente
//! remoto (desktop ligado ao servidor, navegador — ADR-0016) fazê-las pela rede, com o mesmo
//! código que o monoposto usa.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]

mod comandos;
mod consultas;
mod manifesto;
mod membros;
#[cfg(feature = "sqlite")]
mod modulo;
#[cfg(feature = "sqlite")]
pub mod sql;
mod tipos;

pub use cardeal_auth::PapelDeFabrica;
pub use comandos::{AtualizarDadosEmpresa, DefinirContatoEmpresa, DefinirLogoEmpresa};
pub use consultas::{DadosDaEmpresa, IdentidadeDaEmpresa, PapeisDaEmpresa, UsuariosDaEmpresa};
pub use manifesto::{ID, MANIFESTO};
pub use membros::{AdicionarUsuario, DesativarUsuario};
#[cfg(feature = "sqlite")]
pub use modulo::ModuloEmpresa;
pub use tipos::{EmpresaResumo, IdentidadeVisual, PapelResumo, UsuarioResumo, TETO_LOGO_BYTES};
