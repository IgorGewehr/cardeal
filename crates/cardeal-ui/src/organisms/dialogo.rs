//! Camada 3 (organisms) — `Dialogo`: o modal padrão de toda tela de módulo.
//!
//! `docs/12-ui-ux.md` §7 e a regra de UI do projeto: lista sempre visível; criar/ver/editar
//! um item acontece **num dialog**, nunca inline. Um só componente serve os três modos —
//! o corpo é o mesmo conjunto de campos, só muda se estão editáveis.
//!
//! Sem `egui::Modal` (não existe no egui 0.29): montado à mão — um backdrop que come o
//! clique + um painel centrado, fecha no Esc / clique fora / no ✕.

use egui::{Align2, Color32, Context, Id, Order, Sense, Ui};

use crate::atoms::{Botao, Divisor, Etiqueta, Rotulo, Tom};
use crate::tokens::{Espaco, Raio, TemaUi};

/// Largura de um dialog de confirmação/entrada curta (1–2 campos).
const LARGURA_PEQUENA: f32 = 460.0;
/// Largura de um dialog de formulário médio.
const LARGURA_MEDIA: f32 = 620.0;
/// Largura padrão — formulário completo, duas colunas.
const LARGURA_GRANDE: f32 = 780.0;
/// Largura máxima do dialog amplo — um registro de trabalho (OS) com coluna lateral.
const LARGURA_AMPLA: f32 = 1280.0;
/// Altura do cabeçalho + rodapé do dialog, para o amplo calcular o corpo.
const ALTURA_MOLDURA: f32 = 150.0;

/// Um dialog modal centrado.
#[must_use]
pub struct Dialogo {
    titulo: String,
    descricao: Option<String>,
    largura: f32,
    amplo: bool,
    selo: Option<(String, Tom)>,
}

impl Dialogo {
    /// Um dialog com o título dado (largura padrão 780px, limitada à tela).
    pub fn nova(titulo: impl Into<String>) -> Self {
        Self {
            titulo: titulo.into(),
            descricao: None,
            largura: LARGURA_GRANDE,
            amplo: false,
            selo: None,
        }
    }

    /// Uma linha de contexto sob o título ("o que isto faz / o que vai acontecer") — quem
    /// abre um dialog deve saber o que ele pede sem ler os campos.
    pub fn descricao(mut self, texto: impl Into<String>) -> Self {
        self.descricao = Some(texto.into());
        self
    }

    /// 460px — confirmação e entrada curta (1–2 campos).
    pub const fn pequeno(mut self) -> Self {
        self.largura = LARGURA_PEQUENA;
        self
    }

    /// 620px — formulário médio.
    pub const fn medio(mut self) -> Self {
        self.largura = LARGURA_MEDIA;
        self
    }

    /// Amplo e deitado (paisagem): até 1280px de largura, altura **fixa** sempre menor que a
    /// largura — o dialog não pula de tamanho quando o conteúdo muda. Para registros de
    /// trabalho com muita coisa (uma OS). O corpo **não rola sozinho** e vem sem margem: monte-o
    /// com [`ColunasDialogo`](super::ColunasDialogo), que dá a cada coluna a sua rolagem.
    pub const fn amplo(mut self) -> Self {
        self.amplo = true;
        self.largura = LARGURA_AMPLA;
        self
    }

    /// Uma etiqueta de estado ao lado do título (ex.: "Aberta", "Faturada").
    pub fn selo(mut self, texto: impl Into<String>, tom: Tom) -> Self {
        self.selo = Some((texto.into(), tom));
        self
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
        crate::tokens::marcar_modal(ctx);
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

        // Aproveita a largura; ocupa bem menos que a altura total.
        let largura = if self.amplo {
            self.largura.min(tela.width() * 0.92).max(120.0)
        } else {
            self.largura.min(tela.width() * 0.9).max(120.0)
        };
        let altura_corpo_max = (tela.height() * 0.62).clamp(200.0, 540.0);
        // Amplo: altura total limitada a 86% da tela e a 62% da largura (sempre deitado).
        let altura_corpo_ampla =
            ((tela.height() * 0.86).min(largura * 0.62) - ALTURA_MOLDURA).max(200.0);

        egui::Area::new(Id::new(("dialogo", &self.titulo)))
            .order(Order::Foreground)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_max_width(largura);
                // Moldura sem margem própria: cabeçalho, corpo e rodapé cuidam do respiro,
                // para o rodapé poder ser uma faixa que vai de borda a borda.
                egui::Frame::none()
                    .fill(cores.superficie)
                    .stroke(egui::Stroke::new(1.0_f32, cores.borda))
                    .rounding(Raio::MODAL)
                    .shadow(crate::tokens::sombra_dropdown(ctx))
                    .inner_margin(0.0)
                    .show(ui, |ui| {
                        ui.set_width(largura);

                        // Cabeçalho: título (neutro — o vermelho da marca é ação, não
                        // decoração), descrição opcional e ✕.
                        egui::Frame::none()
                            .inner_margin(egui::Margin::symmetric(Espaco::E24, Espaco::E16))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width().max(0.0));
                                ui.horizontal(|ui| {
                                    ui.vertical(|ui| {
                                        // Sem selo, o título fica direto na coluna: um
                                        // `horizontal` a mais muda os ids do dialog inteiro
                                        // (e o foco de campos, como o valor no Pagamento).
                                        match &self.selo {
                                            Some((texto, tom)) => {
                                                ui.horizontal(|ui| {
                                                    ui.add(Rotulo::titulo_secao(
                                                        self.titulo.clone(),
                                                    ));
                                                    ui.add_space(Espaco::E4);
                                                    ui.add(Etiqueta::nova(texto.clone(), *tom));
                                                });
                                            }
                                            None => {
                                                ui.add(Rotulo::titulo_secao(self.titulo.clone()));
                                            }
                                        }
                                        if let Some(d) = &self.descricao {
                                            ui.add_space(Espaco::E4);
                                            ui.add(Rotulo::apoio(d.clone()));
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

                        // Corpo: rolável, ou a área fixa do amplo (sem margem nem rolagem).
                        let altura = if self.amplo {
                            Some(altura_corpo_ampla)
                        } else {
                            None
                        };
                        desenhar_corpo(ui, altura, altura_corpo_max, |ui| corpo(ui, estado));

                        // Rodapé: faixa `superficie_2` de borda a borda, cantos de baixo
                        // acompanhando o raio do modal.
                        egui::Frame::none()
                            .fill(cores.superficie_2)
                            .rounding(egui::Rounding {
                                nw: 0.0,
                                ne: 0.0,
                                sw: Raio::MODAL,
                                se: Raio::MODAL,
                            })
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

        fechar
    }
}

/// O corpo do dialog. `fixo = Some(altura)` (amplo): uma área dessa altura, sem margem nem
/// rolagem, para o chamador dividir. `None`: margem padrão e rolagem até `altura_max`.
fn desenhar_corpo(ui: &mut Ui, fixo: Option<f32>, altura_max: f32, corpo: impl FnOnce(&mut Ui)) {
    if let Some(altura) = fixo {
        let (_, area) = ui.allocate_space(egui::vec2(ui.available_width().max(0.0), altura));
        let mut filho = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(area)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        filho.set_clip_rect(area.intersect(ui.clip_rect()));
        corpo(&mut filho);
        return;
    }
    egui::Frame::none()
        .inner_margin(egui::Margin::symmetric(Espaco::E24, Espaco::E16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width().max(0.0));
            egui::ScrollArea::vertical()
                .max_height(altura_max)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.set_width(ui.available_width().max(0.0));
                    corpo(ui);
                });
        });
}
