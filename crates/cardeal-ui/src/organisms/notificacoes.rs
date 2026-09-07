//! Camada 3 (organisms) — `Notificacoes`: o sistema de toast do Cardeal.
//!
//! Feedback flutuante para **erro, sucesso e carregamento**, ancorado no rodapé central e
//! desenhado em [`Order::Tooltip`] — acima de qualquer painel *e* de qualquer [`Dialogo`]
//! (que vive em [`Order::Foreground`]). O usuário sempre vê o retorno da ação.
//!
//! Não passa por assinatura de tela nenhuma: a fila mora em `ctx.data`, como o [`Tema`]
//! (`tokens::estilo`). Qualquer código com um `&Context` empurra um toast:
//!
//! ```ignore
//! notificar(ui.ctx(), Notificacao::sucesso("Cliente cadastrado"));
//! notificar(ui.ctx(), Notificacao::erro(e.mensagem));
//! ```
//!
//! e `App::update` chama [`Notificacoes::mostrar`] uma vez por quadro, por último.
//!
//! [`Order::Foreground`]: egui::Order::Foreground
//! [`Order::Tooltip`]: egui::Order::Tooltip
//! [`Dialogo`]: crate::organisms::Dialogo
//! [`Tema`]: crate::tokens::Tema

// Frações de tempo/progresso perdem precisão ao virar `f32` de propósito.
#![allow(clippy::cast_possible_truncation)]

use std::sync::atomic::{AtomicU64, Ordering};

use egui::{
    vec2, Align2, Color32, Context, Id, Order, Pos2, Rect, Sense, Stroke, Ui, UiBuilder, Vec2,
};

use crate::atoms::{Rotulo, Spinner};
use crate::tokens::{sombra_dropdown, Espaco, Raio, TemaUi};

/// O caráter de uma [`Notificacao`] — define cor, ícone e duração.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tom {
    /// Ação concluída (verde, ✓). Some sozinho em ~4s.
    Sucesso,
    /// Ação falhou (rubro, !). Fica até o usuário fechar.
    Erro,
    /// Atenção sem falha (âmbar, !). Some em ~6s.
    Aviso,
    /// Informação neutra (azul, i). Some em ~4s.
    Info,
    /// Operação em andamento (spinner). Fica até ser substituída pelo resultado.
    Carregando,
}

impl Tom {
    /// Segundos até o toast começar a sair sozinho — `None` = fica até fechar/substituir.
    const fn duracao(self) -> Option<f64> {
        match self {
            Self::Sucesso | Self::Info => Some(4.0),
            Self::Aviso => Some(6.0),
            Self::Erro | Self::Carregando => None,
        }
    }

    fn cor(self, ui: &Ui) -> Color32 {
        let c = ui.cores();
        match self {
            Self::Sucesso => c.positivo,
            Self::Erro => c.negativo,
            Self::Aviso => c.atencao,
            Self::Info => c.info,
            Self::Carregando => c.rubro,
        }
    }
}

/// Um aviso a ser exibido. Construa por tom e empurre com [`notificar`].
#[derive(Debug, Clone)]
pub struct Notificacao {
    id: Id,
    tom: Tom,
    titulo: String,
    detalhe: Option<String>,
}

fn id_novo() -> Id {
    static N: AtomicU64 = AtomicU64::new(0);
    Id::new(("cardeal-toast", N.fetch_add(1, Ordering::Relaxed)))
}

impl Notificacao {
    fn nova(tom: Tom, titulo: impl Into<String>) -> Self {
        Self {
            id: id_novo(),
            tom,
            titulo: titulo.into(),
            detalhe: None,
        }
    }

    /// Sucesso — "Cliente cadastrado", "Venda concluída".
    pub fn sucesso(titulo: impl Into<String>) -> Self {
        Self::nova(Tom::Sucesso, titulo)
    }

    /// Erro — a mensagem de um `Erro` de domínio.
    pub fn erro(titulo: impl Into<String>) -> Self {
        Self::nova(Tom::Erro, titulo)
    }

    /// Aviso — algo a conferir, sem falha.
    pub fn aviso(titulo: impl Into<String>) -> Self {
        Self::nova(Tom::Aviso, titulo)
    }

    /// Informação neutra.
    pub fn info(titulo: impl Into<String>) -> Self {
        Self::nova(Tom::Info, titulo)
    }

    /// Operação em andamento — normalmente com um `id` estável para depois ser substituída
    /// pelo resultado.
    pub fn carregando(titulo: impl Into<String>) -> Self {
        Self::nova(Tom::Carregando, titulo)
    }

    /// Segunda linha, menor, quebrável.
    #[must_use]
    pub fn detalhe(mut self, d: impl Into<String>) -> Self {
        self.detalhe = Some(d.into());
        self
    }

    /// Fixa o `id` do toast. Um novo [`notificar`] com o mesmo `id` **substitui** o toast no
    /// lugar (troca texto/tom, reinicia o tempo) — é assim que "Emitindo NFC-e…" vira
    /// "NFC-e autorizada".
    #[must_use]
    pub fn id(mut self, id: Id) -> Self {
        self.id = id;
        self
    }
}

#[derive(Clone)]
struct Ativa {
    n: Notificacao,
    criada: f64,
    expira: Option<f64>,
    saindo: bool,
}

#[derive(Clone, Default)]
struct Fila(Vec<Ativa>);

const MAX_VISIVEIS: usize = 4;
/// Duração da animação de entrada/saída, em segundos.
const ANIM: f32 = 0.18;

fn chave() -> Id {
    Id::new("cardeal-notificacoes-fila")
}

/// Empurra um toast na fila global. Se já houver um com o mesmo `id`, substitui no lugar.
pub fn notificar(ctx: &Context, n: Notificacao) {
    let agora = ctx.input(|i| i.time);
    ctx.data_mut(|d| {
        let fila = d.get_temp_mut_or_default::<Fila>(chave());
        let expira = n.tom.duracao().map(|s| agora + s);
        if let Some(a) = fila.0.iter_mut().find(|a| a.n.id == n.id) {
            a.n = n;
            a.criada = agora;
            a.expira = expira;
            a.saindo = false;
        } else {
            fila.0.push(Ativa {
                n,
                criada: agora,
                expira,
                saindo: false,
            });
            let vivos = fila.0.iter().filter(|a| !a.saindo).count();
            if vivos > MAX_VISIVEIS {
                if let Some(velho) = fila.0.iter_mut().find(|a| !a.saindo) {
                    velho.saindo = true;
                }
            }
        }
    });
    ctx.request_repaint();
}

/// O ponto de entrada de desenho — chame uma vez, por último, em `App::update`.
pub struct Notificacoes;

impl Notificacoes {
    /// Desenha a pilha de toasts e avança as animações/temporizadores.
    pub fn mostrar(ctx: &Context) {
        let mut fila = ctx.data(|d| d.get_temp::<Fila>(chave()).unwrap_or_default());
        if fila.0.is_empty() {
            return;
        }

        let agora = ctx.input(|i| i.time);
        for a in &mut fila.0 {
            if a.expira.is_some_and(|e| agora >= e) {
                a.saindo = true;
            }
        }

        let largura = (ctx.screen_rect().width() * 0.9).clamp(120.0, 420.0);
        let mut dispensar: Vec<Id> = Vec::new();

        egui::Area::new(chave())
            .order(Order::Tooltip)
            .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -Espaco::E24))
            .interactable(true)
            .show(ctx, |ui| {
                ui.set_width(largura);
                for (idx, a) in fila.0.iter().enumerate() {
                    let t = ui.ctx().animate_bool_with_time(a.n.id, !a.saindo, ANIM);
                    if t <= 0.01 {
                        continue;
                    }
                    if idx > 0 {
                        ui.add_space(Espaco::E8);
                    }
                    ui.scope(|ui| {
                        ui.set_opacity(t);
                        ui.add_space(10.0 * (1.0 - t));
                        if card(ui, a, agora, largura) {
                            dispensar.push(a.n.id);
                        }
                    });
                }
            });

        for id in dispensar {
            if let Some(a) = fila.0.iter_mut().find(|a| a.n.id == id) {
                a.saindo = true;
            }
        }
        fila.0.retain(|a| {
            let t = ctx.animate_bool_with_time(a.n.id, !a.saindo, ANIM);
            !(a.saindo && t <= 0.01)
        });

        ctx.request_repaint();
        ctx.data_mut(|d| d.insert_temp(chave(), fila));
    }
}

/// Desenha um card. Devolve `true` se o ✕ foi clicado.
fn card(ui: &mut Ui, a: &Ativa, agora: f64, largura: f32) -> bool {
    let cores = ui.cores();
    let cor_tom = a.n.tom.cor(ui);
    let mut fechar = false;

    let resposta = egui::Frame::none()
        .fill(cores.superficie)
        .stroke(Stroke::new(1.0_f32, cores.borda))
        .rounding(Raio::CARTAO)
        .shadow(sombra_dropdown(ui.ctx()))
        .inner_margin(egui::Margin::symmetric(14.0, 12.0))
        .show(ui, |ui| {
            ui.set_width(largura - 28.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = Espaco::E12;
                selo(ui, a.n.tom, cor_tom);
                ui.vertical(|ui| {
                    ui.add(Rotulo::interface(a.n.titulo.clone()).cor(cores.texto_forte).quebravel());
                    if let Some(d) = &a.n.detalhe {
                        ui.add_space(2.0);
                        ui.add(Rotulo::campo(d.clone()).quebravel());
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    if botao_fechar(ui, a.n.tom == Tom::Erro) {
                        fechar = true;
                    }
                });
            });

            // Barra de progresso do tempo restante (só quando some sozinho).
            if let (Some(exp), false) = (a.expira, a.saindo) {
                let dur = (exp - a.criada).max(0.001);
                let frac = (((exp - agora) / dur).clamp(0.0, 1.0)) as f32;
                ui.add_space(Espaco::E8);
                let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 2.0), Sense::hover());
                ui.painter().rect_filled(rect, Raio::PILULA, cores.borda);
                let cheio = Rect::from_min_size(rect.min, vec2(rect.width() * frac, rect.height()));
                ui.painter().rect_filled(cheio, Raio::PILULA, cor_tom.gamma_multiply(0.7));
            }
        });

    // Fita de acento na borda esquerda do card.
    let r = resposta.response.rect;
    let fita = Rect::from_min_max(r.min, Pos2::new(r.min.x + 3.0, r.max.y));
    ui.painter().rect_filled(fita, Raio::PILULA, cor_tom);

    fechar
}

/// O selo redondo à esquerda: fundo do tom a baixo alfa + glifo (ou spinner).
fn selo(ui: &mut Ui, tom: Tom, cor: Color32) {
    let lado = 24.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(lado), Sense::hover());
    ui.painter()
        .circle_filled(rect.center(), lado / 2.0, cor.gamma_multiply(0.15));

    if tom == Tom::Carregando {
        let mut filho = ui.new_child(
            UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight)),
        );
        filho.add(Spinner::novo().pequeno().cor(cor));
        return;
    }

    let p = ui.painter();
    let t = Stroke::new(1.8_f32, cor);
    let c = rect.center();
    match tom {
        Tom::Sucesso => {
            p.line_segment([c + vec2(-4.5, 0.0), c + vec2(-1.0, 3.5)], t);
            p.line_segment([c + vec2(-1.0, 3.5), c + vec2(5.0, -4.0)], t);
        }
        Tom::Info => {
            p.circle_filled(c + vec2(0.0, -4.5), 1.4, cor);
            p.line_segment([c + vec2(0.0, -1.0), c + vec2(0.0, 5.0)], t);
        }
        // Erro e Aviso: exclamação.
        Tom::Erro | Tom::Aviso => {
            p.line_segment([c + vec2(0.0, -5.0), c + vec2(0.0, 2.0)], t);
            p.circle_filled(c + vec2(0.0, 5.0), 1.5, cor);
        }
        Tom::Carregando => {}
    }
}

/// Um ✕ pequeno e clicável. `sempre` = mostra mesmo sem hover (toasts de erro).
fn botao_fechar(ui: &mut Ui, sempre: bool) -> bool {
    let lado = 20.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(lado), Sense::click());
    let cores = ui.cores();
    let visivel = sempre || resp.hovered() || ui.rect_contains_pointer(rect);
    if ui.is_rect_visible(rect) && visivel {
        if resp.hovered() {
            ui.painter().rect_filled(rect, Raio::CAMPO, cores.superficie_hover);
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let c = rect.center();
        let cor = if resp.hovered() { cores.texto } else { cores.texto_fraco };
        let t = Stroke::new(1.4_f32, cor);
        ui.painter().line_segment([c + vec2(-3.5, -3.5), c + vec2(3.5, 3.5)], t);
        ui.painter().line_segment([c + vec2(3.5, -3.5), c + vec2(-3.5, 3.5)], t);
    }
    resp.clicked()
}
