//! Camada 1 (atoms) — `Divisor`: a linha fina que separa blocos de conteúdo.
//!
//! Substitui o `ui.separator()` cru das telas (ADR-0015): a cor vem do tema e a espessura é
//! sempre 1px, em vez da aparência de fábrica do `egui`. Como o `separator`, **segue o layout**:
//! numa coluna é uma linha horizontal de largura cheia; dentro de uma linha (`ui.horizontal`) é
//! uma linha vertical da altura de um controle. Não reserva espaço em volta — quem chama decide
//! o respiro com `Espaco`.

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
        // Dentro de uma linha o divisor é vertical; senão esticaria a linha inteira.
        let tamanho = if ui.layout().main_dir().is_horizontal() {
            egui::vec2(1.0, ui.spacing().interact_size.y)
        } else {
            egui::vec2(ui.available_width().max(0.0), 1.0)
        };
        let (rect, resp) = ui.allocate_exact_size(tamanho, Sense::hover());
        if ui.is_rect_visible(rect) {
            ui.painter().rect_filled(rect, 0.0, cor);
        }
        resp
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn tamanho(horizontal: bool) -> egui::Vec2 {
        let ctx = egui::Context::default();
        let entrada = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 300.0),
            )),
            ..Default::default()
        };
        let mut t = egui::Vec2::ZERO;
        let _ = ctx.run(entrada, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                if horizontal {
                    ui.horizontal(|ui| t = ui.add(Divisor::novo()).rect.size());
                } else {
                    t = ui.add(Divisor::novo()).rect.size();
                }
            });
        });
        t
    }

    #[test]
    fn na_coluna_e_horizontal_de_largura_cheia() {
        let t = tamanho(false);
        assert!(t.x > 100.0 && (t.y - 1.0).abs() < f32::EPSILON, "{t:?}");
    }

    #[test]
    fn na_linha_e_vertical_e_nao_estica_a_linha() {
        let t = tamanho(true);
        assert!((t.x - 1.0).abs() < f32::EPSILON && t.y > 10.0, "{t:?}");
    }
}
