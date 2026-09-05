//! Camada 1 (atoms) — `Rotulo`: um texto num papel tipográfico do design system, como
//! `Widget`. `docs/12-ui-ux.md` §3.
//!
//! É o jeito recomendado de escrever texto numa tela (`ui.add(Rotulo::titulo_tela("Pulso"))`)
//! — pega cor e fonte do papel + tema sozinho, sem `&Cores` na chamada. O antigo
//! `Papel::texto(&cores)` continua existindo para quem precisa de um `RichText` cru.

use egui::{Color32, Response, Ui, Widget};

use crate::tokens::{Papel, TemaUi};

/// Um texto tipografado pelo design system.
#[must_use]
pub struct Rotulo {
    texto: String,
    papel: Papel,
    cor: Option<Color32>,
    quebra: bool,
}

impl Rotulo {
    /// Um rótulo com papel e texto explícitos.
    pub fn novo(papel: Papel, texto: impl Into<String>) -> Self {
        Self {
            texto: texto.into(),
            papel,
            cor: None,
            quebra: false,
        }
    }

    /// Título de tela (20px/600, `texto_forte`).
    pub fn titulo_tela(texto: impl Into<String>) -> Self {
        Self::novo(Papel::TituloTela, texto)
    }

    /// Título de seção (15px/600).
    pub fn titulo_secao(texto: impl Into<String>) -> Self {
        Self::novo(Papel::TituloSecao, texto)
    }

    /// Rótulo de campo (12px/500, `texto_medio`) — também serve de legenda.
    pub fn campo(texto: impl Into<String>) -> Self {
        Self::novo(Papel::RotuloCampo, texto)
    }

    /// Texto de interface padrão (13px/400).
    pub fn interface(texto: impl Into<String>) -> Self {
        Self::novo(Papel::Interface, texto)
    }

    /// Código / chave de acesso / log (12px, monoespaçada).
    pub fn codigo(texto: impl Into<String>) -> Self {
        Self::novo(Papel::Codigo, texto)
    }

    /// Sobrescreve a cor (ex.: um semântico — `ui.cores().negativo` para erro).
    pub const fn cor(mut self, cor: Color32) -> Self {
        self.cor = Some(cor);
        self
    }

    /// Deixa o texto quebrar em várias linhas em vez de truncar.
    pub const fn quebravel(mut self) -> Self {
        self.quebra = true;
        self
    }
}

impl Widget for Rotulo {
    fn ui(self, ui: &mut Ui) -> Response {
        let cores = ui.cores();
        let mut rt = self.papel.texto(self.texto, &cores);
        if let Some(cor) = self.cor {
            rt = rt.color(cor);
        }
        ui.add(egui::Label::new(rt).wrap_mode(if self.quebra {
            egui::TextWrapMode::Wrap
        } else {
            egui::TextWrapMode::Truncate
        }))
    }
}
