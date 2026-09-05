//! Camada 0 (ions) — espaçamento, raio e elevação. `docs/12-ui-ux.md` §4.

/// A escala de espaçamento, base 4px.
pub struct Espaco;

impl Espaco {
    /// Sem espaço.
    pub const E0: f32 = 0.0;
    /// 4px — a unidade base.
    pub const E4: f32 = 4.0;
    /// 8px.
    pub const E8: f32 = 8.0;
    /// 12px.
    pub const E12: f32 = 12.0;
    /// 16px.
    pub const E16: f32 = 16.0;
    /// 24px.
    pub const E24: f32 = 24.0;
    /// 32px.
    pub const E32: f32 = 32.0;
    /// 48px.
    pub const E48: f32 = 48.0;
    /// 64px.
    pub const E64: f32 = 64.0;
}

/// Raio de canto por papel do elemento.
pub struct Raio;

impl Raio {
    /// Campo, botão.
    pub const CAMPO: f32 = 4.0;
    /// Cartão.
    pub const CARTAO: f32 = 8.0;
    /// Modal.
    pub const MODAL: f32 = 14.0;
    /// Pílula / badge.
    pub const PILULA: f32 = 999.0;
}

/// Altura de linha de grade — configurável pelo usuário (`docs/12-ui-ux.md` §4).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum AlturaLinha {
    /// 28px.
    Compacta,
    /// 32px — padrão.
    #[default]
    Confortavel,
    /// 36px, para telas de toque (PDV).
    Toque,
}

impl AlturaLinha {
    #[must_use]
    /// A altura em pixels lógicos.
    pub const fn pixels(self) -> f32 {
        match self {
            Self::Compacta => 28.0,
            Self::Confortavel => 32.0,
            Self::Toque => 36.0,
        }
    }
}

/// Nível de elevação — borda + sombra mínima, nunca sombra dramática.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Elevacao {
    /// Sem sombra.
    #[default]
    Nivel0,
    /// Sombra mínima — cartão em repouso.
    Nivel1,
    /// Sombra mais perceptível — menu flutuante, modal.
    Nivel2,
}

impl Elevacao {
    /// `(deslocamento_y, desfoque, alfa)` da sombra — `0` em `Nivel0` (sem sombra).
    #[must_use]
    pub const fn sombra(self) -> (f32, f32, u8) {
        match self {
            Self::Nivel0 => (0.0, 0.0, 0),
            Self::Nivel1 => (1.0, 2.0, 15),
            Self::Nivel2 => (4.0, 12.0, 26),
        }
    }
}
