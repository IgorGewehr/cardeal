//! # cardeal-ui
//!
//! O design system Rubro, construído em camadas ao estilo Atomic Design —
//! `docs/12-ui-ux.md` e `docs/adr/0004-egui-como-camada-de-interface.md`.
//!
//! - [`tokens`] — Camada 0 (ions): cor, tipografia, espaçamento e a ponte para o
//!   `egui::Style` ([`instalar_estilo`](tokens::instalar_estilo) +
//!   [`TemaUi`](tokens::TemaUi)). Nunca string/hex solta; nunca `&Cores` na assinatura de um
//!   componente — o tema vem do `Ui`.
//! - [`atoms`] — Camada 1: as menores peças com aparência própria (`Botao`, `Rotulo`,
//!   `CampoTexto`, `ValorDinheiro`) e [`superficie_clicavel`](atoms::superficie_clicavel),
//!   a linha clicável de largura cheia que sidebar, lista e grade compartilham.
//! - [`molecules`] — Camada 2: composições de átomos (`Campo`, `CabecalhoTela`, `CartaoKpi`,
//!   `ItemDeLista`, `LinhaDeAcao`, `EstadoVazio`).
//! - [`organisms`] — Camada 3: peças de tela inteiras (`LayoutTela` responsivo, `Sidebar`,
//!   `Grade` com seleção de linha, `Cartao`). Paleta de comandos e gaveta chegam conforme as
//!   telas as exigirem.
//!
//! Galeria visual: `cargo run -p cardeal-ui --example galeria`.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]

pub mod atoms;
pub mod molecules;
pub mod organisms;
pub mod tokens;
