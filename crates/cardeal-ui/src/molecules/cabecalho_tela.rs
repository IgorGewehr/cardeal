//! Camada 2 (molecules) — `CabecalhoTela`: o topo padrão de toda tela de módulo.
//! Título à esquerda, ações à direita, divisória embaixo. `docs/12-ui-ux.md` §7.

use egui::Ui;

use crate::atoms::Rotulo;
use crate::tokens::{Espaco, TemaUi};

/// O cabeçalho de uma tela: título + zona de ações.
#[must_use]
pub struct CabecalhoTela {
    titulo: String,
}

impl CabecalhoTela {
    /// Um cabeçalho com o título dado.
    pub fn novo(titulo: impl Into<String>) -> Self {
        Self {
            titulo: titulo.into(),
        }
    }

    /// Desenha o cabeçalho. `acoes` monta os botões à direita (recarregar, novo, etc.),
    /// já num `Ui` alinhado da direita para a esquerda.
    pub fn mostrar(self, ui: &mut Ui, acoes: impl FnOnce(&mut Ui)) {
        let cores = ui.cores();
        ui.horizontal(|ui| {
            ui.add(Rotulo::titulo_tela(self.titulo));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), acoes);
        });
        ui.add_space(Espaco::E12);
        let sep = ui.available_rect_before_wrap();
        ui.painter().hline(
            sep.x_range(),
            sep.top(),
            egui::Stroke::new(1.0_f32, cores.borda),
        );
        ui.add_space(Espaco::E16);
    }
}
