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
//! - [`Usuario`] — a pessoa: autenticação, troca e redefinição de senha, bloqueio,
//!   ativação. Domínio puro (o relógio e a persistência vêm de quem chama).
//! - [`Papel`]/[`PapelDeFabrica`]/[`PoliticaPapel`] — conjuntos de permissões e limites
//!   (§3.2, §3.4). Os nove papéis de fábrica são regras declarativas expandidas contra o
//!   catálogo real de permissões — sem depender de `cardeal-modkit` (que depende deste
//!   crate para o despacho).
//! - [`ValorLimite`] — o teto de um limite quantitativo (§3.4).
//! - [`Sessao`]/[`AutorizacoesEfetivas`] e a função livre [`autorizar`] — o único ponto de
//!   verificação (§3.5): concede a permissão **e** o escopo abrange o recurso.
//!
//! ## O que falta (ver `docs/17-roadmap.md`)
//!
//! A **persistência** de `Usuario`/`Papel`/`Sessao`/`Dispositivo` (as tabelas `nucleo_*` já
//! existem em `cardeal-storage`), a montagem de [`AutorizacoesEfetivas`] a partir do
//! `ConjuntoEfetivo` de `cardeal-modkit` (feita na camada que tem ambos), o handshake
//! desafio-resposta (§2.3), o PIN de operador (§2.2) e a CA interna (§4.1).

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]
#![allow(clippy::result_large_err)]

mod bloqueio;
mod erros;
mod escopo;
mod limite;
mod papel;
mod senha;
mod sessao;
mod usuario;

pub use bloqueio::Bloqueio;
pub use erros::ErroAuth;
pub use escopo::Escopo;
pub use limite::ValorLimite;
pub use papel::{Papel, PapelDeFabrica, PoliticaPapel};
pub use senha::{hash_senha, verificar_senha, HashDeSenha, PoliticaSenha};
pub use sessao::{autorizar, AutorizacoesEfetivas, EmissaoSessao, Sessao, DURACAO_PADRAO_SEGUNDOS};
pub use usuario::Usuario;
