//! Camada 1 (atoms) — `CampoTexto`. `docs/12-ui-ux.md` §7 (base para `CampoMoeda`,
//! `CampoDocumento`, `CampoData`) e §9 (foco sempre visível: anel `rubro-500`).
//!
//! O visual do `TextEdit` (fundo, borda, borda de foco, arredondamento) já vem de
//! [`instalar_estilo`](crate::tokens::instalar_estilo); aqui só montamos rótulo + campo e o
//! preenchimento de largura.

use egui::{Response, Ui, Widget};

use crate::tokens::{Papel, TemaUi};

/// Um campo de texto de uma linha, com rótulo opcional.
#[must_use]
pub struct CampoTexto<'a> {
    valor: &'a mut String,
    rotulo: Option<String>,
    marcador: Option<String>,
    senha: bool,
    preenche_largura: bool,
}

impl<'a> CampoTexto<'a> {
    /// Um campo vazio, sem rótulo nem marcador.
    pub fn novo(valor: &'a mut String) -> Self {
        Self {
            valor,
            rotulo: None,
            marcador: None,
            senha: false,
            preenche_largura: true,
        }
    }

    /// Rótulo mostrado acima do campo (`docs/12-ui-ux.md` §3, papel `RotuloCampo`).
    pub fn rotulo(mut self, rotulo: impl Into<String>) -> Self {
        self.rotulo = Some(rotulo.into());
        self
    }

    /// Texto de exemplo mostrado quando o campo está vazio.
    pub fn marcador(mut self, marcador: impl Into<String>) -> Self {
        self.marcador = Some(marcador.into());
        self
    }

    /// Mascara o conteúdo digitado (campo de senha).
    pub const fn senha(mut self, v: bool) -> Self {
        self.senha = v;
        self
    }

    /// Se o campo ocupa toda a largura disponível (padrão: sim).
    pub const fn preenche_largura(mut self, v: bool) -> Self {
        self.preenche_largura = v;
        self
    }
}

impl Widget for CampoTexto<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let cores = ui.cores();
        ui.vertical(|ui| {
            if let Some(rotulo) = &self.rotulo {
                ui.add(crate::atoms::Rotulo::campo(rotulo.clone()));
                ui.add_space(crate::tokens::Espaco::E4);
            }

            let mut edicao = egui::TextEdit::singleline(self.valor)
                .password(self.senha)
                .font(Papel::Interface.font_id())
                .text_color(cores.texto)
                .margin(egui::Margin::symmetric(10.0_f32, 8.0_f32));
            if self.preenche_largura {
                edicao = edicao.desired_width(f32::INFINITY);
            }
            if let Some(marcador) = &self.marcador {
                edicao = edicao.hint_text(marcador.clone());
            }
            ui.add(edicao)
        })
        .inner
    }
}
