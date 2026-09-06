//! Camada 3 (organisms) — `Dialogo`: o modal padrão de toda tela de módulo.
//!
//! `docs/12-ui-ux.md` §7 e a regra de UI do projeto: lista sempre visível; criar/ver/editar
//! um item acontece **num dialog**, nunca inline. Um só componente serve os três modos —
//! o corpo é o mesmo conjunto de campos, só muda se estão editáveis.
//!
//! Sem `egui::Modal` (não existe no egui 0.29): montado à mão — um backdrop que come o
//! clique + um painel centrado, fecha no Esc / clique fora / no ✕.

use egui::{Align2, Color32, Context, Id, Order, Sense, Ui};

use crate::atoms::{Botao, Rotulo};
use crate::tokens::{Espaco, Raio, TemaUi};

/// Um dialog modal centrado.
#[must_use]
pub struct Dialogo {
    titulo: String,
    largura: f32,
}

impl Dialogo {
    /// Um dialog com o título dado (largura padrão 720px, limitada à tela).
    pub fn nova(titulo: impl Into<String>) -> Self {
        Self {
            titulo: titulo.into(),
            largura: 720.0,
        }
    }

    /// Sobrescreve a largura desejada.
    pub const fn largura(mut self, px: f32) -> Self {
        self.largura = px;
        self
    }

    /// Desenha o dialog. `corpo` monta os campos; `rodape` monta os botões (já alinhado da
    /// direita para a esquerda). Devolve `true` quando o dialog deve fechar (Esc, clique
    /// fora, ✕) — o chamador zera o seu `Option<EstadoDialogo>`.
    pub fn mostrar<T>(
        self,
        ctx: &Context,
        estado: &mut T,
        corpo: impl FnOnce(&mut Ui, &mut T),
        rodape: impl FnOnce(&mut Ui, &mut T),
    ) -> bool {
        let cores = ctx.cores();
        let tela = ctx.screen_rect();
        let mut fechar = ctx.input(|i| i.key_pressed(egui::Key::Escape));

        // Backdrop — acima dos painéis, escurece e intercepta o clique.
        egui::Area::new(Id::new(("dialogo-backdrop", &self.titulo)))
            .order(Order::Middle)
            .fixed_pos(tela.min)
            .show(ctx, |ui| {
                let r = ui.allocate_rect(tela, Sense::click());
                ui.painter()
                    .rect_filled(tela, 0.0, Color32::from_black_alpha(110));
                if r.clicked() {
                    fechar = true;
                }
            });

        let largura = self.largura.min(tela.width() - Espaco::E48);
        let altura_corpo_max = (tela.height() - 220.0).max(160.0);

        egui::Area::new(Id::new(("dialogo", &self.titulo)))
            .order(Order::Foreground)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_max_width(largura);
                egui::Frame::none()
                    .fill(cores.superficie)
                    .stroke(egui::Stroke::new(1.0_f32, cores.borda))
                    .rounding(Raio::MODAL)
                    .shadow(egui::epaint::Shadow {
                        offset: egui::vec2(0.0_f32, 12.0_f32),
                        blur: 40.0_f32,
                        spread: 0.0_f32,
                        color: Color32::from_black_alpha(60),
                    })
                    .inner_margin(Espaco::E24)
                    .show(ui, |ui| {
                        ui.set_width(largura - Espaco::E24 * 2.0);

                        ui.horizontal(|ui| {
                            ui.add(Rotulo::titulo_secao(self.titulo.clone()));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.add(Botao::fantasma("\u{2715}")).clicked() {
                                        fechar = true;
                                    }
                                },
                            );
                        });
                        ui.add_space(Espaco::E12);
                        ui.separator();
                        ui.add_space(Espaco::E16);

                        egui::ScrollArea::vertical()
                            .max_height(altura_corpo_max)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                corpo(ui, estado);
                            });

                        ui.add_space(Espaco::E16);
                        ui.separator();
                        ui.add_space(Espaco::E12);
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| rodape(ui, estado),
                        );
                    });
            });

        fechar
    }
}

