//! Camada 3 (organisms) — `Janela`: o desenho do **shell** da janela sem decoração nativa
//! (`with_decorations(false)`): a barra de título com arrastar, duplo-clique e os três botões,
//! e as molduras dos painéis, cujos cantos externos acompanham o raio da janela.
//!
//! Vive no design system porque é aparência (cor, raio, altura), não lógica de tela — e o ADR-0015
//! não abre exceção para o `main.rs`.
//!
//! Os cantos: a barra de título arredonda os de cima; a sidebar, o canto inferior esquerdo; o
//! painel central, o inferior direito (e o esquerdo quando não há sidebar).

use egui::{Context, Frame, Margin, Rounding};

use crate::atoms::{BotaoJanela, Rotulo, TipoBotaoJanela};
use crate::tokens::{Espaco, Tema};

/// O shell de uma janela sem decoração nativa.
#[derive(Debug, Clone, Copy)]
#[must_use]
pub struct Janela {
    raio: f32,
    altura_barra: f32,
}

impl Janela {
    /// Um shell com o raio dos cantos da janela e a altura da barra de título, em pixels.
    pub const fn nova(raio: f32, altura_barra: f32) -> Self {
        Self { raio, altura_barra }
    }

    /// A barra de título: logo, nome, e minimizar/maximizar/fechar. Arrasta a janela, e o
    /// duplo-clique maximiza ou restaura. Sempre visível — inclusive antes do login.
    pub fn barra_titulo(
        self,
        ctx: &Context,
        tema: Tema,
        logo: (&'static str, &'static [u8]),
        titulo: &str,
    ) {
        let cores = tema.cores();
        let raio = self.raio;
        egui::TopBottomPanel::top("barra_titulo")
            .exact_height(self.altura_barra)
            .frame(Frame::none().fill(cores.superficie_2).rounding(Rounding {
                nw: raio,
                ne: raio,
                sw: 0.0,
                se: 0.0,
            }))
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                let resp = ui.interact(
                    rect,
                    egui::Id::new("arrastar-janela"),
                    egui::Sense::click_and_drag(),
                );
                let maximizada = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                if resp.double_clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximizada));
                } else if resp.drag_started_by(egui::PointerButton::Primary) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }

                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| {
                    ui.horizontal_centered(|ui| {
                        ui.add_space(Espaco::E12);
                        ui.add(
                            egui::Image::from_bytes(logo.0, logo.1)
                                .max_height(20.0)
                                .fit_to_original_size(1.0),
                        );
                        ui.add_space(Espaco::E8);
                        ui.add(Rotulo::titulo_secao(titulo));

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            self.botoes(ui, ctx, maximizada);
                        });
                    });
                });
            });
    }

    /// Fechar, maximizar/restaurar e minimizar — da direita para a esquerda.
    fn botoes(self, ui: &mut egui::Ui, ctx: &Context, maximizada: bool) {
        let botao = |tipo| BotaoJanela::novo(tipo).altura(self.altura_barra);
        if ui.add(botao(TipoBotaoJanela::Fechar)).clicked() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let tipo = if maximizada {
            TipoBotaoJanela::Restaurar
        } else {
            TipoBotaoJanela::Maximizar
        };
        if ui.add(botao(tipo)).clicked() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximizada));
        }
        if ui.add(botao(TipoBotaoJanela::Minimizar)).clicked() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
    }

    /// A moldura da sidebar: `superficie_2`, com o canto inferior esquerdo da janela (a sidebar
    /// é o rodapé esquerdo).
    pub fn moldura_sidebar(self, tema: Tema) -> Frame {
        Frame::none()
            .fill(tema.cores().superficie_2)
            .inner_margin(Margin::same(Espaco::E8))
            .rounding(Rounding {
                nw: 0.0,
                ne: 0.0,
                sw: self.raio,
                se: 0.0,
            })
    }

    /// A moldura do painel central: `fundo`, com o canto inferior direito da janela — e o
    /// esquerdo também, quando não há sidebar (`tem_sidebar == false`).
    pub fn moldura_conteudo(self, tema: Tema, tem_sidebar: bool) -> Frame {
        Frame::none()
            .fill(tema.cores().fundo)
            .inner_margin(Margin::same(Espaco::E24))
            .rounding(Rounding {
                nw: 0.0,
                ne: 0.0,
                sw: if tem_sidebar { 0.0 } else { self.raio },
                se: self.raio,
            })
    }
}
