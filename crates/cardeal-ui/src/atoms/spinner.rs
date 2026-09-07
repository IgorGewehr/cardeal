//! Camada 1 (atoms) — `Spinner`: indicador de carregamento. `docs/12-ui-ux.md` §7.
//!
//! Três pontos que pulsam em sequência (referência: gestao-raiz — spinner de 3 bolinhas).
//! É um `Widget`: `ui.add(Spinner::novo())`. Pede repaint contínuo enquanto visível.

// Módulo de desenho: fase de animação e índices de laço pequenos viram `f32`.
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

use egui::{Color32, Response, Sense, Ui, Vec2, Widget};

use crate::tokens::TemaUi;

/// Um indicador de carregamento — três pontos pulsando.
#[must_use]
pub struct Spinner {
    tamanho: f32,
    cor: Option<Color32>,
}

impl Default for Spinner {
    fn default() -> Self {
        Self::novo()
    }
}

impl Spinner {
    /// Spinner padrão (~18px de lado), na cor da marca.
    pub const fn novo() -> Self {
        Self {
            tamanho: 18.0,
            cor: None,
        }
    }

    /// Versão compacta (~13px) — para dentro de um botão ou linha densa.
    pub const fn pequeno(mut self) -> Self {
        self.tamanho = 13.0;
        self
    }

    /// Sobrescreve a cor dos pontos (ex.: `Rubro::CONTRASTE` dentro de um botão primário).
    pub const fn cor(mut self, cor: Color32) -> Self {
        self.cor = Some(cor);
        self
    }
}

impl Widget for Spinner {
    fn ui(self, ui: &mut Ui) -> Response {
        let cor = self.cor.unwrap_or_else(|| ui.cores().rubro);
        let (rect, resp) = ui.allocate_exact_size(Vec2::splat(self.tamanho), Sense::hover());

        if ui.is_rect_visible(rect) {
            ui.ctx().request_repaint();
            let t = ui.input(|i| i.time);
            let raio_ponto = self.tamanho * 0.14;
            let vao = (self.tamanho - raio_ponto * 2.0) / 2.0;
            let base_x = rect.center().x - vao;
            let y = rect.center().y;
            let painter = ui.painter();
            for k in 0..3 {
                // Onda senoidal defasada — cada ponto pulsa 1/3 de ciclo depois do anterior.
                let fase = (t * 2.4 - f64::from(k) * 0.5).sin() as f32;
                let escala = 0.45 + 0.55 * (0.5 + 0.5 * fase);
                let alfa = (0.35 + 0.65 * (0.5 + 0.5 * fase)).clamp(0.0, 1.0);
                painter.circle_filled(
                    egui::pos2(base_x + vao * k as f32, y),
                    raio_ponto * escala,
                    cor.gamma_multiply(alfa),
                );
            }
        }
        resp
    }
}
