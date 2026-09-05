//! Camada 2 (molecules) — `ItemDeLista`. `docs/12-ui-ux.md` §7.
//!
//! Uma linha de lista clicável no conteúdo inteiro, sobre
//! [`superficie_clicavel`](crate::atoms::superficie_clicavel) — o mesmo alvo de clique e o
//! mesmo indicador de seleção da sidebar. Título, subtítulo opcional e um traço à direita
//! (valor, badge, seta).

use egui::{Response, Ui};

use crate::atoms::{altura_item_duplo, superficie_clicavel, Rotulo};
use crate::tokens::{AlturaLinha, Espaco};

/// Uma linha de lista genérica.
#[must_use]
pub struct ItemDeLista {
    titulo: String,
    subtitulo: Option<String>,
    selecionado: bool,
}

impl ItemDeLista {
    /// Uma linha só com título.
    pub fn novo(titulo: impl Into<String>) -> Self {
        Self {
            titulo: titulo.into(),
            subtitulo: None,
            selecionado: false,
        }
    }

    /// Segunda linha, em `texto_medio`, abaixo do título.
    pub fn subtitulo(mut self, subtitulo: impl Into<String>) -> Self {
        self.subtitulo = Some(subtitulo.into());
        self
    }

    /// Fundo `rubro-50` + barra `rubro-500` à esquerda (item ativo/selecionado).
    pub const fn selecionado(mut self, v: bool) -> Self {
        self.selecionado = v;
        self
    }

    /// Desenha a linha; `traco` monta o conteúdo à direita. Devolve a `Response` da linha
    /// inteira (`.clicked()`).
    pub fn mostrar(self, ui: &mut Ui, traco: impl FnOnce(&mut Ui)) -> Response {
        let altura = if self.subtitulo.is_some() {
            altura_item_duplo()
        } else {
            AlturaLinha::Confortavel.pixels() + Espaco::E8
        };

        superficie_clicavel(ui, self.selecionado, altura, |ui| {
            ui.vertical(|ui| {
                ui.add(Rotulo::interface(self.titulo));
                if let Some(sub) = self.subtitulo {
                    ui.add(Rotulo::campo(sub));
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(Espaco::E12);
                traco(ui);
            });
        })
    }
}
