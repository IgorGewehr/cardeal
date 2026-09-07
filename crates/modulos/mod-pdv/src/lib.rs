//! # mod-pdv
//!
//! A frente de caixa: venda de balcão rápida, sobre a sessão de caixa que `financeiro` já
//! possui. Ver `docs/modulos/pdv.md` para a especificação funcional completa.
//!
//! ## O que este crate contém agora
//!
//! - [`cupom`] — `Cupom`/`ItemCupom`/`PagamentoCupom`, a máquina de estado
//!   `EmAndamento→Finalizado`/`Cancelado`, preço congelado no item, desconto por item com
//!   teto do papel, e [`cupom::validar_pagamentos`] (a soma das formas tem que fechar
//!   exatamente com o total — §11 regra 5).
//! - [`receituario`] — o lançamento combinado de finalização (um débito por forma de
//!   pagamento + desconto + CMV), mesmo desenho de `mod-vendas::faturar_pedido`.
//! - [`manifesto`] — a identidade declarativa do módulo, validada (depende de financeiro,
//!   clientes, estoque e vendas — reusa `TabelaPreco`/`RegraPreco` de lá, nunca decide preço
//!   por conta própria).
//! - **A amarração ao motor** ([`ModuloPdv`]): migrações (`pdv_faixa_numeracao`/`pdv_cupom`/
//!   `pdv_item_cupom`/`pdv_pagamento_cupom`), `RepositorioPdv`, e seis comandos: `AbrirCupom`
//!   (não estava no spec original como comando próprio — ver a nota no arquivo do comando),
//!   `AdicionarItem`, `AplicarDescontoItem`, `CancelarItem`, `CancelarCupom` e
//!   `FinalizarVenda` — consome estoque de verdade (`mod_estoque::registrar_saida_comum`) e
//!   posta o lançamento combinado via `Razao::registrar`.
//!
//! ## O que falta (ver `docs/17-roadmap.md`)
//!
//! Nesta ordem de dependência real:
//! - **`AbrirCaixaPdv`/`FecharCaixaPdv`/`RegistrarSangriaPdv`**: o spec (§5) pede comandos
//!   próprios que delegam a `financeiro`; nesta fatia o front-end chama
//!   `financeiro.abrir_caixa.v1`/`financeiro.fechar_caixa.v1`/`financeiro.registrar_sangria.v1`
//!   diretamente — são as mesmas permissões, sem lógica de PDV no meio (o único ganho real de
//!   um `AbrirCaixaPdv` próprio seria a reserva de `FaixaNumeracao` por lote, que esta versão
//!   já simplificou para numeração contínua, ver `src/repositorio.rs`).
//! - **TEF/`PortaTef`, balança/`PortaBalanca`, impressora**: portas (traits) que dependem de
//!   hardware real — `IniciarPagamentoTef`, leitura de peso e o cupom operacional impresso
//!   ficam para quando houver um adaptador de verdade para mirar.
//! - **O diário durável do carrinho** (arquivo append-only com CRC32 por terminal, §11 regra
//!   1) — cada mutação do cupom já é sua própria transação `SAVEPOINT`, então a recuperação
//!      após queda de energia funciona pela durabilidade normal do SQLite; só não na
//!      granularidade de "o último campo digitado" que o diário promete. Sem `cardeal-desktop`
//!      rodando um loop de terminal de verdade ainda, não há quem se beneficie desse ganho
//!      extra de latência.
//! - **NFC-e**: PDV nunca vai emiti-la (é `mod-fiscal`, assíncrono, `docs/10-modulo-fiscal.md`
//!   §3) — `documento_fiscal` nem é campo do `Cupom` nesta fatia.
//! - **Venda "na carteira" (a prazo)**, submódulos `pista`/`delivery`, cancelamento de cupom
//!   **depois** de finalizado (o spec permite no mesmo dia, se o fiscal ainda não autorizou) —
//!   todos deferidos, sem consumidor ainda.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]
#![allow(clippy::result_large_err)]

mod comandos;
mod cupom;
mod erros;
pub mod eventos;
mod manifesto;
pub mod migracoes;
mod modulo;
pub mod receituario;
mod repositorio;

pub use comandos::{
    AbrirCupom, AdicionarItem, AplicarDescontoItem, CancelarCupom, CancelarItem, CupomAberto,
    FinalizarVenda, ItemFoiAdicionado, PagamentoInformado, VendaFoiFinalizada,
};
pub use cupom::{
    validar_pagamentos, Cupom, EstadoCupom, FormaPagamentoPdv, ItemCupom, PagamentoCupom,
};
pub use erros::ErroPdv;
pub use manifesto::{manifesto, MANIFESTO};
pub use modulo::ModuloPdv;
pub use repositorio::RepositorioPdv;
