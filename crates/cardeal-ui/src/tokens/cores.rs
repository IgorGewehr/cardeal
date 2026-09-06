//! Camada 0 (ions) — a paleta Rubro. `docs/12-ui-ux.md` §2.
//!
//! Cada token é uma constante tipada, nunca uma string hex solta numa tela: usar a cor
//! errada aqui é erro de compilação (`ui.cores().rubro_500`, não `"#EF443B"`).
//!
//! A paleta chega às telas por dois caminhos, nunca por parâmetro `&Cores` na assinatura:
//!
//! - componentes que desenham com `egui` cru (campo, combo, separador) recebem o tema via
//!   [`instalar_estilo`](super::instalar_estilo), que traduz `Cores` para `egui::Style`;
//! - componentes do design system leem [`TemaUi::cores`](super::TemaUi) direto do `Ui`.

// Os literais hex ficam agrupados como `RRGGBB` (a forma em que a paleta do doc 12 é lida e
// conferida), não como o agrupamento de dígitos que o clippy pediria.
#![allow(clippy::unreadable_literal)]

use egui::Color32;

const fn hex(v: u32) -> Color32 {
    Color32::from_rgb(
        ((v >> 16) & 0xFF) as u8,
        ((v >> 8) & 0xFF) as u8,
        (v & 0xFF) as u8,
    )
}

/// A escala Rubro (marca), fixa entre os dois temas — `docs/12-ui-ux.md` §2.1.
pub struct Rubro;

impl Rubro {
    /// Fundo de destaque muito sutil, linha selecionada.
    pub const R50: Color32 = hex(0xFFF2F1);
    /// Fundo de badge, hover de item de menu.
    pub const R100: Color32 = hex(0xFFE1DF);
    /// Bordas suaves, divisórias em contexto rubro.
    pub const R200: Color32 = hex(0xFFC5C1);
    /// Ilustração, gráfico secundário.
    pub const R300: Color32 = hex(0xFF9C95);
    /// Hover do botão primário.
    pub const R400: Color32 = hex(0xF87068);
    /// A cor da marca: botão primário, indicador ativo, logotipo.
    pub const R500: Color32 = hex(0xEF443B);
    /// Pressionado; texto sobre fundo claro.
    pub const R600: Color32 = hex(0xD93028);
    /// Erro / destrutivo. Nunca a mesma função visual que `R500` na mesma tela.
    pub const R700: Color32 = hex(0xB3221C);
    /// Erro em fundo escuro.
    pub const R800: Color32 = hex(0x8C1A15);
    /// Texto de erro em superfície muito clara.
    pub const R900: Color32 = hex(0x6B1411);

    /// A cor de texto/ícone que vai **sobre** um preenchimento `R400`–`R600` (contraste AA).
    pub const CONTRASTE: Color32 = Color32::WHITE;
}

/// Neutros e semânticos de um tema (claro ou escuro) — `docs/12-ui-ux.md` §2.2–2.4.
#[derive(Debug, Clone, Copy)]
pub struct Cores {
    /// Superfície da janela.
    pub fundo: Color32,
    /// Cartões, painéis.
    pub superficie: Color32,
    /// Cabeçalho de tabela, sidebar.
    pub superficie_2: Color32,
    /// Fundo de hover de item/linha — bem sutil (`rgba(0,0,0,.04)` claro).
    pub superficie_hover: Color32,
    /// Fundo do item de navegação ativo — tint da marca a ~10% (`bg-primary/10`), não o
    /// rosa saturado do `rubro-50`.
    pub rubro_ativo: Color32,
    /// A cor da marca (`Rubro::R500`), como campo — pro componente não importar `Rubro`.
    pub rubro: Color32,
    /// Divisórias.
    pub borda: Color32,
    /// Contorno de campo.
    pub borda_forte: Color32,
    /// Legenda, placeholder.
    pub texto_fraco: Color32,
    /// Rótulo, texto secundário.
    pub texto_medio: Color32,
    /// Texto principal.
    pub texto: Color32,
    /// Títulos, valores em destaque.
    pub texto_forte: Color32,

    /// Entrada de dinheiro, saldo positivo, autorizado.
    pub positivo: Color32,
    /// Fundo para `positivo`.
    pub positivo_suave: Color32,
    /// Saída de dinheiro, saldo negativo, rejeitado. = `Rubro::R700` no claro — nunca por
    /// coincidência (`docs/12-ui-ux.md` §2.3).
    pub negativo: Color32,
    /// Fundo para `negativo`.
    pub negativo_suave: Color32,
    /// Vence hoje, em contingência, requer conferência.
    pub atencao: Color32,
    /// Fundo para `atencao`.
    pub atencao_suave: Color32,
    /// Previsto, informativo, em processamento.
    pub info: Color32,
    /// Fundo para `info`.
    pub info_suave: Color32,
    /// Séries neutras em gráfico.
    pub neutro_barra: Color32,
}

impl Cores {
    /// Tema claro — padrão.
    #[must_use]
    pub const fn claro() -> Self {
        Self {
            fundo: hex(0xF8F9FA),
            superficie: hex(0xFFFFFF),
            superficie_2: hex(0xF3F4F6),
            superficie_hover: hex(0xEEEFF1),
            rubro_ativo: hex(0xFDECEB),
            rubro: Rubro::R500,
            borda: hex(0xE7E7EA),
            borda_forte: hex(0xD4D4D8),
            texto_fraco: hex(0x8A8A93),
            texto_medio: hex(0x52525B),
            texto: hex(0x27272A),
            texto_forte: hex(0x111114),

            positivo: hex(0x12855A),
            positivo_suave: hex(0xE6F5EE),
            negativo: Rubro::R700,
            negativo_suave: hex(0xFDEBEA),
            atencao: hex(0xB45309),
            atencao_suave: hex(0xFEF3E2),
            info: hex(0x1D4ED8),
            info_suave: hex(0xEAF0FE),
            neutro_barra: hex(0xA1A1AA),
        }
    }

    /// Tema escuro — mesma estrutura, superfície e texto invertidos; a marca não muda
    /// (`docs/12-ui-ux.md` §2.4). Os fundos semânticos (`*_suave`) são tinturas **escuras**,
    /// nunca os valores do tema claro.
    #[must_use]
    pub const fn escuro() -> Self {
        Self {
            fundo: hex(0x0F1012),
            superficie: hex(0x17181B),
            superficie_2: hex(0x1E1F23),
            superficie_hover: hex(0x22242A),
            rubro_ativo: hex(0x33191A),
            rubro: Rubro::R500,
            borda: hex(0x2A2B30),
            borda_forte: hex(0x3A3B41),
            texto_fraco: hex(0x7A7B84),
            texto_medio: hex(0xA9AAB2),
            texto: hex(0xEDEDF0),
            texto_forte: hex(0xFFFFFF),

            positivo: hex(0x3BBE86),
            positivo_suave: hex(0x102A20),
            negativo: hex(0xF3766E),
            negativo_suave: hex(0x2C1512),
            atencao: hex(0xE0993A),
            atencao_suave: hex(0x2A1E0A),
            info: hex(0x6B9BF0),
            info_suave: hex(0x121C33),
            neutro_barra: hex(0x6B6C75),
        }
    }
}

/// Qual dos dois temas está ativo — `docs/12-ui-ux.md` §2.4, alternado por `Ctrl+Shift+D`.
///
/// É `'static + Send + Sync + Copy`, então mora em `ctx.data` e chega a qualquer `Ui` via
/// [`TemaUi`](super::TemaUi) — nenhuma tela precisa carregar `Cores` na mão.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tema {
    /// Padrão.
    #[default]
    Claro,
    /// Padrão sugerido para terminais de PDV em ambiente de baixa luz.
    Escuro,
}

impl Tema {
    #[must_use]
    /// As cores deste tema.
    pub const fn cores(self) -> Cores {
        match self {
            Self::Claro => Cores::claro(),
            Self::Escuro => Cores::escuro(),
        }
    }

    /// O tema oposto — para o gesto de alternância.
    #[must_use]
    pub const fn alternado(self) -> Self {
        match self {
            Self::Claro => Self::Escuro,
            Self::Escuro => Self::Claro,
        }
    }

    /// Verdadeiro no tema escuro.
    #[must_use]
    pub const fn e_escuro(self) -> bool {
        matches!(self, Self::Escuro)
    }
}
