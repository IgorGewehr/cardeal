//! Camada 1 (atoms) — `superficie_clicavel`: uma linha de largura cheia e altura fixa,
//! clicável no conteúdo inteiro.
//!
//! Base de item de sidebar, linha de lista e linha de grade.
//!
//! **O truque que conserta o clique:** o conteúdo (ícone, rótulo) é desenhado *primeiro*,
//! num `Ui` filho recortado — com `selectable_labels = false`, senão o `Label` do egui 0.29
//! passa a sentir arrasto/clique para seleção de texto e **come o clique da linha**. Só
//! *depois* a linha reivindica o clique com `ui.interact` sobre o `rect` inteiro; como é a
//! última interação registrada ali, fica no topo. (O hover do fundo usa
//! `rect_contains_pointer`, que não registra widget.)

use egui::{CursorIcon, Layout, Rect, Response, Sense, UiBuilder};

use crate::tokens::{AlturaLinha, Espaco, Raio, TemaUi};

/// Desenha uma linha clicável de largura cheia e devolve a `Response` da linha inteira.
pub fn superficie_clicavel(
    ui: &mut egui::Ui,
    selecionada: bool,
    altura: f32,
    conteudo: impl FnOnce(&mut egui::Ui),
) -> Response {
    let cores = ui.cores();
    let largura = ui.available_width().max(1.0);

    let (rect, resposta) = ui.allocate_exact_size(egui::vec2(largura, altura), Sense::click());
    let sob_ponteiro = ui.rect_contains_pointer(rect);

    if ui.is_rect_visible(rect) {
        let fundo = if selecionada {
            Some(cores.rubro_ativo)
        } else if sob_ponteiro {
            Some(cores.superficie_hover)
        } else {
            None
        };
        if let Some(cor) = fundo {
            ui.painter().rect_filled(rect, Raio::ITEM, cor);
        }
        if selecionada {
            let h = (rect.height() * 0.55).min(20.0);
            let barra = Rect::from_center_size(
                egui::pos2(rect.left() + 1.5, rect.center().y),
                egui::vec2(3.0, h),
            );
            ui.painter().rect_filled(barra, Raio::PILULA, cores.rubro);
        }

        let miolo = rect.shrink2(egui::vec2(Espaco::E12, 0.0));
        let mut filho = ui.new_child(
            UiBuilder::new()
                .max_rect(miolo)
                .layout(Layout::left_to_right(egui::Align::Center)),
        );
        filho.set_clip_rect(rect);
        filho.set_min_size(miolo.size());
        // Sem seleção de texto: no egui 0.29 um `Label` selecionável sente arrasto e rouba
        // o clique da linha inteira — o bug histórico de "a label da sidebar não clica".
        filho.style_mut().interaction.selectable_labels = false;
        conteudo(&mut filho);
    }

    if resposta.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    resposta
}

/// Altura de linha recomendada para item de navegação (sidebar).
#[must_use]
pub const fn altura_navegacao() -> f32 {
    40.0
}

/// Altura de linha recomendada para item de lista com título + subtítulo.
#[must_use]
pub fn altura_item_duplo() -> f32 {
    AlturaLinha::Confortavel.pixels() + 22.0
}
