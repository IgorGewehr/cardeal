//! Camada 2 (molecules) — `ItemDeLista`. `docs/12-ui-ux.md` §7.
//!
//! Uma linha de lista clicável no conteúdo inteiro, sobre
//! [`superficie_clicavel`](crate::atoms::superficie_clicavel) — o mesmo alvo de clique e o
//! mesmo indicador de seleção da sidebar. Título, subtítulo opcional e um traço à direita
//! (valor, badge, seta).

use egui::{Response, Ui};

use crate::atoms::{altura_item_duplo, superficie_clicavel, Rotulo, Tecla};
use crate::tokens::{AlturaLinha, Espaco, Papel, TemaUi};

/// Uma linha de lista genérica.
#[must_use]
pub struct ItemDeLista {
    titulo: String,
    subtitulo: Option<String>,
    selecionado: bool,
    atalho: Option<String>,
    esmaecido: bool,
}

impl ItemDeLista {
    /// Uma linha só com título.
    pub fn novo(titulo: impl Into<String>) -> Self {
        Self {
            titulo: titulo.into(),
            subtitulo: None,
            selecionado: false,
            atalho: None,
            esmaecido: false,
        }
    }

    /// Mostra uma [`Tecla`] à esquerda do título — o atalho que escolhe esta linha (`1`,
    /// `F3`…). Só anote atalhos que a tela realmente trata.
    pub fn atalho(mut self, tecla: impl Into<String>) -> Self {
        self.atalho = Some(tecla.into());
        self
    }

    /// Texto esmaecido — linha inativa (item cancelado, opção indisponível).
    pub const fn esmaecido(mut self, v: bool) -> Self {
        self.esmaecido = v;
        self
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

        let fraco = ui.cores().texto_fraco;
        superficie_clicavel(ui, self.selecionado, altura, |ui| {
            if let Some(tecla) = self.atalho {
                ui.add(Tecla::nova(tecla));
                ui.add_space(Espaco::E12);
            }
            // O `ui.vertical` dentro de um layout centralizado começa no topo da linha, não no
            // meio: centraliza-se o bloco de texto na mão, com a altura real das fontes.
            let (h_titulo, h_sub) = ui.fonts(|f| {
                (
                    f.row_height(&Papel::Interface.font_id()),
                    f.row_height(&Papel::RotuloCampo.font_id()),
                )
            });
            let bloco = if self.subtitulo.is_some() {
                h_titulo + h_sub + ui.spacing().item_spacing.y
            } else {
                h_titulo
            };
            ui.vertical(|ui| {
                ui.add_space(((altura - bloco) / 2.0).max(0.0));
                let titulo = Rotulo::interface(self.titulo);
                ui.add(if self.esmaecido {
                    titulo.cor(fraco)
                } else {
                    titulo
                });
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
