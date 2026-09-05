//! Camada 3 (organisms) — `Cartao`. Um painel com elevação e largura máxima, usado para
//! centralizar formulários curtos (login, primeiro acesso) — minimalismo não é deixar
//! campos esticarem a largura inteira da janela.

use egui::Ui;

use crate::tokens::{Espaco, Raio, TemaUi};

/// Um cartão de largura limitada, centralizado horizontalmente no espaço disponível.
#[must_use]
pub struct Cartao {
    largura_max: f32,
}

impl Cartao {
    /// Largura máxima padrão (420px) — confortável para um formulário de poucos campos.
    pub const fn novo() -> Self {
        Self { largura_max: 420.0 }
    }

    /// Sobrescreve a largura máxima.
    pub const fn largura(mut self, px: f32) -> Self {
        self.largura_max = px;
        self
    }

    /// Desenha o cartão centralizado, com o conteúdo montado por `conteudo`.
    pub fn mostrar(self, ui: &mut Ui, conteudo: impl FnOnce(&mut Ui)) {
        let cores = ui.cores();
        let disponivel = ui.available_width();
        let largura = self.largura_max.min(disponivel);
        let margem = ((disponivel - largura) / 2.0).max(0.0);
        ui.horizontal(|ui| {
            ui.add_space(margem);
            ui.vertical(|ui| {
                ui.set_max_width(largura);
                egui::Frame::none()
                    .fill(cores.superficie)
                    .stroke(egui::Stroke::new(1.0_f32, cores.borda))
                    .rounding(Raio::MODAL)
                    .inner_margin(Espaco::E32)
                    .show(ui, |ui| {
                        ui.set_width(largura - Espaco::E32 * 2.0);
                        conteudo(ui);
                    });
            });
        });
    }
}

impl Default for Cartao {
    fn default() -> Self {
        Self::novo()
    }
}
