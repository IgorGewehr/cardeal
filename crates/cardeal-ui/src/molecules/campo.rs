//! Camada 2 (molecules) — `Campo`: rótulo + entrada de texto + mensagem de erro, num bloco
//! só. `docs/12-ui-ux.md` §7 e §9.
//!
//! É o que as telas devem usar em formulário (em vez de repetir `Rotulo::campo` +
//! `CampoTexto` + tratamento de erro na mão). Com `erro`, o contorno fica `negativo` e a
//! mensagem aparece abaixo.

use egui::{Response, Ui, Widget};

use crate::atoms::Rotulo;
use crate::tokens::{Espaco, Papel, TemaUi};

/// Um campo de formulário completo.
///
/// Serve os três modos do [`Dialogo`](crate::organisms::Dialogo): em `.somente_leitura(true)`
/// (modo "Ver") o valor aparece como texto formatado, não como `TextEdit`.
#[must_use]
pub struct Campo<'a> {
    valor: &'a mut String,
    rotulo: String,
    marcador: Option<String>,
    erro: Option<String>,
    senha: bool,
    somente_leitura: bool,
}

impl<'a> Campo<'a> {
    /// Um campo com rótulo, ligado a `valor`.
    pub fn novo(rotulo: impl Into<String>, valor: &'a mut String) -> Self {
        Self {
            valor,
            rotulo: rotulo.into(),
            marcador: None,
            erro: None,
            senha: false,
            somente_leitura: false,
        }
    }

    /// Modo leitura: mostra o valor como texto, sem caixa de edição.
    pub const fn somente_leitura(mut self, v: bool) -> Self {
        self.somente_leitura = v;
        self
    }

    /// Texto de exemplo quando vazio.
    pub fn marcador(mut self, marcador: impl Into<String>) -> Self {
        self.marcador = Some(marcador.into());
        self
    }

    /// Mensagem de erro — pinta o contorno de `negativo` e mostra o texto abaixo.
    pub fn erro(mut self, erro: Option<impl Into<String>>) -> Self {
        self.erro = erro.map(Into::into);
        self
    }

    /// Campo de senha.
    pub const fn senha(mut self, v: bool) -> Self {
        self.senha = v;
        self
    }
}

impl Widget for Campo<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let cores = ui.cores();
        let com_erro = self.erro.is_some();

        ui.vertical(|ui| {
            ui.add(Rotulo::campo(self.rotulo));
            ui.add_space(Espaco::E4);

            if self.somente_leitura {
                let texto = if self.valor.trim().is_empty() {
                    "—".to_owned()
                } else if self.senha {
                    "••••••••".to_owned()
                } else {
                    self.valor.clone()
                };
                return ui.add(Rotulo::interface(texto).quebravel());
            }

            let resp = ui
                .scope(|ui| {
                    if com_erro {
                        let v = ui.visuals_mut();
                        v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, cores.negativo);
                        v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, cores.negativo);
                        v.widgets.active.bg_stroke = egui::Stroke::new(2.0_f32, cores.negativo);
                    }
                    let mut edicao = egui::TextEdit::singleline(self.valor)
                        .password(self.senha)
                        .font(Papel::Interface.font_id())
                        .text_color(cores.texto)
                        .desired_width(f32::INFINITY)
                        .margin(egui::Margin::symmetric(10.0_f32, 8.0_f32));
                    if let Some(m) = &self.marcador {
                        edicao = edicao.hint_text(m.clone());
                    }
                    ui.add(edicao)
                })
                .inner;

            if let Some(erro) = self.erro {
                ui.add_space(Espaco::E4);
                ui.add(Rotulo::campo(erro).cor(cores.negativo));
            }
            resp
        })
        .inner
    }
}
