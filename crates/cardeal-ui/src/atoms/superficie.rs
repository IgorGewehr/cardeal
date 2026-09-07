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

use crate::tokens::{ativar, lerp_cor, AlturaLinha, Espaco, Mov, Raio, TemaUi};

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
        // O hover acende e a seleção "entra" deslizando — nunca um corte seco. `ativar` só
        // mantém o repaint enquanto a transição corre (Pilar I).
        let th = ativar(ui, resposta.id.with("hover"), sob_ponteiro, Mov::RAPIDO);
        let ts = ativar(ui, resposta.id.with("sel"), selecionada, Mov::CALMA);

        if th > 0.001_f32 || ts > 0.001_f32 {
            let fundo = lerp_cor(
                cores.superficie_hover.gamma_multiply(th),
                cores.rubro_ativo,
                ts,
            );
            ui.painter().rect_filled(rect, Raio::ITEM, fundo);
        }
        if ts > 0.001_f32 {
            let h = (rect.height() * 0.55_f32).min(20.0_f32) * ts;
            let barra = Rect::from_center_size(
                egui::pos2(rect.left() + 1.5_f32, rect.center().y),
                egui::vec2(3.0_f32, h),
            );
            ui.painter()
                .rect_filled(barra, Raio::PILULA, cores.rubro.gamma_multiply(ts));
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
