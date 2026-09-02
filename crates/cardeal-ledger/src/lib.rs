//! # cardeal-ledger
//!
//! O Razão: plano de contas e lançamentos de partidas dobradas. Ver
//! `docs/05-nucleo-financeiro.md` para a tese completa — resumo aqui:
//!
//! > Toda operação de negócio, no instante em que acontece e na mesma transação, vira
//! > partida dobrada no Razão. Não existe "integrar com o financeiro": tudo já nasce ali.
//!
//! ## O que este crate contém agora
//!
//! O **domínio puro**: [`Conta`], [`Lancamento`], [`Partida`] e, principalmente,
//! [`ConstrutorLancamento`] — o único caminho para produzir um [`LancamentoBalanceado`],
//! cujo campo privado torna um lançamento desbalanceado impossível de existir.
//!
//! [`Razao`] (registrar, estornar, confirmar, liquidar) e [`Contas`] (papel → `Id`) já
//! existem e são genéricos sobre [`PortaRazao`] — uma trait mínima que substitui, por
//! enquanto, a `UnidadeDeTrabalho` real de `cardeal-storage` (que ainda não existe). Nada
//! aqui toca banco de dados; tudo é testável com `cargo test -p cardeal-ledger` sem SQLite.
//! Ver `porta` para a justificativa completa dessa escolha.
//!
//! ## O que falta (ver `docs/17-roadmap.md`)
//!
//! O adaptador de persistência real (`repositorio.rs`, atrás da feature `sqlite`, que
//! implementará [`PortaRazao`] sobre a `UnidadeDeTrabalho` de `cardeal-storage`),
//! `fechar_periodo`/`reabrir_periodo` (exigem somar saldos históricos) e as consultas
//! (saldo, fluxo de caixa, DRE, prova do razão) — todas dependem de uma consulta real
//! sobre o histórico, que só faz sentido junto do backend SQL.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]
// Ver a mesma justificativa em cardeal-kernel: Erro carrega Detalhes de propósito.
#![allow(clippy::result_large_err)]

mod construtor;
mod conta;
mod contas;
mod erros;
mod lancamento;
mod plano;
mod porta;
mod razao;
#[cfg(test)]
mod testkit_interno;

pub use construtor::{ConstrutorLancamento, LancamentoBalanceado};
pub use conta::{CodigoConta, Conta, GrupoFluxo, Natureza, PapelConta, TipoConta};
pub use contas::Contas;
pub use erros::ErroRazao;
pub use lancamento::{Contraparte, EstadoLancamento, Lancamento, Origem, Partida};
pub use plano::{plano_padrao, ContaSemente};
pub use porta::{InfoConta, PortaRazao};
pub use razao::Razao;
