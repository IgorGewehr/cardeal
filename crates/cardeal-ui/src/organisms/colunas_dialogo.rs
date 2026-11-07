//! Camada 3 (organisms) — `ColunasDialogo`: o corpo de um [`Dialogo::amplo`](super::Dialogo)
//! em duas colunas — uma **lateral** estreita, em `superficie_2` de borda a borda (quem é, em
//! que pé está, o que já aconteceu), e a **principal** (o trabalho: campos e itens). Cada
//! coluna rola sozinha, então a linha do tempo à esquerda não some quando se desce o
//! orçamento à direita.

use egui::{Margin, Stroke, Ui};

use crate::tokens::{Espaco, TemaUi};

/// Largura padrão da coluna lateral.
const LARGURA_LATERAL: f32 = 340.0;

/// Duas colunas roláveis para o corpo de um dialog amplo.
#[must_use]
pub struct ColunasDialogo {
    largura_lateral: f32,
    id: &'static str,
}

impl ColunasDialogo {
    /// Colunas com a lateral de 340px. `id` distingue a rolagem de dialogs diferentes.
    pub const fn nova(id: &'static str) -> Self {
        Self {
            largura_lateral: LARGURA_LATERAL,
            id,
        }
    }

    /// Sobrescreve a largura da coluna lateral (limitada a 40% do espaço).
    pub const fn lateral(mut self, px: f32) -> Self {
        self.largura_lateral = px;
        self
    }

    /// Desenha as duas colunas, ocupando toda a área disponível.
    pub fn mostrar<T>(
        self,
        ui: &mut Ui,
        estado: &mut T,
        lateral: impl FnOnce(&mut Ui, &mut T),
        principal: impl FnOnce(&mut Ui, &mut T),
    ) {
        let cores = ui.cores();
        let area = ui.available_rect_before_wrap();
        let largura_lateral = self.largura_lateral.min(area.width() * 0.4);
        let (r_lateral, r_principal) = area.split_left_right_at_x(area.left() + largura_lateral);

        ui.painter().rect_filled(r_lateral, 0.0, cores.superficie_2);
        ui.painter().line_segment(
            [r_lateral.right_top(), r_lateral.right_bottom()],
            Stroke::new(1.0_f32, cores.borda),
        );

        let margem_lateral = Margin {
            left: Espaco::E24,
            right: Espaco::E16,
            top: Espaco::E16,
            bottom: Espaco::E16,
        };
        let margem_principal = Margin::symmetric(Espaco::E24, Espaco::E16);
        coluna(ui, r_lateral, margem_lateral, (self.id, "lateral"), |ui| {
            lateral(ui, estado);
        });
        coluna(
            ui,
            r_principal,
            margem_principal,
            (self.id, "principal"),
            |ui| {
                principal(ui, estado);
            },
        );
        ui.advance_cursor_after_rect(area);
    }
}

fn coluna(
    ui: &mut Ui,
    rect: egui::Rect,
    margem: Margin,
    id: (&'static str, &'static str),
    conteudo: impl FnOnce(&mut Ui),
) {
    let mut filho = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    filho.set_clip_rect(rect.intersect(ui.clip_rect()));
    egui::ScrollArea::vertical()
        .id_salt(id)
        .auto_shrink([false, false])
        .max_height(rect.height())
        .show(&mut filho, |ui| {
            egui::Frame::none().inner_margin(margem).show(ui, |ui| {
                ui.set_width((rect.width() - margem.left - margem.right).max(0.0));
                conteudo(ui);
            });
        });
}
