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
//! [`Razao`] (registrar, estornar, confirmar, liquidar) e [`Contas`] (papel → `Id`) são
//! genéricos sobre [`PortaRazao`]. Há **duas** implementações: o double em memória dos
//! testes, e [`RepositorioRazao`] — o adaptador SQLite real sobre a
//! [`UnidadeDeTrabalho`](cardeal_storage::UnidadeDeTrabalho). As tabelas `razao_*` vêm de
//! [`migracoes::conjunto`].
//!
//! ## O que falta (ver `docs/17-roadmap.md`)
//!
//! `fechar_periodo`/`reabrir_periodo` (exigem somar saldos históricos) e as consultas mais
//! amplas (fluxo de caixa, DRE, prova do razão) — todas dependem de uma agregação genérica
//! sobre `razao_partida` que ainda não existe. O caso mínimo já está resolvido:
//! [`RepositorioRazao::saldo_realizado`] soma as partidas `Realizado` de uma conta — é o que
//! `mod-financeiro` usa para o saldo do caixa físico na sangria e no fechamento cego.

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
pub mod migracoes;
mod plano;
mod porta;
mod razao;
mod repositorio;
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
pub use repositorio::{semear_plano_padrao, RepositorioRazao};
