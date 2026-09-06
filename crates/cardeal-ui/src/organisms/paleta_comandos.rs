//! Camada 3 (organisms) — `PaletaComandos`: o `Ctrl+K` do `docs/12-ui-ux.md` §7/§8.
//!
//! Um campo de busca centrado no topo + lista filtrada de destinos e ações; setas navegam,
//! `Enter` executa, `Esc` fecha. O estado de navegação (item destacado) mora em `ui.memory`;
//! o chamador só cuida de `aberta` e do texto de busca.

use egui::{Align2, Color32, Context, Id, Key, Order, Sense};

use crate::atoms::Rotulo;
use crate::tokens::{Espaco, Raio, TemaUi};

/// Uma entrada da paleta.
pub struct ItemComando {
    /// Id estável devolvido quando o item é escolhido.
    pub id: &'static str,
    /// O rótulo mostrado.
    pub rotulo: &'static str,
    /// Um grupo/categoria curto à direita (ex.: "Ir para", "Ação").
    pub grupo: &'static str,
}

/// A paleta de comandos.
pub struct PaletaComandos<'a> {
    itens: &'a [ItemComando],
}

impl<'a> PaletaComandos<'a> {
    /// Uma paleta com as entradas dadas.
    #[must_use]
    pub const fn nova(itens: &'a [ItemComando]) -> Self {
        Self { itens }
    }

    /// Desenha a paleta se `aberta`. Devolve o id do comando escolhido (e fecha a paleta).
    #[allow(clippy::too_many_lines)]
    pub fn mostrar(
        self,
        ctx: &Context,
        aberta: &mut bool,
        busca: &mut String,
    ) -> Option<&'static str> {
        if !*aberta {
            return None;
        }
        let cores = ctx.cores();
        let tela = ctx.screen_rect();

        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            *aberta = false;
            busca.clear();
            return None;
        }

        // Filtragem simples por substring, sem acento-sensibilidade fina.
        let termo = busca.to_lowercase();
        let filtrados: Vec<&ItemComando> = self
            .itens
            .iter()
            .filter(|c| termo.is_empty() || c.rotulo.to_lowercase().contains(&termo))
            .collect();

        let id_sel = Id::new("paleta-comandos-sel");
        let mut sel: usize = ctx.data(|d| d.get_temp(id_sel)).unwrap_or(0);
        sel = sel.min(filtrados.len().saturating_sub(1));
        if ctx.input(|i| i.key_pressed(Key::ArrowDown)) {
            sel = (sel + 1).min(filtrados.len().saturating_sub(1));
        }
        if ctx.input(|i| i.key_pressed(Key::ArrowUp)) {
            sel = sel.saturating_sub(1);
        }

        let mut escolhido = None;

        // Backdrop.
        egui::Area::new(Id::new("paleta-backdrop"))
            .order(Order::Middle)
            .fixed_pos(tela.min)
            .show(ctx, |ui| {
                let r = ui.allocate_rect(tela, Sense::click());
                ui.painter()
                    .rect_filled(tela, 0.0, Color32::from_black_alpha(90));
                if r.clicked() {
                    *aberta = false;
                }
            });

        let largura = 560.0_f32.min(tela.width() - Espaco::E32).max(120.0);
        egui::Area::new(Id::new("paleta-comandos"))
            .order(Order::Foreground)
            .anchor(Align2::CENTER_TOP, [0.0, tela.height() * 0.14])
            .show(ctx, |ui| {
                ui.set_max_width(largura);
                egui::Frame::none()
                    .fill(cores.superficie)
                    .stroke(egui::Stroke::new(1.0_f32, cores.borda))
                    .rounding(Raio::MODAL)
                    .shadow(egui::epaint::Shadow {
                        offset: egui::vec2(0.0_f32, 14.0_f32),
                        blur: 44.0_f32,
                        spread: 0.0_f32,
                        color: Color32::from_black_alpha(70),
                    })
                    .inner_margin(Espaco::E12)
                    .show(ui, |ui| {
                        ui.set_width((largura - Espaco::E12 * 2.0).max(1.0));

                        let campo = ui.add(
                            egui::TextEdit::singleline(busca)
                                .hint_text("Ir para… ou uma ação")
                                .desired_width(f32::INFINITY)
                                .font(crate::tokens::Papel::Interface.font_id())
                                .margin(egui::Margin::symmetric(10.0_f32, 9.0_f32)),
                        );
                        campo.request_focus();
                        let enter = campo.lost_focus()
                            && ui.input(|i| i.key_pressed(Key::Enter));

                        ui.add_space(Espaco::E8);
                        egui::ScrollArea::vertical()
                            .max_height(tela.height() * 0.42)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width().max(0.0));
                                for (i, c) in filtrados.iter().enumerate() {
                                    let clic = crate::atoms::superficie_clicavel(
                                        ui,
                                        i == sel,
                                        36.0,
                                        |ui| {
                                            ui.add(Rotulo::interface(c.rotulo));
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    ui.add(Rotulo::campo(c.grupo));
                                                },
                                            );
                                        },
                                    )
                                    .clicked();
                                    if clic || (enter && i == sel) {
                                        escolhido = Some(c.id);
                                    }
                                }
                                if filtrados.is_empty() {
                                    ui.add(Rotulo::campo("Nada encontrado."));
                                }
                            });
                    });
            });

        ctx.data_mut(|d| d.insert_temp(id_sel, sel));

        if let Some(id) = escolhido {
            *aberta = false;
            busca.clear();
            ctx.data_mut(|d| d.insert_temp(id_sel, 0usize));
            return Some(id);
        }
        None
    }
}
