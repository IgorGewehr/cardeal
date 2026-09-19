//! Camada 2 (molecules) — `CampoBusca`: a entrada grande de busca/bipe, com indicador de
//! prompt (`▸`) e o anel de foco do design system.
//!
//! Pensada para o PDV (`docs/modulos/pdv.md` §10: "bipe o próximo item ou digite o código"):
//! o leitor de código de barras "digita" o código e manda `Enter`, então o campo precisa
//! estar **sempre focado** — [`CampoBusca::foco`] existe para a tela pedir o foco de volta
//! depois de cada ação. Devolve a `Response` do `TextEdit`; a tela trata o `Enter` com
//! `resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))`.

use egui::{Response, Ui, Widget};

use crate::atoms::moldura_foco_campo;
use crate::tokens::{Espaco, Papel, TemaUi};

/// Margem interna do campo grande — mais alto que um `Campo` de formulário.
const MARGEM: egui::Vec2 = egui::Vec2::new(14.0, 13.0);

/// Uma entrada de busca/bipe grande.
#[must_use]
pub struct CampoBusca<'a> {
    valor: &'a mut String,
    marcador: String,
    foco: bool,
}

impl<'a> CampoBusca<'a> {
    /// Um campo ligado a `valor`.
    pub fn novo(valor: &'a mut String) -> Self {
        Self {
            valor,
            marcador: String::new(),
            foco: false,
        }
    }

    /// Texto de exemplo quando vazio.
    pub fn marcador(mut self, marcador: impl Into<String>) -> Self {
        self.marcador = marcador.into();
        self
    }

    /// Pede o foco neste quadro (a tela decide quando — tipicamente quando nenhum outro
    /// widget tem o foco e nenhum diálogo está aberto).
    pub const fn foco(mut self, pedir: bool) -> Self {
        self.foco = pedir;
        self
    }
}

impl Widget for CampoBusca<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let cores = ui.cores();
        ui.horizontal(|ui| {
            // O prompt: um triângulo na cor da marca, desenhado (não um glifo — não depende
            // de a fonte ter o caractere).
            let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 20.0), egui::Sense::hover());
            let c = rect.center();
            ui.painter().add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(c.x - 4.0, c.y - 6.0),
                    egui::pos2(c.x + 5.0, c.y),
                    egui::pos2(c.x - 4.0, c.y + 6.0),
                ],
                cores.rubro,
                egui::Stroke::NONE,
            ));
            ui.add_space(Espaco::E4);

            let edicao = egui::TextEdit::singleline(self.valor)
                .font(Papel::TituloSecao.font_id())
                .text_color(cores.texto_forte)
                .hint_text(self.marcador)
                .desired_width(f32::INFINITY)
                .margin(egui::Margin::symmetric(MARGEM.x, MARGEM.y));
            let resp = ui.add(edicao);
            if self.foco {
                resp.request_focus();
            }
            moldura_foco_campo(ui, &resp, MARGEM, None);
            resp
        })
        .inner
    }
}
