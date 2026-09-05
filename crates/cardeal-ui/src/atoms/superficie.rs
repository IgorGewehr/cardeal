//! Camada 1 (atoms) — `superficie_clicavel`: uma linha de largura cheia e altura fixa,
//! clicável no conteúdo inteiro.
//!
//! Base de item de sidebar, linha de lista e linha de grade. Padrão robusto do egui:
//! aloca o `rect` da linha primeiro (`allocate_space`), sente o clique nesse `rect`
//! (`interact`), pinta o fundo, e só então desenha o conteúdo num `Ui` filho **recortado**
//! ao `rect` — rótulo longo trunca dentro da área clicável, nunca vaza dela.

use egui::{CursorIcon, Layout, Rect, Response, Sense, UiBuilder};

use crate::tokens::{AlturaLinha, Espaco, Raio, Rubro, TemaUi};

/// Desenha uma linha clicável de largura cheia e devolve a `Response` da linha inteira.
pub fn superficie_clicavel(
    ui: &mut egui::Ui,
    selecionada: bool,
    altura: f32,
    conteudo: impl FnOnce(&mut egui::Ui),
) -> Response {
    let cores = ui.cores();
    let largura = ui.available_width().max(1.0);

    let (id, rect) = ui.allocate_space(egui::vec2(largura, altura));
    let resposta = ui.interact(rect, id, Sense::click());

    let fundo = if selecionada {
        Some(Rubro::R50)
    } else if resposta.hovered() {
        Some(cores.superficie_2)
    } else {
        None
    };
    if let Some(cor) = fundo {
        ui.painter().rect_filled(rect, Raio::CARTAO, cor);
    }
    if selecionada {
        let barra = Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height()));
        ui.painter().rect_filled(barra, 0.0, Rubro::R500);
    }

    let miolo = rect.shrink2(egui::vec2(Espaco::E12, 0.0));
    let mut filho = ui.new_child(
        UiBuilder::new()
            .max_rect(miolo)
            .layout(Layout::left_to_right(egui::Align::Center)),
    );
    filho.set_clip_rect(rect);
    conteudo(&mut filho);

    if resposta.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    resposta
}

/// Altura de linha recomendada para item de navegação (sidebar).
#[must_use]
pub const fn altura_navegacao() -> f32 {
    38.0
}

/// Altura de linha recomendada para item de lista com título + subtítulo.
#[must_use]
pub fn altura_item_duplo() -> f32 {
    AlturaLinha::Confortavel.pixels() + 22.0
}
