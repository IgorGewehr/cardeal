//! Camada 0 (ions): os tokens do design system Rubro — cor, tipografia, espaçamento e a
//! ponte para o `egui::Style`. `docs/12-ui-ux.md` §2–4 e §10. Nada de string/hex solta fora
//! daqui.

mod cores;
mod espacamento;
mod estilo;
mod tipografia;

pub use cores::{Cores, Rubro, Tema};
pub use espacamento::{AlturaLinha, Elevacao, Espaco, Raio};
pub use estilo::{instalar_estilo, sombra_cartao, TemaUi};
pub use tipografia::{instalar_fontes, Papel};
