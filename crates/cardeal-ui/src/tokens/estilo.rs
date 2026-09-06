//! Camada 0 (ions) — a ponte entre os tokens Rubro e o `egui::Style`.
//!
//! `docs/12-ui-ux.md` §10: "não há CSS". Sem esta ponte, os widgets crus do `egui`
//! (`TextEdit`, `ComboBox`, `SelectableLabel`, `Separator`) seguiriam a paleta de fábrica do
//! `egui` enquanto os componentes do design system seguiriam `Cores` — os dois sistemas
//! brigando é a causa do visual "sem acabamento".
//!
//! [`instalar_estilo`] resolve isso de uma vez: traduz um [`Tema`] inteiro para
//! `egui::Style` (widgets, seleção, foco, espaçamento, arredondamento, tipografia base) e
//! guarda o [`Tema`] em `ctx.data`, de onde [`TemaUi`] o lê em qualquer `Ui`.
//!
//! Chame uma vez no boot e de novo só quando o tema muda — **nunca todo frame**.

use egui::{Color32, Context, FontFamily, FontId, Id, Margin, Rounding, Stroke, TextStyle, Ui, Vec2};

use super::cores::Rubro;
use super::cores::{Cores, Tema};
use super::espacamento::{Espaco, Raio};

const CHAVE_TEMA: &str = "cardeal:tema";

/// Instala o [`Tema`] no contexto: guarda-o em `ctx.data` e reescreve o `egui::Style` para
/// que todo widget — do design system ou cru — saia na identidade Rubro.
pub fn instalar_estilo(ctx: &Context, tema: Tema) {
    ctx.data_mut(|d| d.insert_temp(Id::new(CHAVE_TEMA), tema));

    let c = tema.cores();
    let mut style = (*ctx.style()).clone();

    // ---- tipografia base (os componentes usam `Papel` direto; isto é para o egui cru) ----
    style.text_styles = [
        (TextStyle::Small, FontId::new(12.0_f32, FontFamily::Proportional)),
        (TextStyle::Body, FontId::new(13.0_f32, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(13.0_f32, FontFamily::Proportional)),
        (TextStyle::Heading, FontId::new(20.0_f32, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(12.0_f32, FontFamily::Monospace)),
    ]
    .into();

    // ---- espaçamento e forma ----
    let sp = &mut style.spacing;
    sp.item_spacing = Vec2::new(Espaco::E8, Espaco::E8);
    sp.button_padding = Vec2::new(Espaco::E12, Espaco::E8);
    sp.menu_margin = Margin::same(Espaco::E4);
    sp.window_margin = Margin::same(Espaco::E16);
    sp.interact_size.y = 32.0_f32;
    sp.icon_width = 18.0_f32;
    sp.icon_width_inner = 10.0_f32;

    // ---- cores ----
    let v = &mut style.visuals;
    v.dark_mode = tema.e_escuro();
    v.override_text_color = Some(c.texto);
    v.window_fill = c.fundo;
    v.panel_fill = c.fundo;
    v.faint_bg_color = c.superficie_2;
    v.extreme_bg_color = c.superficie;
    v.window_stroke = Stroke::new(1.0_f32, c.borda);
    v.window_rounding = Rounding::same(Raio::MODAL);
    v.menu_rounding = Rounding::same(Raio::CARTAO);
    v.popup_shadow = sombra(tema, 1.0_f32, 3.0_f32, 90, 16);
    v.window_shadow = sombra(tema, 4.0_f32, 14.0_f32, 120, 26);
    v.hyperlink_color = if tema.e_escuro() { Rubro::R400 } else { Rubro::R600 };
    v.error_fg_color = c.negativo;
    v.warn_fg_color = c.atencao;

    let r: Rounding = Raio::CAMPO.into();

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = c.fundo;
    w.noninteractive.weak_bg_fill = c.fundo;
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, c.borda);
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, c.texto_medio);
    w.noninteractive.rounding = r;

    w.inactive.bg_fill = c.superficie;
    w.inactive.weak_bg_fill = c.superficie;
    w.inactive.bg_stroke = Stroke::new(1.0_f32, c.borda_forte);
    w.inactive.fg_stroke = Stroke::new(1.0_f32, c.texto);
    w.inactive.rounding = r;
    w.inactive.expansion = 0.0_f32;

    w.hovered.bg_fill = c.superficie_2;
    w.hovered.weak_bg_fill = c.superficie_2;
    w.hovered.bg_stroke = Stroke::new(1.0_f32, c.borda_forte);
    w.hovered.fg_stroke = Stroke::new(1.5_f32, c.texto_forte);
    w.hovered.rounding = r;
    w.hovered.expansion = 0.0_f32;

    w.active.bg_fill = c.superficie_2;
    w.active.weak_bg_fill = c.superficie_2;
    w.active.bg_stroke = Stroke::new(2.0_f32, Rubro::R500);
    w.active.fg_stroke = Stroke::new(2.0_f32, c.texto_forte);
    w.active.rounding = r;
    w.active.expansion = 0.0_f32;

    w.open.bg_fill = c.superficie;
    w.open.weak_bg_fill = c.superficie;
    w.open.bg_stroke = Stroke::new(1.0_f32, c.borda_forte);
    w.open.fg_stroke = Stroke::new(1.0_f32, c.texto);
    w.open.rounding = r;

    // Seleção de texto e de listas: rubro translúcido, nunca o azul de fábrica.
    let alfa_sel = if tema.e_escuro() { 0.32_f32 } else { 0.22_f32 };
    v.selection.bg_fill = Rubro::R500.gamma_multiply(alfa_sel);
    v.selection.stroke = Stroke::new(
        1.0_f32,
        if tema.e_escuro() { Rubro::R400 } else { Rubro::R600 },
    );

    // Foco sempre visível (`docs/12-ui-ux.md` §9).
    style.visuals.clip_rect_margin = 3.0_f32;

    ctx.set_style(style);
}

fn sombra(tema: Tema, offset_y: f32, blur: f32, alfa_escuro: u8, alfa_claro: u8) -> egui::epaint::Shadow {
    egui::epaint::Shadow {
        offset: Vec2::new(0.0_f32, offset_y),
        blur,
        spread: 0.0_f32,
        color: Color32::from_black_alpha(if tema.e_escuro() { alfa_escuro } else { alfa_claro }),
    }
}

/// A sombra sutil de um cartão em repouso (`--shadow-card` do gestao-raiz).
#[must_use]
pub fn sombra_cartao(ctx: &Context) -> egui::epaint::Shadow {
    sombra(ctx.tema(), 1.0_f32, 6.0_f32, 70, 22)
}

/// Acesso ao tema ativo a partir de qualquer `Ui` — o caminho normal para um componente
/// descobrir a paleta, sem `&Cores` na assinatura.
pub trait TemaUi {
    /// O tema ativo (`Tema::default()` se [`instalar_estilo`] ainda não rodou).
    fn tema(&self) -> Tema;
    /// As cores do tema ativo — atalho para `self.tema().cores()`.
    fn cores(&self) -> Cores;
}

impl TemaUi for Ui {
    fn tema(&self) -> Tema {
        self.ctx()
            .data(|d| d.get_temp::<Tema>(Id::new(CHAVE_TEMA)))
            .unwrap_or_default()
    }

    fn cores(&self) -> Cores {
        self.tema().cores()
    }
}

impl TemaUi for Context {
    fn tema(&self) -> Tema {
        self.data(|d| d.get_temp::<Tema>(Id::new(CHAVE_TEMA)))
            .unwrap_or_default()
    }

    fn cores(&self) -> Cores {
        self.tema().cores()
    }
}
