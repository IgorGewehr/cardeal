//! # mod-vendas
//!
//! O caminho comercial completo — orçamento, pedido, preço, comissão, entrega e devolução —
//! que termina sempre num lançamento balanceado no Razão. Ver `docs/modulos/vendas.md`.
//!
//! ## O que este crate contém agora
//!
//! O **domínio puro** (`docs/19-estado-e-processo.md` §3.2):
//!
//! - [`preco_vigente`] sobre [`TabelaPreco`]/[`RegraPreco`] — resolução por faixa de
//!   quantidade e promoção com vigência; a regra mais específica que cobre vence.
//! - [`Orcamento`] / [`Pedido`] / [`ItemVenda`] — as duas máquinas de estado, o total
//!   calculado a partir dos itens, o **preço congelado no item** (§11.1), a reserva só na
//!   confirmação (§11.2), **faturar irreversível** (§11.3) e o **desconto acima do teto do
//!   papel recusado na hora** (§11.4).
//! - [`Devolucao`] — total e parcial, com rateio proporcional que **não perde centavo**
//!   (§11.5, propriedade).
//! - [`Comissao`] — apuração sobre a base líquida de devolução; nunca sobre pedido
//!   cancelado ou devolvido por inteiro (§11.6).
//! - [`receituario`] — faturamento (receita + desconto + CMV), devolução e comissão viram
//!   [`LancamentoBalanceado`](cardeal_ledger::LancamentoBalanceado) (§7).
//! - [`manifesto`] — a identidade declarativa do módulo, validada.
//!
//! ## O que falta (ver `docs/17-roadmap.md`, Fase 2)
//!
//! Os **comandos** e **consultas** paginadas, que dependem de `cardeal-storage`; as portas
//! síncronas para `estoque` (reservar/consumir) e `clientes` (`LimiteDisponivel`); a
//! apuração mensal de comissão em lote; o contrato recorrente que cria a `Recorrencia` no
//! financeiro.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]
#![allow(clippy::result_large_err)]

mod comissao;
mod devolucao;
mod erros;
mod manifesto;
mod pedido;
mod preco;
pub mod receituario;

pub use comissao::{Comissao, EstadoComissao};
pub use devolucao::{Devolucao, EstadoDevolucao, ItemDevolvido, TipoDevolucao};
pub use erros::ErroVendas;
pub use manifesto::{manifesto, MANIFESTO};
pub use pedido::{EstadoOrcamento, EstadoPedido, ItemVenda, Orcamento, Pedido};
pub use preco::{preco_vigente, AlvoRegra, RegraPreco, TabelaPreco, TipoTabela};
