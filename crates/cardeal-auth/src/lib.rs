//! # cardeal-auth
//!
//! Autenticação e autorização. Ver `docs/08-seguranca-permissoes.md` para o modelo de
//! ameaças completo — resumo: as ameaças reais de um ERP de PME brasileira são internas e
//! cotidianas (desconto indevido, senha compartilhada, dado apagado para esconder desvio),
//! não invasão externa. O design reflete isso.
//!
//! ## O que este crate contém agora
//!
//! Domínio puro, sem banco nem rede:
//!
//! - [`hash_senha`]/[`verificar_senha`] — Argon2id de verdade, com os parâmetros de
//!   `docs/08-seguranca-permissoes.md` §2.1.
//! - [`PoliticaSenha`] — comprimento mínimo e recusa de senha comum.
//! - [`Bloqueio`] — o bloqueio progressivo por tentativa malsucedida (§2.1), que também
//!   serve ao PIN de operador do PDV (§2.2) — é o mesmo mecanismo.
//! - [`Escopo`] — a parte ABAC da autorização (§3.3): "gerente da Filial 2 não vê o caixa
//!   da Filial 1", modelada como uma regra só (`Escopo::abrange`).
//!
//! ## O que falta (ver `docs/17-roadmap.md`)
//!
//! `Usuario`, `Sessao`, `Dispositivo` persistidos, a função `autorizar(sessao, permissao,
//! escopo)` que os une a [`Escopo`] e ao catálogo de permissões de `cardeal-modkit`, e a CA
//! interna (`docs/08-seguranca-permissoes.md` §4.1) — todos dependem de `cardeal-storage`.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]
#![allow(clippy::result_large_err)]

mod bloqueio;
mod erros;
mod escopo;
mod senha;

pub use bloqueio::Bloqueio;
pub use erros::ErroAuth;
pub use escopo::Escopo;
pub use senha::{hash_senha, verificar_senha, HashDeSenha, PoliticaSenha};
