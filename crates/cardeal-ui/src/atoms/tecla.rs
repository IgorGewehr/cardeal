//! Camada 1 (atoms) — `Tecla`: a "tecla física" que ensina um atalho (`F2`, `Esc`, `1`).
//!
//! `docs/12-ui-ux.md` §1.3 (teclado primeiro): um atalho que ninguém vê é um atalho que
//! ninguém usa. A `Tecla` é só a **dica visual** — quem trata a tecla de verdade é a tela
//! (`ctx.input`). Regra do projeto: só mostre a `Tecla` de um atalho que **funciona**.

use egui::{Response, Sense, Ui, Widget};

use crate::tokens::{Papel, TemaUi};

/// Uma tecla desenhada como keycap.
#[must_use]
pub struct Tecla {
    texto: String,
}

impl Tecla {
    /// Uma tecla com o texto dado (`"F2"`, `"Esc"`, `"↑↓"`).
    pub fn nova(texto: impl Into<String>) -> Self {
        Self {
            texto: texto.into(),
        }
    }
}

impl Widget for Tecla {
    fn ui(self, ui: &mut Ui) -> Response {
        let cores = ui.cores();
        let galley =
            ui.painter()
                .layout_no_wrap(self.texto, Papel::Codigo.font_id(), cores.texto_medio);
        let tamanho = egui::vec2((galley.size().x + 14.0).max(24.0), 22.0);
        let (rect, resp) = ui.allocate_exact_size(tamanho, Sense::hover());
        if ui.is_rect_visible(rect) {
            ui.painter().rect_filled(rect, 6.0, cores.superficie_2);
            ui.painter()
                .rect_stroke(rect, 6.0, egui::Stroke::new(1.0_f32, cores.borda_forte));
            ui.painter().galley(
                rect.center() - galley.size() / 2.0,
                galley,
                cores.texto_medio,
            );
        }
        resp
    }
}
