//! Camada 3 (organisms) — `Painel`: o contêiner padrão de conteúdo — moldura, cabeçalho
//! opcional (título + ações à direita) e corpo.
//!
//! É a peça que o ADR-0015 mandou criar: até aqui, cada tela montava o seu próprio
//! `egui::Frame` (cor, raio, margem, sombra decididos fora dos tokens). O `Painel` fecha
//! isso — a aparência de "um bloco de conteúdo" tem um único dono.
//!
//! Três aparências:
//! - **elevado** (padrão): superfície + borda + sombra mínima — um cartão em repouso;
//! - **plano**: sem sombra — bloco dentro de outro bloco, ou lista densa;
//! - **realce(`Tom`)**: fundo `*_suave` e borda no tom — avisos, resultado de venda,
//!   "caixa fechado". O tom nunca substitui o texto (§9: nenhuma informação só por cor).

use egui::{Stroke, Ui};

use crate::atoms::{Rotulo, Tom};
use crate::tokens::{sombra_cartao, Espaco, Raio, TemaUi};

/// Um bloco de conteúdo com moldura.
#[must_use]
pub struct Painel {
    titulo: Option<String>,
    tom: Option<Tom>,
    elevado: bool,
    margem: f32,
}

impl Painel {
    /// Um painel elevado, sem título, com margem confortável (16px).
    pub const fn novo() -> Self {
        Self {
            titulo: None,
            tom: None,
            elevado: true,
            margem: Espaco::E16,
        }
    }

    /// Título do cabeçalho (papel `TituloSecao`).
    pub fn titulo(mut self, titulo: impl Into<String>) -> Self {
        self.titulo = Some(titulo.into());
        self
    }

    /// Sem sombra: para blocos dentro de outros blocos.
    pub const fn plano(mut self) -> Self {
        self.elevado = false;
        self
    }

    /// Margem interna menor (12px) — listas densas, painéis laterais.
    pub const fn compacto(mut self) -> Self {
        self.margem = Espaco::E12;
        self
    }

    /// Realça o painel num tom semântico (fundo suave + borda no tom). Sem sombra.
    pub const fn realce(mut self, tom: Tom) -> Self {
        self.tom = Some(tom);
        self.elevado = false;
        self
    }

    /// Desenha o painel com o corpo dado. Devolve o que o corpo devolver.
    pub fn mostrar<R>(self, ui: &mut Ui, corpo: impl FnOnce(&mut Ui) -> R) -> R {
        self.mostrar_com_acoes(ui, |_| {}, corpo)
    }

    /// Desenha o painel com `acoes` alinhadas à direita do cabeçalho (botões, contadores).
    /// O cabeçalho só aparece se houver título.
    pub fn mostrar_com_acoes<R>(
        self,
        ui: &mut Ui,
        acoes: impl FnOnce(&mut Ui),
        corpo: impl FnOnce(&mut Ui) -> R,
    ) -> R {
        let cores = ui.cores();
        let (fundo, borda) = match self.tom {
            None => (cores.superficie, cores.borda),
            Some(tom) => {
                let (fg, bg) = tom.cores(&cores);
                (bg, fg.gamma_multiply(0.35_f32))
            }
        };
        let mut moldura = egui::Frame::none()
            .fill(fundo)
            .stroke(Stroke::new(1.0_f32, borda))
            .rounding(Raio::CARTAO)
            .inner_margin(self.margem);
        if self.elevado {
            moldura = moldura.shadow(sombra_cartao(ui.ctx()));
        }
        moldura
            .show(ui, |ui| {
                // Ocupa a largura toda: painéis lado a lado ficam alinhados.
                ui.set_width(ui.available_width().max(0.0));
                if let Some(titulo) = self.titulo {
                    ui.horizontal(|ui| {
                        ui.add(Rotulo::titulo_secao(titulo));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), acoes);
                    });
                    ui.add_space(Espaco::E12);
                }
                corpo(ui)
            })
            .inner
    }
}

impl Default for Painel {
    fn default() -> Self {
        Self::novo()
    }
}
