//! Camada 0 (ions) — motion. `docs/12-ui-ux.md` §1.5 ("nada pisca, nada gira sem motivo")
//! e §5 (a sidebar anima 160ms e "volta a dormir").
//!
//! Animação no Cardeal existe para **explicar uma transição de estado** — um botão que
//! acende no hover, um campo que ganha foco, uma aba que troca — nunca para entreter. E
//! custa energia (Pilar I): por isso todo helper daqui é um fino invólucro sobre
//! `egui::Context::animate_*`, que pede repaint **só enquanto o valor está em movimento** e
//! deixa a janela dormir assim que a transição fecha.
//!
//! A linguagem de motion vem da referência (`~/develop/gestao-raiz`): transições curtas
//! (~150ms) com aceleração `cubic-bezier(.4, 0, .2, 1)` — rápidas no começo, suaves no fim.

use egui::{Color32, Id, Ui};

/// Durações de transição, em segundos. Escala curta de propósito — o olho percebe a
/// suavidade, não a espera.
pub struct Mov;

impl Mov {
    /// 90ms — realce de hover, press, troca de cor de borda. Quase instantâneo, só sem o
    /// "corte seco".
    pub const RAPIDO: f32 = 0.09;
    /// 150ms — o padrão (`--transition-fast` da referência): anel de foco, indicador de
    /// aba, giro do chevron.
    pub const PADRAO: f32 = 0.15;
    /// 220ms — movimento com deslocamento perceptível: barra de seleção crescendo, largura
    /// de painel.
    pub const CALMA: f32 = 0.22;
    /// 420ms — a contagem de um valor (dinheiro subindo até o número final).
    pub const CONTAGEM: f32 = 0.42;
}

/// A curva de aceleração do design system — `cubic-bezier(.4, 0, .2, 1)` aproximado
/// (arranque rápido, chegada suave). Recebe e devolve `t` em `0.0..=1.0`.
#[must_use]
pub fn suave(t: f32) -> f32 {
    egui::emath::easing::cubic_out(t.clamp(0.0, 1.0))
}

/// Transição suave de um booleano de estado (hover, foco, aberto, selecionado…). Devolve
/// `0.0..=1.0` já com o easing [`suave`] aplicado; pede repaint apenas enquanto anima.
///
/// `id` precisa ser estável entre quadros — normalmente `resp.id.with("hover")`.
pub fn ativar(ui: &Ui, id: Id, ligado: bool, dur: f32) -> f32 {
    ui.ctx()
        .animate_bool_with_time_and_easing(id, ligado, dur, egui::emath::easing::cubic_out)
}

/// Um valor `f32` perseguindo um alvo — contagem de números, largura animada, posição de um
/// indicador que desliza. Interpola linearmente no tempo dado; pede repaint só até chegar.
pub fn perseguir(ui: &Ui, id: Id, alvo: f32, dur: f32) -> f32 {
    ui.ctx().animate_value_with_time(id, alvo, dur)
}

/// Interpola duas cores em espaço linear (evita o cinza-lodo do meio do caminho que a
/// interpolação em sRGB puro produz). `t = 0.0` → `a`, `t = 1.0` → `b`.
#[must_use]
pub fn lerp_cor(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let a = egui::Rgba::from(a);
    let b = egui::Rgba::from(b);
    Color32::from(a * (1.0 - t) + b * t)
}
