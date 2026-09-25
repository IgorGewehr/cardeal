//! Camada 3 (organisms) — `Gaveta`: um painel que desliza da direita, com a lista ainda
//! visível atrás. Para registros que são um **processo** (uma OS dura dias e passa por vários
//! estados): o balcão abre, age e volta à fila sem perder o contexto, coisa que o `Dialogo`
//! centrado, tapando a tela, não permite.
//!
//! Mesmo contrato do [`Dialogo`](super::Dialogo): `corpo` rola, `rodape` fica fixo embaixo
//! (botões da direita para a esquerda) e `mostrar` devolve `true` quando deve fechar (Esc,
//! clique fora, ✕).

use egui::{Align2, Color32, Context, Id, Order, Sense, Ui};

use crate::atoms::{Botao, Divisor, Rotulo};
use crate::tokens::{Espaco, TemaUi};

/// Largura padrão: cabe um formulário de duas colunas e sobra a lista à esquerda.
const LARGURA_PADRAO: f32 = 640.0;
/// Quanto do topo fica livre para a barra de título da janela.
const MARGEM_TOPO: f32 = 40.0;
/// Duração da entrada deslizando, em segundos.
const DURACAO_ENTRADA: f32 = 0.18;

/// Um painel lateral à direita.
#[must_use]
pub struct Gaveta {
    titulo: String,
    subtitulo: Option<String>,
    largura: f32,
}

impl Gaveta {
    /// Uma gaveta com o título dado.
    pub fn nova(titulo: impl Into<String>) -> Self {
        Self {
            titulo: titulo.into(),
            subtitulo: None,
            largura: LARGURA_PADRAO,
        }
    }

    /// Uma linha sob o título (ex.: o cliente e o estado).
    pub fn subtitulo(mut self, texto: impl Into<String>) -> Self {
        self.subtitulo = Some(texto.into());
        self
    }

    /// Sobrescreve a largura (limitada a 90% da tela).
    pub const fn largura(mut self, px: f32) -> Self {
        self.largura = px;
        self
    }

    /// Desenha a gaveta. Devolve `true` quando ela deve fechar.
    pub fn mostrar<T>(
        self,
        ctx: &Context,
        estado: &mut T,
        corpo: impl FnOnce(&mut Ui, &mut T),
        rodape: impl FnOnce(&mut Ui, &mut T),
    ) -> bool {
        crate::tokens::marcar_modal(ctx);
        let cores = ctx.cores();
        let tela = ctx.screen_rect();
        let mut fechar = ctx.input(|i| i.key_pressed(egui::Key::Escape));

        // Véu leve: a lista continua legível atrás, mas o clique fora fecha.
        egui::Area::new(Id::new("gaveta-veu"))
            .order(Order::Middle)
            .fixed_pos(tela.min)
            .show(ctx, |ui| {
                let r = ui.allocate_rect(tela, Sense::click());
                ui.painter()
                    .rect_filled(tela, 0.0, Color32::from_black_alpha(60));
                if r.clicked() {
                    fechar = true;
                }
            });

        let largura = self.largura.min(tela.width() * 0.9).max(320.0);
        let altura = (tela.height() - MARGEM_TOPO).max(200.0);
        // Entra deslizando da borda: 0 → 1 na primeira aparição desta gaveta.
        let t = ctx.animate_bool_with_time(
            Id::new(("gaveta-entrada", &self.titulo)),
            true,
            DURACAO_ENTRADA,
        );
        let deslocamento = (1.0 - t) * largura * 0.25;

        egui::Area::new(Id::new("gaveta"))
            .order(Order::Foreground)
            // Sem o "empurrar para caber na tela" do egui: a gaveta começa sempre abaixo da
            // barra de título (os botões da janela continuam clicáveis) e a altura já é a que
            // sobra dela.
            .constrain(false)
            .anchor(Align2::RIGHT_TOP, [deslocamento, MARGEM_TOPO])
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(cores.superficie)
                    .stroke(egui::Stroke::new(1.0_f32, cores.borda))
                    .shadow(crate::tokens::sombra_dropdown(ctx))
                    .show(ui, |ui| {
                        ui.set_width(largura);
                        ui.set_height(altura);
                        ui.vertical(|ui| {
                            egui::Frame::none()
                                .inner_margin(egui::Margin::symmetric(Espaco::E24, Espaco::E16))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width().max(0.0));
                                    ui.horizontal(|ui| {
                                        ui.vertical(|ui| {
                                            ui.add(Rotulo::titulo_secao(self.titulo.clone()));
                                            if let Some(s) = &self.subtitulo {
                                                ui.add_space(Espaco::E4);
                                                ui.add(Rotulo::campo(s.clone()).quebravel());
                                            }
                                        });
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Min),
                                            |ui| {
                                                if ui
                                                    .add(Botao::fantasma("\u{00D7}").pequeno())
                                                    .clicked()
                                                {
                                                    fechar = true;
                                                }
                                            },
                                        );
                                    });
                                });
                            ui.add(Divisor::novo());

                            // O rodapé é desenhado de baixo para cima, então o corpo usa só o
                            // que sobra acima dele.
                            let altura_rodape = 72.0;
                            let altura_corpo = (ui.available_height() - altura_rodape).max(80.0);
                            egui::Frame::none()
                                .inner_margin(egui::Margin::symmetric(Espaco::E24, Espaco::E16))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width().max(0.0));
                                    egui::ScrollArea::vertical()
                                        .max_height(altura_corpo - Espaco::E16 * 2.0)
                                        .auto_shrink([false, false])
                                        .show(ui, |ui| {
                                            ui.set_width(ui.available_width().max(0.0));
                                            corpo(ui, estado);
                                        });
                                });

                            egui::Frame::none()
                                .fill(cores.superficie_2)
                                .inner_margin(egui::Margin::symmetric(Espaco::E24, Espaco::E12))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width().max(0.0));
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| rodape(ui, estado),
                                    );
                                });
                        });
                    });
            });

        fechar
    }
}
