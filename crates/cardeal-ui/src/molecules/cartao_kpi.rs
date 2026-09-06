//! Camada 2 (molecules) — `CartaoKpi`. Os cartões de topo do Pulso (`docs/12-ui-ux.md` §6):
//! rótulo, valor em destaque, variação opcional. Largura flexível — pensado para viver
//! dentro de um layout de colunas responsivo (ver `organisms::FaixaKpi`).

use cardeal_kernel::Dinheiro;
use egui::{Ui, Widget};

use crate::atoms::{Rotulo, ValorDinheiro};
use crate::tokens::{Espaco, Raio, TemaUi};

/// Um cartão de indicador do Pulso — ex.: "SALDO HOJE".
#[must_use]
pub struct CartaoKpi {
    rotulo: String,
    valor: Dinheiro,
    variacao: Option<String>,
}

impl CartaoKpi {
    /// Um cartão com o rótulo e o valor pedidos, sem variação.
    pub fn novo(rotulo: impl Into<String>, valor: Dinheiro) -> Self {
        Self {
            rotulo: rotulo.into(),
            valor,
            variacao: None,
        }
    }

    /// Texto de variação mostrado abaixo do valor (ex.: "7 títulos", "▲ 4,2% vs ontem").
    pub fn variacao(mut self, texto: impl Into<String>) -> Self {
        self.variacao = Some(texto.into());
        self
    }
}

impl Widget for CartaoKpi {
    fn ui(self, ui: &mut Ui) -> egui::Response {
        let cores = ui.cores();
        egui::Frame::none()
            .fill(cores.superficie)
            .stroke(egui::Stroke::new(1.0_f32, cores.borda))
            .rounding(Raio::CARTAO)
            .shadow(crate::tokens::sombra_cartao(ui.ctx()))
            .inner_margin(Espaco::E16)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width().max(0.0));
                ui.vertical(|ui| {
                    ui.add(Rotulo::campo(self.rotulo.to_uppercase()));
                    ui.add_space(Espaco::E4);
                    ui.add(ValorDinheiro::novo(self.valor).destaque());
                    if let Some(variacao) = self.variacao {
                        ui.add_space(Espaco::E4);
                        ui.add(Rotulo::campo(variacao));
                    }
                });
            })
            .response
    }
}
