//! # cardeal-storage
//!
//! A camada de persistência: SQLite com **escritor único e group commit**. Ver
//! `docs/07-persistencia-sqlite.md` e [ADR-0012](../adr/0012-escritor-unico-com-group-commit.md).
//!
//! O modelo mental cabe em uma frase: **escritas são uma fila, leituras são livres**.
//!
//! ## O que este crate contém agora
//!
//! - [`Armazenamento`] — abre a base, aplica as migrações do núcleo, sobe o escritor e o
//!   pool de leitura.
//! - [`Escritor`] — a thread única. [`Escritor::executar`] enfileira uma
//!   [`UnidadeDeTrabalho`], participa do group commit (`SAVEPOINT` por tarefa, um `COMMIT`
//!   por lote) e devolve **após o `fsync`**. Uma tarefa que falha ou entra em pânico é
//!   isolada — as demais do lote seguem.
//! - [`UnidadeDeTrabalho`] — `conexao()`, `publicar()` (evento → `nucleo_outbox` na mesma
//!   transação), `auditar()` (cadeia de hash BLAKE3), `proximo_numero()`/`reservar_faixa()`
//!   (`nucleo_sequencia`), `travar()` (trava pessimista com TTL).
//! - [`Leitor`] — pool de conexões `query_only` sobre o snapshot do WAL.
//! - [`Migracao`]/[`ConjuntoMigracoes`] — o executor com ordenação topológica e verificação
//!   de hash (migração publicada é imutável).
//! - [`Segredo`] — valor sensível que não vaza em `Debug` e zera no `Drop`.
//!
//! ## O que falta (ver `docs/17-roadmap.md`, Fase 0)
//!
//! O `Outbox` (leitura das pendências para o publicador de eventos), o `Backup` online
//! (`sqlite3_backup` + verificação automática), o `verificar()` (`integrity_check` + prova
//! do razão), o checkpoint `TRUNCATE` na ociosidade, a feature `cripto` (`SQLCipher`) e o
//! timeout por tarefa. `cardeal-ledger` ainda precisa implementar `PortaRazao` sobre a
//! [`UnidadeDeTrabalho`] (hoje o adaptador SQLite do Razão não existe).

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]
#![allow(clippy::result_large_err)]

mod armazenamento;
mod conexao;
mod config;
mod contexto;
mod erros;
mod escritor;
mod evento;
mod leitor;
mod migracao;
mod nucleo;
mod segredo;
mod sql;
mod uow;

pub use armazenamento::Armazenamento;
pub use config::ConfigArmazenamento;
pub use contexto::{Confirmado, ContextoEscrita};
pub use erros::ErroArmazenamento;
pub use escritor::{Escritor, MetricasEscritor};
pub use evento::{EventoDominio, RegistroAuditoria};
pub use leitor::Leitor;
pub use migracao::{ConjuntoMigracoes, Migracao, RelatorioMigracao, TipoMigracao};
pub use nucleo::conjunto as conjunto_nucleo;
pub use segredo::Segredo;
pub use uow::{FaixaNumeracao, Trava, UnidadeDeTrabalho};

#[cfg(test)]
mod testes;
