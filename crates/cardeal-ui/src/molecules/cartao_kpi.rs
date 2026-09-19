//! Camada 2 (molecules) — `CartaoKpi`. Os cartões de topo do Pulso (`docs/12-ui-ux.md` §6):
//! rótulo, valor em destaque, variação opcional. Largura flexível — pensado para viver
//! dentro de um layout de colunas responsivo (ver `organisms::FaixaKpi`).

use cardeal_kernel::Dinheiro;
use cardeal_modkit::Icone;
use egui::{Ui, Widget};

use crate::atoms::{desenhar_icone, Rotulo, Tom, ValorDinheiro};
use crate::tokens::{Espaco, Papel, Raio, TemaUi};

/// O que um [`CartaoKpi`] mostra em destaque — dinheiro (formatado e colorido pelo sinal) ou
/// uma contagem/texto livre (uma contagem de itens não é dinheiro; forçá-la pelo formatador
/// monetário imprimiria "R$ 3,00" para "3 ordens").
enum ConteudoKpi {
    Dinheiro(Dinheiro),
    Texto(String),
}

/// Um cartão de indicador do Pulso — ex.: "SALDO HOJE".
#[must_use]
pub struct CartaoKpi {
    rotulo: String,
    conteudo: ConteudoKpi,
    variacao: Option<String>,
    tom: Option<Tom>,
    icone: Option<Icone>,
}

impl CartaoKpi {
    /// Um cartão de valor monetário, sem variação.
    pub fn novo(rotulo: impl Into<String>, valor: Dinheiro) -> Self {
        Self {
            rotulo: rotulo.into(),
            conteudo: ConteudoKpi::Dinheiro(valor),
            variacao: None,
            tom: None,
            icone: None,
        }
    }

    /// Um cartão de contagem/texto livre (não dinheiro) — ex.: "3" ordens em aberto.
    pub fn contagem(rotulo: impl Into<String>, valor: impl std::fmt::Display) -> Self {
        Self {
            rotulo: rotulo.into(),
            conteudo: ConteudoKpi::Texto(valor.to_string()),
            variacao: None,
            tom: None,
            icone: None,
        }
    }

    /// Acento de cor: uma faixa fina no topo do cartão no tom dado (positivo, atenção…).
    /// Comunica o "clima" do indicador antes de o olho ler o número.
    pub const fn tom(mut self, tom: Tom) -> Self {
        self.tom = Some(tom);
        self
    }

    /// Um ícone num chip tingido, no canto direito. Usa o tom do cartão (neutro se não houver).
    pub const fn icone(mut self, icone: Icone) -> Self {
        self.icone = Some(icone);
        self
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
        let resp = egui::Frame::none()
            .fill(cores.superficie)
            .stroke(egui::Stroke::new(1.0_f32, cores.borda))
            .rounding(Raio::CARTAO)
            .shadow(crate::tokens::sombra_cartao(ui.ctx()))
            .inner_margin(Espaco::E16)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width().max(0.0));
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.add(Rotulo::campo(self.rotulo.to_uppercase()));
                        ui.add_space(Espaco::E4);
                        match self.conteudo {
                            ConteudoKpi::Dinheiro(valor) => {
                                ui.add(ValorDinheiro::novo(valor).destaque());
                            }
                            ConteudoKpi::Texto(texto) => {
                                ui.add(Rotulo::novo(Papel::ValorDestaque, texto));
                            }
                        }
                        if let Some(variacao) = self.variacao {
                            ui.add_space(Espaco::E4);
                            ui.add(Rotulo::campo(variacao));
                        }
                    });
                    if let Some(icone) = self.icone {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                            let (fg, bg) = self.tom.unwrap_or(Tom::Neutro).cores(&cores);
                            egui::Frame::none()
                                .fill(bg)
                                .rounding(Raio::ITEM)
                                .inner_margin(Espaco::E8)
                                .show(ui, |ui| desenhar_icone(ui, icone, 20.0, fg));
                        });
                    }
                });
            })
            .response;

        if let Some(tom) = self.tom {
            // Faixa de acento no topo, recuada pelo raio para não vazar pelos cantos.
            let (fg, _) = tom.cores(&cores);
            let r = resp.rect;
            let faixa = egui::Rect::from_min_size(
                egui::pos2(r.left() + Raio::CARTAO, r.top()),
                egui::vec2((r.width() - Raio::CARTAO * 2.0).max(0.0), 3.0),
            );
            ui.painter().rect_filled(faixa, Raio::PILULA, fg);
        }
        resp
    }
}
