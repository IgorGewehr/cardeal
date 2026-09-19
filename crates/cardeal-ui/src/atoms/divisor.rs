//! Camada 1 (atoms) — `Divisor`: a linha fina que separa blocos de conteúdo.
//!
//! Substitui o `ui.separator()` cru das telas (ADR-0015): a cor vem do tema e a espessura é
//! sempre 1px, em vez da aparência de fábrica do `egui`. Não reserva espaço em volta — quem
//! chama decide o respiro com `Espaco`.

use egui::{Response, Sense, Ui, Widget};

use crate::tokens::TemaUi;

/// Uma linha horizontal de largura cheia.
#[must_use]
pub struct Divisor {
    forte: bool,
}

impl Divisor {
    /// Um divisor sutil (`borda`) — o padrão entre seções de um mesmo bloco.
    pub const fn novo() -> Self {
        Self { forte: false }
    }

    /// Um divisor mais marcado (`borda_forte`) — entre blocos independentes.
    pub const fn forte(mut self) -> Self {
        self.forte = true;
        self
    }
}

impl Default for Divisor {
    fn default() -> Self {
        Self::novo()
    }
}

impl Widget for Divisor {
    fn ui(self, ui: &mut Ui) -> Response {
        let cores = ui.cores();
        let cor = if self.forte {
            cores.borda_forte
        } else {
            cores.borda
        };
        let (rect, resp) = ui.allocate_exact_size(
            egui::vec2(ui.available_width().max(0.0), 1.0),
            Sense::hover(),
        );
        if ui.is_rect_visible(rect) {
            ui.painter().rect_filled(rect, 0.0, cor);
        }
        resp
    }
}
