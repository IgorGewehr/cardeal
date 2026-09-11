//! Camada 2 (molecules) — `Campo`: rótulo + entrada de texto + mensagem de erro, num bloco
//! só. `docs/12-ui-ux.md` §7 e §9.
//!
//! É o que as telas devem usar em formulário (em vez de repetir `Rotulo::campo` +
//! `CampoTexto` + tratamento de erro na mão). Com `erro`, o contorno fica `negativo` e a
//! mensagem aparece abaixo.

use std::str::FromStr;

use egui::{Response, Ui, Widget};

use crate::atoms::{moldura_foco_campo, Rotulo, MARGEM_CAMPO};
use crate::tokens::{Espaco, Papel, TemaUi};

/// Máscara aplicada ao valor quando o campo perde o foco.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mascara {
    /// Sem formatação.
    #[default]
    Nenhuma,
    /// CPF ou CNPJ, escolhido pela contagem de dígitos (11 → CPF, 14 → CNPJ).
    Documento,
    /// Data — normaliza `10/3`, `1003`, `2026-03-10` para `dd/mm/aaaa`.
    Data,
}

impl Mascara {
    fn aplicar(self, bruto: &str) -> String {
        match self {
            Self::Nenhuma => bruto.to_owned(),
            Self::Documento => formatar_documento(bruto),
            Self::Data => formatar_data(bruto),
        }
    }
}

fn formatar_documento(bruto: &str) -> String {
    let d: String = bruto.chars().filter(char::is_ascii_digit).collect();
    match d.len() {
        11 => format!("{}.{}.{}-{}", &d[0..3], &d[3..6], &d[6..9], &d[9..11]),
        14 => format!(
            "{}.{}.{}/{}-{}",
            &d[0..2],
            &d[2..5],
            &d[5..8],
            &d[8..12],
            &d[12..14]
        ),
        _ => bruto.trim().to_owned(),
    }
}

fn formatar_data(bruto: &str) -> String {
    cardeal_kernel::Data::from_str(bruto.trim())
        .map_or_else(|_| bruto.trim().to_owned(), |d| d.to_string())
}

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
    mascara: Mascara,
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
            mascara: Mascara::Nenhuma,
        }
    }

    /// Modo leitura: mostra o valor como texto, sem caixa de edição.
    pub const fn somente_leitura(mut self, v: bool) -> Self {
        self.somente_leitura = v;
        self
    }

    /// Formata o valor ao perder o foco (CPF/CNPJ, data).
    pub const fn mascara(mut self, m: Mascara) -> Self {
        self.mascara = m;
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
                        v.selection.stroke = egui::Stroke::new(1.0_f32, cores.negativo);
                    }
                    let mut edicao = egui::TextEdit::singleline(self.valor)
                        .password(self.senha)
                        .font(Papel::Interface.font_id())
                        .text_color(cores.texto)
                        .desired_width(f32::INFINITY)
                        .margin(egui::Margin::symmetric(MARGEM_CAMPO.x, MARGEM_CAMPO.y));
                    if let Some(m) = &self.marcador {
                        edicao = edicao.hint_text(m.clone());
                    }
                    let resp = ui.add(edicao);
                    moldura_foco_campo(ui, &resp, MARGEM_CAMPO, com_erro.then_some(cores.negativo));
                    resp
                })
                .inner;

            if self.mascara != Mascara::Nenhuma && resp.lost_focus() {
                let f = self.mascara.aplicar(self.valor);
                if f != *self.valor {
                    *self.valor = f;
                }
            }

            if let Some(erro) = self.erro {
                ui.add_space(Espaco::E4);
                ui.add(Rotulo::campo(erro).cor(cores.negativo));
            }
            resp
        })
        .inner
    }
}
