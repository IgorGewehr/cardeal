//! # mod-estoque
//!
//! A verdade física do que existe, onde existe e quanto custou. Ver
//! `docs/modulos/estoque.md` para a especificação funcional completa.
//!
//! ## O que este crate contém agora
//!
//! O **domínio puro** (`docs/19-estado-e-processo.md` §3.2):
//!
//! - [`Produto`] / [`Variacao`] / [`validar_gtin`] / [`Unidade`] / [`Conversao`] / [`Lote`]
//!   — cadastro, validação de NCM e de GTIN (dígito verificador mod-10) sem I/O, e a regra
//!   "validade exige lote" (§11.7).
//! - [`SaldoLocal`] / [`custo_medio_movel`] — o **custo médio ponderado móvel** recalculado
//!   só na entrada (§11.1), o saldo que **pode ficar negativo** e é sinalizado, não
//!   bloqueado (§11.2), e a reserva que **não é saída** (§11.3).
//! - [`Inventario`] — máquina de estado com **contagem cega** (§11.5) e geração de ajustes.
//! - [`receituario`] — os lançamentos que o próprio estoque posta (ajuste, perda,
//!   transferência, entrada avulsa — §7); o CMV da venda é lançado pelo módulo que consome.
//! - [`manifesto`] — a identidade declarativa do módulo, validada.
//!
//! ## O que falta (ver `docs/17-roadmap.md`, Fase 2)
//!
//! Os **comandos** e **consultas** paginadas, que dependem de `cardeal-storage` (e da
//! trava pessimista de inventário — `ErroEstoque::LocalCongelado` já está modelado); a
//! **curva ABC**, a `PortaCatalogo` que `vendas`/`pdv` usam para reservar e consumir, e o
//! perfil tributário.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]
#![allow(clippy::result_large_err)]

mod erros;
mod inventario;
mod manifesto;
mod produto;
pub mod receituario;
mod saldo;

pub use erros::ErroEstoque;
pub use inventario::{AjusteInventario, ContagemItem, EstadoInventario, Inventario};
pub use manifesto::{manifesto, MANIFESTO};
pub use produto::{validar_gtin, Conversao, EstadoLote, Lote, Produto, Unidade, Variacao};
pub use saldo::{custo_medio_movel, Movimento, SaidaAplicada, SaldoLocal, TipoMovimento};
