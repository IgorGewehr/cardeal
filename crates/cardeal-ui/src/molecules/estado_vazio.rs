//! Camada 2 (molecules) — `EstadoVazio`: o que uma lista mostra quando não há nada.
//! `docs/12-ui-ux.md` §6.3 ("Nada exige sua atenção agora") — nunca uma área em branco.

use egui::Ui;

use crate::atoms::{desenhar_icone, Botao, Rotulo};
use crate::tokens::{Espaco, TemaUi};
use cardeal_modkit::Icone;

/// Um estado vazio centrado: ícone discreto, frase e uma ação opcional.
#[must_use]
pub struct EstadoVazio {
    icone: Icone,
    mensagem: String,
    acao: Option<String>,
}

impl EstadoVazio {
    /// Um estado vazio com ícone e mensagem.
    pub fn novo(icone: Icone, mensagem: impl Into<String>) -> Self {
        Self {
            icone,
            mensagem: mensagem.into(),
            acao: None,
        }
    }

    /// Acrescenta um botão de ação (ex.: "Cadastrar produto").
    pub fn acao(mut self, rotulo: impl Into<String>) -> Self {
        self.acao = Some(rotulo.into());
        self
    }

    /// Desenha o estado vazio. Devolve `true` se o botão de ação foi clicado.
    pub fn mostrar(self, ui: &mut Ui) -> bool {
        let cores = ui.cores();
        let mut clicado = false;
        ui.vertical_centered(|ui| {
            ui.add_space(Espaco::E48);
            desenhar_icone(ui, self.icone, 28.0, cores.texto_fraco);
            ui.add_space(Espaco::E12);
            ui.add(Rotulo::interface(self.mensagem).cor(cores.texto_medio));
            if let Some(rotulo) = self.acao {
                ui.add_space(Espaco::E16);
                clicado = ui.add(Botao::secundario(rotulo)).clicked();
            }
            ui.add_space(Espaco::E48);
        });
        clicado
    }
}
