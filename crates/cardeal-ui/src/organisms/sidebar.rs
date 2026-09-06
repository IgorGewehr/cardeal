//! Camada 3 (organisms) — `Sidebar`. `docs/12-ui-ux.md` §5.
//!
//! Navegação **agrupada** (referência: `develop/gestao-raiz`): o primeiro grupo sem título,
//! os demais com cabeçalho em caixa alta + divisória. Cada entrada é uma
//! [`superficie_clicavel`](crate::atoms::superficie_clicavel) — a linha inteira responde ao
//! clique. Item ativo: fundo `rubro_ativo` (tint 10%) + barra `rubro` à esquerda + ícone e
//! texto na cor da marca.
//!
//! As entradas vêm de fora (do `Registro`/`Manifesto` dos módulos, nunca escritas à mão numa
//! tela — `docs/12-ui-ux.md` §5); este organismo só desenha e devolve o que foi clicado.

use cardeal_modkit::Icone;
use egui::Ui;

use crate::atoms::{altura_navegacao, desenhar_icone, superficie_clicavel, Rotulo};
use crate::tokens::{Espaco, Papel, TemaUi};

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

/// Um grupo de entradas — `titulo: None` para o primeiro grupo (sem cabeçalho).
pub struct GrupoSidebar<'a> {
    /// Cabeçalho do grupo, em caixa alta (`None` = sem cabeçalho).
    pub titulo: Option<&'a str>,
    /// As entradas do grupo.
    pub itens: &'a [ItemSidebar],
}

/// A sidebar retrátil. `expandida = false` mostra só os ícones.
pub struct Sidebar<'a> {
    grupos: &'a [GrupoSidebar<'a>],
    ativo: &'a str,
    expandida: bool,
}

impl<'a> Sidebar<'a> {
    /// Uma sidebar com os grupos dados, destacando o item `ativo`.
    pub const fn nova(grupos: &'a [GrupoSidebar<'a>], ativo: &'a str, expandida: bool) -> Self {
        Self {
            grupos,
            ativo,
            expandida,
        }
    }

    /// Desenha a sidebar; devolve o id do item clicado, se algum foi.
    pub fn mostrar(self, ui: &mut Ui) -> Option<&'static str> {
        let cores = ui.cores();
        let mut clicado = None;

        ui.vertical(|ui| {
            for (gi, grupo) in self.grupos.iter().enumerate() {
                if let Some(titulo) = grupo.titulo {
                    if self.expandida {
                        ui.add_space(if gi == 0 { Espaco::E8 } else { Espaco::E16 });
                        ui.horizontal(|ui| {
                            let (tick, _) = ui.allocate_exact_size(
                                egui::vec2(Espaco::E12, 14.0),
                                egui::Sense::hover(),
                            );
                            let barra = egui::Rect::from_center_size(
                                egui::pos2(tick.left() + 4.5, tick.center().y),
                                egui::vec2(3.0, 11.0),
                            );
                            ui.painter()
                                .rect_filled(barra, 999.0, cores.rubro.gamma_multiply(0.55));
                            let espacado: String = titulo
                                .to_uppercase()
                                .chars()
                                .flat_map(|c| [c, '\u{2009}'])
                                .collect();
                            ui.label(
                                egui::RichText::new(espacado)
                                    .font(Papel::RotuloCampo.font_id())
                                    .color(cores.texto_medio)
                                    .size(10.5)
                                    .strong(),
                            );
                        });
                        ui.add_space(Espaco::E4);
                    } else if gi > 0 {
                        ui.add_space(Espaco::E8);
                        let sep = ui.available_rect_before_wrap();
                        let x = sep.center().x;
                        ui.painter().hline(
                            (x - 10.0)..=(x + 10.0),
                            sep.top(),
                            egui::Stroke::new(1.0_f32, cores.borda),
                        );
                        ui.add_space(Espaco::E8);
                    }
                }

                for item in grupo.itens {
                    let sel = item.id == self.ativo;
                    let cor = if sel { cores.rubro } else { cores.texto_medio };

                    let resp = superficie_clicavel(ui, sel, altura_navegacao(), |ui| {
                        desenhar_icone(ui, item.icone, 18.0, cor);
                        if self.expandida {
                            ui.add_space(Espaco::E12);
                            ui.add(Rotulo::interface(item.rotulo).cor(cor));
                            if let Some(n) = item.badge {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        badge(ui, n, cores.negativo);
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
            }
        });

        clicado
    }
}

fn badge(ui: &mut Ui, n: u32, cor: egui::Color32) {
    let texto = if n > 99 { "99+".to_owned() } else { n.to_string() };
    let galley = ui.painter().layout_no_wrap(
        texto,
        Papel::RotuloCampo.font_id(),
        egui::Color32::WHITE,
    );
    let w = galley.size().x.max(10.0) + 10.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 18.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 9.0, cor);
    ui.painter().galley(
        rect.center() - galley.size() / 2.0,
        galley,
        egui::Color32::WHITE,
    );
}
