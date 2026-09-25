//! Camada 2 (molecules) — `LinhaDoTempo`: os passos de um processo, em coluna, com um ponto
//! e um traço ligando cada um ao seguinte. Passos feitos ficam preenchidos na cor positiva,
//! o atual em destaque (a cor da marca) e os que faltam vazados — dá para ler "onde a OS está
//! e o que falta" sem ler texto.

use egui::{Response, Sense, Stroke, Ui, Vec2, Widget};

use crate::atoms::Rotulo;
use crate::tokens::{Espaco, TemaUi};

/// Em que ponto um passo está.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SituacaoPasso {
    /// Já aconteceu.
    Feito,
    /// É onde o processo está agora.
    Atual,
    /// Ainda vai acontecer (ou pode acontecer).
    Pendente,
}

/// Um passo da linha do tempo.
#[derive(Debug, Clone)]
pub struct PassoLinhaDoTempo {
    /// O que é ("Orçamento aprovado").
    pub titulo: String,
    /// Quando e quem ("25/09 14:02 · Igor"); vazio para passos pendentes.
    pub detalhe: String,
    /// Feito, atual ou pendente.
    pub situacao: SituacaoPasso,
}

/// A coluna de passos.
#[must_use]
pub struct LinhaDoTempo<'a> {
    passos: &'a [PassoLinhaDoTempo],
}

impl<'a> LinhaDoTempo<'a> {
    /// A linha com os passos dados, na ordem.
    pub const fn nova(passos: &'a [PassoLinhaDoTempo]) -> Self {
        Self { passos }
    }
}

const RAIO_PONTO: f32 = 5.0;
const COLUNA_PONTO: f32 = 18.0;

impl Widget for LinhaDoTempo<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let cores = ui.cores();
        let n = self.passos.len();
        ui.vertical(|ui| {
            for (i, passo) in self.passos.iter().enumerate() {
                let (cor_ponto, cheio) = match passo.situacao {
                    SituacaoPasso::Feito => (cores.positivo, true),
                    SituacaoPasso::Atual => (cores.rubro, true),
                    SituacaoPasso::Pendente => (cores.texto_fraco, false),
                };
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(Vec2::new(COLUNA_PONTO, 36.0), Sense::hover());
                    let centro = egui::pos2(rect.center().x, rect.top() + 9.0);
                    if i + 1 < n {
                        let cor_traco = if passo.situacao == SituacaoPasso::Feito {
                            cores.positivo
                        } else {
                            cores.borda
                        };
                        ui.painter().line_segment(
                            [
                                egui::pos2(centro.x, centro.y + RAIO_PONTO),
                                egui::pos2(centro.x, rect.bottom() + Espaco::E4),
                            ],
                            Stroke::new(1.5_f32, cor_traco),
                        );
                    }
                    if cheio {
                        ui.painter().circle_filled(centro, RAIO_PONTO, cor_ponto);
                    } else {
                        ui.painter().circle_stroke(
                            centro,
                            RAIO_PONTO,
                            Stroke::new(1.5_f32, cor_ponto),
                        );
                    }
                    ui.vertical(|ui| {
                        let titulo = Rotulo::interface(passo.titulo.clone());
                        ui.add(match passo.situacao {
                            SituacaoPasso::Pendente => titulo.cor(cores.texto_fraco),
                            _ => titulo,
                        });
                        if !passo.detalhe.is_empty() {
                            ui.add(Rotulo::campo(passo.detalhe.clone()));
                        }
                    });
                });
            }
        })
        .response
    }
}
