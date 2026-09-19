//! Camada 1 (atoms) — `BotaoJanela`: minimizar, maximizar/restaurar e fechar da barra de título
//! desenhada à mão (o Cardeal não usa a decoração nativa da janela).
//!
//! Os ícones são traços desenhados, não glifos: não dependem de fonte alguma. Só o **fechar**
//! fica vermelho ao passar o mouse (convenção do sistema); os outros usam o realce neutro.

use egui::{Color32, CursorIcon, Painter, Rect, Response, Sense, Stroke, Ui, Widget};

use crate::tokens::Rubro;

/// Largura de um botão de janela.
const LARGURA: f32 = 44.0;
/// Espessura do traço dos ícones.
const TRACO: f32 = 1.3;

/// Qual das ações da janela o botão representa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoBotaoJanela {
    /// Minimizar.
    Minimizar,
    /// Maximizar (janela em tamanho normal).
    Maximizar,
    /// Restaurar (janela maximizada): dois quadrados sobrepostos.
    Restaurar,
    /// Fechar.
    Fechar,
}

/// Um botão de janela.
#[must_use]
pub struct BotaoJanela {
    tipo: TipoBotaoJanela,
    altura: f32,
}

impl BotaoJanela {
    /// Um botão do tipo dado, com a altura de uma barra de título de 40px.
    pub const fn novo(tipo: TipoBotaoJanela) -> Self {
        Self { tipo, altura: 40.0 }
    }

    /// A altura do botão — a da barra de título que o contém.
    pub const fn altura(mut self, px: f32) -> Self {
        self.altura = px;
        self
    }
}

fn desenhar(pintor: &Painter, area: Rect, cor: Color32, tipo: TipoBotaoJanela) {
    let centro = area.center();
    let traco = Stroke::new(TRACO, cor);
    match tipo {
        TipoBotaoJanela::Fechar => {
            let d = 5.0_f32;
            pintor.line_segment(
                [centro + egui::vec2(-d, -d), centro + egui::vec2(d, d)],
                traco,
            );
            pintor.line_segment(
                [centro + egui::vec2(-d, d), centro + egui::vec2(d, -d)],
                traco,
            );
        }
        TipoBotaoJanela::Maximizar => {
            pintor.rect_stroke(
                Rect::from_center_size(centro, egui::vec2(11.0, 11.0)),
                0.0,
                traco,
            );
        }
        TipoBotaoJanela::Restaurar => {
            for deslocamento in [egui::vec2(-2.0, 2.0), egui::vec2(2.0, -2.0)] {
                pintor.rect_stroke(
                    Rect::from_center_size(centro + deslocamento, egui::vec2(9.0, 9.0)),
                    0.0,
                    traco,
                );
            }
        }
        TipoBotaoJanela::Minimizar => {
            pintor.line_segment(
                [
                    centro + egui::vec2(-5.0, 5.0),
                    centro + egui::vec2(5.0, 5.0),
                ],
                traco,
            );
        }
    }
}

impl Widget for BotaoJanela {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(LARGURA, self.altura), Sense::click());
        if ui.is_rect_visible(rect) {
            let fechar = self.tipo == TipoBotaoJanela::Fechar;
            if resp.hovered() {
                let fundo = if fechar {
                    Rubro::R600
                } else {
                    ui.style().visuals.widgets.hovered.bg_fill
                };
                ui.painter().rect_filled(rect, 0.0, fundo);
            }
            let cor = if resp.hovered() && fechar {
                Rubro::CONTRASTE
            } else {
                ui.style().visuals.text_color()
            };
            desenhar(ui.painter(), rect, cor, self.tipo);
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        resp
    }
}
