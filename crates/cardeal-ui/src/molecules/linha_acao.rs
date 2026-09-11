//! Camada 2 (molecules) — `LinhaDeAcao`. A fila "exige ação hoje" do Pulso
//! (`docs/12-ui-ux.md` §6.3): severidade, título, valor opcional e **uma ação que resolve**.

use cardeal_kernel::Dinheiro;
use egui::{Color32, Ui};

use crate::atoms::{Botao, Rotulo, ValorDinheiro};
use crate::tokens::{Cores, Espaco, Raio, TemaUi};

/// A severidade de um item de ação — mapeia para um dos quatro tokens semânticos
/// (`docs/12-ui-ux.md` §2.3). Nunca só a cor: cada nível também tem um glifo distinto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severidade {
    /// 🔴 — requer ação imediata.
    Critica,
    /// 🟠 — requer atenção em breve.
    Atencao,
    /// 🔵 — informativo.
    Info,
    /// 🟢 — tudo certo.
    Ok,
}

impl Severidade {
    const fn cor(self, cores: &Cores) -> Color32 {
        match self {
            Self::Critica => cores.negativo,
            Self::Atencao => cores.atencao,
            Self::Info => cores.info,
            Self::Ok => cores.positivo,
        }
    }

    const fn glifo(self) -> &'static str {
        match self {
            Self::Critica => "\u{1F534}",
            Self::Atencao => "\u{1F7E0}",
            Self::Info => "\u{1F535}",
            Self::Ok => "\u{1F7E2}",
        }
    }
}

/// Uma linha da fila "exige ação hoje".
#[must_use]
pub struct LinhaDeAcao {
    severidade: Severidade,
    titulo: String,
    subtitulo: Option<String>,
    valor: Option<Dinheiro>,
    rotulo_acao: String,
}

impl LinhaDeAcao {
    /// Uma linha de ação sem subtítulo nem valor.
    pub fn novo(
        severidade: Severidade,
        titulo: impl Into<String>,
        rotulo_acao: impl Into<String>,
    ) -> Self {
        Self {
            severidade,
            titulo: titulo.into(),
            subtitulo: None,
            valor: None,
            rotulo_acao: rotulo_acao.into(),
        }
    }

    /// Segunda linha, em `texto_medio`.
    pub fn subtitulo(mut self, subtitulo: impl Into<String>) -> Self {
        self.subtitulo = Some(subtitulo.into());
        self
    }

    /// Mostra um valor monetário ao lado do título.
    pub const fn valor(mut self, valor: Dinheiro) -> Self {
        self.valor = Some(valor);
        self
    }

    /// Desenha a linha; devolve verdadeiro se o botão de ação foi clicado.
    pub fn mostrar(self, ui: &mut Ui) -> bool {
        let cores = ui.cores();
        let mut clicado = false;
        egui::Frame::none()
            .fill(cores.superficie)
            .stroke(egui::Stroke::new(1.0_f32, cores.borda))
            .rounding(Raio::CARTAO)
            .inner_margin(Espaco::E12)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width().max(0.0));
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(self.severidade.glifo()).size(13.0_f32));
                    ui.add_space(Espaco::E8);
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.add(Rotulo::interface(self.titulo).cor(self.severidade.cor(&cores)));
                            if let Some(valor) = self.valor {
                                ui.add(ValorDinheiro::novo(valor));
                            }
                        });
                        if let Some(subtitulo) = self.subtitulo {
                            ui.add(Rotulo::campo(subtitulo));
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(Botao::secundario(self.rotulo_acao)).clicked() {
                            clicado = true;
                        }
                    });
                });
            });
        clicado
    }
}
