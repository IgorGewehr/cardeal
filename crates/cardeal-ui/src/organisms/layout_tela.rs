//! Camada 3 (organisms) — `LayoutTela`: o esqueleto de toda tela de módulo.
//!
//! `docs/12-ui-ux.md` §2 (densidade honesta) e §7. Resolve as três queixas de layout de uma
//! vez:
//!
//! - **cabeçalho fixo** com título + ações (não rola com o conteúdo);
//! - **corpo rolável** (`ScrollArea`) — conteúdo maior que a janela nunca é cortado;
//! - **painel de detalhe responsivo**: lado a lado com a lista acima de ~900px de largura,
//!   empilhado abaixo disso (janela estreita, PDV em tela pequena).
//!
//! As seções recebem um `&mut T` (o estado da tela) em vez de capturá-lo — assim o mesmo
//! estado mutável pode alimentar cabeçalho, lista e detalhe sem o borrow-checker reclamar de
//! acesso duplo.

use egui::{ScrollArea, Ui};

use crate::atoms::Rotulo;
use crate::tokens::{Espaco, TemaUi};

/// Largura da janela abaixo da qual lista e detalhe se empilham em vez de ficar lado a lado.
const LIMIAR_COLUNAS: f32 = 900.0;

/// O esqueleto de uma tela: cabeçalho + corpo rolável, com ou sem painel de detalhe.
#[must_use]
pub struct LayoutTela {
    titulo: String,
    fracao_lista: f32,
}

impl LayoutTela {
    /// Uma tela com o título dado.
    pub fn nova(titulo: impl Into<String>) -> Self {
        Self {
            titulo: titulo.into(),
            fracao_lista: 0.58,
        }
    }

    /// Ajusta a fração da largura dada à lista quando há painel de detalhe (padrão 0.58).
    pub const fn fracao_lista(mut self, f: f32) -> Self {
        self.fracao_lista = f;
        self
    }

    /// Tela simples: cabeçalho + um corpo rolável.
    pub fn mostrar<T>(
        self,
        ui: &mut Ui,
        estado: &mut T,
        acoes: impl FnOnce(&mut Ui, &mut T),
        corpo: impl FnOnce(&mut Ui, &mut T),
    ) {
        cabecalho(ui, &self.titulo, |ui| acoes(ui, &mut *estado));
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_width(ui.available_width().max(0.0));
                corpo(ui, &mut *estado);
            });
    }

    /// Tela mestre-detalhe responsiva.
    pub fn mostrar_com_detalhe<T>(
        self,
        ui: &mut Ui,
        estado: &mut T,
        acoes: impl FnOnce(&mut Ui, &mut T),
        lista: impl FnOnce(&mut Ui, &mut T),
        detalhe: impl FnOnce(&mut Ui, &mut T),
    ) {
        let cores = ui.cores();
        cabecalho(ui, &self.titulo, |ui| acoes(ui, &mut *estado));

        let largura = ui.available_width();
        if largura >= LIMIAR_COLUNAS {
            let largura_lista = (largura * self.fracao_lista).max(320.0);
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(largura_lista, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(largura_lista);
                        ScrollArea::vertical()
                            .id_salt("lista")
                            .auto_shrink([false, false])
                            .show(ui, |ui| lista(ui, &mut *estado));
                    },
                );
                ui.add_space(Espaco::E16);
                ui.painter().vline(
                    ui.cursor().left(),
                    ui.clip_rect().y_range(),
                    egui::Stroke::new(1.0_f32, cores.borda),
                );
                ui.add_space(Espaco::E16);
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width().max(0.0));
                    ScrollArea::vertical()
                        .id_salt("detalhe")
                        .auto_shrink([false, false])
                        .show(ui, |ui| detalhe(ui, &mut *estado));
                });
            });
        } else {
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(ui.available_width().max(0.0));
                    lista(ui, &mut *estado);
                    ui.add_space(Espaco::E24);
                    ui.separator();
                    ui.add_space(Espaco::E16);
                    detalhe(ui, &mut *estado);
                });
        }
    }
}

fn cabecalho(ui: &mut Ui, titulo: &str, acoes: impl FnOnce(&mut Ui)) {
    let cores = ui.cores();
    ui.horizontal(|ui| {
        ui.add(Rotulo::titulo_tela(titulo.to_owned()));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), acoes);
    });
    ui.add_space(Espaco::E12);
    let linha = ui.available_rect_before_wrap();
    ui.painter().hline(
        linha.x_range(),
        linha.top(),
        egui::Stroke::new(1.0_f32, cores.borda),
    );
    ui.add_space(Espaco::E16);
}
