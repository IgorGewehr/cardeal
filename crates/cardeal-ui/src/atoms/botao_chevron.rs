//! Camada 1 (atoms) — `BotaoChevron`: a seta `‹` / `›` que recolhe e expande um painel (a
//! sidebar). Desenhada com traços, sem depender de glifo.

use egui::{Color32, CursorIcon, Response, Sense, Stroke, Ui, Widget};

use crate::tokens::{Raio, TemaUi};

/// Um botão-chevron.
#[must_use]
pub struct BotaoChevron {
    aponta_esquerda: bool,
    cor: Option<Color32>,
}

impl BotaoChevron {
    /// `aponta_esquerda` = o painel está expandido (a seta convida a recolher).
    pub const fn novo(aponta_esquerda: bool) -> Self {
        Self {
            aponta_esquerda,
            cor: None,
        }
    }

    /// Sobrescreve a cor do traço (o padrão é `texto_medio`).
    pub const fn cor(mut self, cor: Color32) -> Self {
        self.cor = Some(cor);
        self
    }
}

impl Widget for BotaoChevron {
    fn ui(self, ui: &mut Ui) -> Response {
        let cor = self.cor.unwrap_or_else(|| ui.cores().texto_medio);
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), Sense::click());
        if ui.is_rect_visible(rect) {
            if resp.hovered() {
                ui.painter().rect_filled(
                    rect,
                    Raio::CAMPO * 0.75,
                    ui.style().visuals.widgets.hovered.bg_fill,
                );
            }
            let c = rect.center();
            let (dx, dy) = (3.5_f32, 5.0_f32);
            let (perto, longe) = if self.aponta_esquerda {
                (c.x + dx / 2.0, c.x - dx / 2.0)
            } else {
                (c.x - dx / 2.0, c.x + dx / 2.0)
            };
            let traco = Stroke::new(2.0_f32, cor);
            ui.painter()
                .line_segment([egui::pos2(perto, c.y - dy), egui::pos2(longe, c.y)], traco);
            ui.painter()
                .line_segment([egui::pos2(longe, c.y), egui::pos2(perto, c.y + dy)], traco);
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        resp
    }
}
