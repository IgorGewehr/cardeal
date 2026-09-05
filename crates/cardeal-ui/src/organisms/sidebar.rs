//! Camada 3 (organisms) — `Sidebar`. `docs/12-ui-ux.md` §5: retrátil, indicador ativo em
//! barra `rubro-500` de 3px + fundo `rubro-50`, badges numéricos só quando exigem ação.
//!
//! Cada entrada é uma [`superficie_clicavel`](crate::atoms::superficie_clicavel) — a linha
//! inteira (ícone + rótulo + badge) é o alvo de clique, corrigindo o bug em que a metade
//! direita do rótulo não respondia.
//!
//! As entradas vêm de fora (do `ConjuntoEfetivo`/`Manifesto`, nunca escritas à mão numa
//! tela — `docs/12-ui-ux.md` §5); este organismo só desenha a lista e devolve o que foi
//! clicado.

use cardeal_modkit::Icone;
use egui::Ui;

use crate::atoms::{altura_navegacao, desenhar_icone, superficie_clicavel, Rotulo};
use crate::tokens::{Espaco, Rubro, TemaUi};

/// Uma entrada da sidebar.
pub struct ItemSidebar {
    /// Identidade estável da entrada (id do módulo/submódulo).
    pub id: &'static str,
    /// O ícone do módulo.
    pub icone: Icone,
    /// O rótulo mostrado quando a sidebar está expandida.
    pub rotulo: &'static str,
    /// Contagem de pendências — nunca decorativo (`docs/12-ui-ux.md` §5).
    pub badge: Option<u32>,
}

/// A sidebar retrátil. `expandida = false` mostra só os ícones.
pub struct Sidebar<'a> {
    itens: &'a [ItemSidebar],
    ativo: &'a str,
    expandida: bool,
}

impl<'a> Sidebar<'a> {
    /// Uma sidebar com as entradas dadas, destacando `ativo`.
    pub const fn nova(itens: &'a [ItemSidebar], ativo: &'a str, expandida: bool) -> Self {
        Self {
            itens,
            ativo,
            expandida,
        }
    }

    /// Desenha a sidebar; devolve o id do item clicado, se algum foi.
    pub fn mostrar(self, ui: &mut Ui) -> Option<&'static str> {
        let cores = ui.cores();
        let mut clicado = None;

        ui.vertical(|ui| {
            for item in self.itens {
                let selecionado = item.id == self.ativo;
                let cor = if selecionado { Rubro::R600 } else { cores.texto };

                let resp = superficie_clicavel(ui, selecionado, altura_navegacao(), |ui| {
                    desenhar_icone(ui, item.icone, 18.0, cor);
                    if self.expandida {
                        ui.add_space(Espaco::E12);
                        ui.add(Rotulo::interface(item.rotulo).cor(cor));
                        if let Some(n) = item.badge {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.add_space(Espaco::E12);
                                    ui.label(
                                        egui::RichText::new(n.to_string())
                                            .size(12.0_f32)
                                            .color(cores.negativo)
                                            .strong(),
                                    );
                                },
                            );
                        }
                    }
                });

                let resp = if self.expandida {
                    resp
                } else {
                    resp.on_hover_text(item.rotulo)
                };
                if resp.clicked() {
                    clicado = Some(item.id);
                }
                ui.add_space(2.0);
            }
        });

        clicado
    }
}
