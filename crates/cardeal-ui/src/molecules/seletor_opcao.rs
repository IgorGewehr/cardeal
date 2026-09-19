//! Camada 2 (molecules) — `SeletorOpcao`: um dropdown no estilo Rubro, para substituir o
//! `egui::ComboBox::from_label` cru das telas (rótulo à direita, largura de fábrica,
//! tipografia fora do sistema). `docs/12-ui-ux.md` §7.
//!
//! Genérico sobre o tipo do valor selecionado (`Id`, um enum, etc.), desde que seja
//! `PartialEq + Copy`.

use egui::{Response, Ui, Vec2};

use crate::atoms::{moldura_foco_campo, Rotulo};
use crate::tokens::{Espaco, Papel, TemaUi};

/// Um seletor de uma opção entre várias.
#[must_use]
pub struct SeletorOpcao<'a, T> {
    rotulo: String,
    selecionado: &'a mut Option<T>,
    opcoes: Vec<(T, String)>,
    placeholder: String,
    com_rotulo: bool,
}

impl<'a, T: PartialEq + Copy> SeletorOpcao<'a, T> {
    /// Um seletor com rótulo, ligado a `selecionado`.
    pub fn novo(rotulo: impl Into<String>, selecionado: &'a mut Option<T>) -> Self {
        Self {
            rotulo: rotulo.into(),
            selecionado,
            opcoes: Vec::new(),
            placeholder: "Selecione...".to_owned(),
            com_rotulo: true,
        }
    }

    /// Acrescenta uma opção.
    pub fn opcao(mut self, valor: T, texto: impl Into<String>) -> Self {
        self.opcoes.push((valor, texto.into()));
        self
    }

    /// Acrescenta várias opções de uma vez.
    pub fn opcoes<I, S>(mut self, it: I) -> Self
    where
        I: IntoIterator<Item = (T, S)>,
        S: Into<String>,
    {
        self.opcoes
            .extend(it.into_iter().map(|(v, s)| (v, s.into())));
        self
    }

    /// Não desenha o rótulo acima do combo — para barras de ferramentas, onde o valor
    /// selecionado já diz do que se trata ("Todos os recursos"). O rótulo continua sendo o
    /// identificador interno do widget, então deve ser único.
    pub const fn sem_rotulo(mut self) -> Self {
        self.com_rotulo = false;
        self
    }

    /// Texto mostrado quando nada está selecionado.
    pub fn placeholder(mut self, s: impl Into<String>) -> Self {
        self.placeholder = s.into();
        self
    }

    /// Desenha o seletor. Devolve a `Response` do botão do combo.
    pub fn mostrar(self, ui: &mut Ui) -> Response {
        let Self {
            rotulo,
            selecionado,
            opcoes,
            placeholder,
            com_rotulo,
        } = self;
        let cores = ui.cores();

        ui.vertical(|ui| {
            if com_rotulo {
                ui.add(Rotulo::campo(&rotulo));
                ui.add_space(Espaco::E4);
            }

            let atual = selecionado
                .and_then(|s| opcoes.iter().find(|(v, _)| *v == s))
                .map_or_else(|| placeholder.clone(), |(_, t)| t.clone());
            let vazio = selecionado.is_none();

            let resp = egui::ComboBox::from_id_salt(("seletor", rotulo.as_str()))
                .selected_text(
                    egui::RichText::new(atual)
                        .font(Papel::Interface.font_id())
                        .color(if vazio {
                            cores.texto_fraco
                        } else {
                            cores.texto
                        }),
                )
                .width((ui.available_width() - Espaco::E8).max(60.0_f32))
                .show_ui(ui, |ui| {
                    for (valor, texto) in &opcoes {
                        ui.selectable_value(selecionado, Some(*valor), texto.clone());
                    }
                })
                .response;
            // Mesmo anel de foco responsivo de um `Campo` — o botão do combo não tem margem
            // interna própria, então a moldura cola no `rect` da resposta.
            moldura_foco_campo(ui, &resp, Vec2::ZERO, None);
            resp
        })
        .inner
    }
}
