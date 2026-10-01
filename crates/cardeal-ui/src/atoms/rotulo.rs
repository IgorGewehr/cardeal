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

    /// Título de tela (24px semibold, `texto_forte`).
    pub fn titulo_tela(texto: impl Into<String>) -> Self {
        Self::novo(Papel::TituloTela, texto)
    }

    /// Título de seção, de cartão ou de diálogo (16px semibold, `texto_forte`).
    pub fn titulo_secao(texto: impl Into<String>) -> Self {
        Self::novo(Papel::TituloSecao, texto)
    }

    /// Rótulo de campo (13px Medium, `texto_medio`) — o nome acima de um campo ou valor.
    /// Para frase explicativa use [`Rotulo::apoio`].
    pub fn campo(texto: impl Into<String>) -> Self {
        Self::novo(Papel::RotuloCampo, texto)
    }

    /// Texto de interface padrão (14px Regular).
    pub fn interface(texto: impl Into<String>) -> Self {
        Self::novo(Papel::Interface, texto)
    }

    /// Código / chave de acesso / log (12px, monoespaçada).
    pub fn codigo(texto: impl Into<String>) -> Self {
        Self::novo(Papel::Codigo, texto)
    }

    /// Sobrelinha (11px semibold, CAIXA ALTA espaçada, `texto_medio`) — o rótulo curto acima
    /// de um valor ou de um grupo de controles ("Período", "A receber em aberto"). Escreva
    /// em caixa normal: o papel converte.
    pub fn sobrelinha(texto: impl Into<String>) -> Self {
        Self::novo(Papel::Sobrelinha, texto)
    }

    /// Texto de apoio (13px Regular, `texto_medio`) — explicação, dica, linha secundária.
    /// Quebra linha por padrão: explicação nunca deve sumir truncada.
    pub fn apoio(texto: impl Into<String>) -> Self {
        Self::novo(Papel::Apoio, texto).quebravel()
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
