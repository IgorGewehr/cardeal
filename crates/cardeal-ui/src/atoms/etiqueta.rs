//! Camada 1 (atoms) — `Etiqueta`: uma pílula de estado (badge). `docs/12-ui-ux.md` §2.3.
//!
//! Um rótulo curto com fundo `*_suave` e texto na cor semântica — o jeito do design system
//! de mostrar o estado de uma linha (orçamento "Enviado", título "Vencido", OS "Em
//! execução") sem recorrer a texto colorido solto numa grade.
//!
//! É um `Widget`: `ui.add(Etiqueta::nova("Aprovado", Tom::Positivo))`.

use egui::{Color32, Response, Sense, Ui, Widget};

use crate::tokens::{Cores, Papel, Raio, TemaUi};

/// O tom semântico de uma [`Etiqueta`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tom {
    /// Neutro — cinza. Estados sem carga (rascunho, arquivado).
    Neutro,
    /// Positivo — verde. Aprovado, pago, concluído.
    Positivo,
    /// Atenção — âmbar. Aguardando, vence hoje, requer conferência.
    Atencao,
    /// Negativo — vermelho. Recusado, vencido, cancelado.
    Negativo,
    /// Informativo — azul. Enviado, em processamento, previsto.
    Info,
}

impl Tom {
    /// `(texto, fundo)` deste tom no tema dado — a única tabela tom → cor do design system,
    /// compartilhada por `Etiqueta`, `Painel` e `CartaoKpi` (nunca copie o `match`).
    #[must_use]
    pub fn cores(self, cores: &Cores) -> (Color32, Color32) {
        match self {
            Self::Neutro => (cores.texto_medio, cores.superficie_2),
            Self::Positivo => (cores.positivo, cores.positivo_suave),
            Self::Atencao => (cores.atencao, cores.atencao_suave),
            Self::Negativo => (cores.negativo, cores.negativo_suave),
            Self::Info => (cores.info, cores.info_suave),
        }
    }
}

/// Uma pílula de estado.
#[must_use]
pub struct Etiqueta {
    texto: String,
    tom: Tom,
    ponto: bool,
}

impl Etiqueta {
    /// Uma etiqueta com texto e tom.
    pub fn nova(texto: impl Into<String>, tom: Tom) -> Self {
        Self {
            texto: texto.into(),
            tom,
            ponto: false,
        }
    }

    /// Antepõe um ponto colorido ao texto — o "status" (● aberto, ● fechado). O ponto
    /// reforça o tom sem depender só da cor do fundo (`docs/12-ui-ux.md` §9).
    pub const fn com_ponto(mut self) -> Self {
        self.ponto = true;
        self
    }

    /// Etiqueta neutra.
    pub fn neutra(texto: impl Into<String>) -> Self {
        Self::nova(texto, Tom::Neutro)
    }

    /// Etiqueta positiva.
    pub fn positiva(texto: impl Into<String>) -> Self {
        Self::nova(texto, Tom::Positivo)
    }

    /// Etiqueta de atenção.
    pub fn atencao(texto: impl Into<String>) -> Self {
        Self::nova(texto, Tom::Atencao)
    }

    /// Etiqueta negativa.
    pub fn negativa(texto: impl Into<String>) -> Self {
        Self::nova(texto, Tom::Negativo)
    }

    /// Etiqueta informativa.
    pub fn info(texto: impl Into<String>) -> Self {
        Self::nova(texto, Tom::Info)
    }
}

impl Widget for Etiqueta {
    fn ui(self, ui: &mut Ui) -> Response {
        let cores = ui.cores();
        let (fg, bg) = self.tom.cores(&cores);

        let fonte = Papel::RotuloCampo.font_id();
        let galley = ui.painter().layout_no_wrap(self.texto, fonte, fg);

        let padding = egui::vec2(9.0, 3.5);
        let largura_ponto = if self.ponto { 12.0 } else { 0.0 };
        let tamanho = egui::vec2(
            galley.size().x + padding.x * 2.0 + largura_ponto,
            (galley.size().y + padding.y * 2.0).max(20.0),
        );
        let (rect, resp) = ui.allocate_exact_size(tamanho, Sense::hover());
        if ui.is_rect_visible(rect) {
            ui.painter().rect_filled(rect, Raio::PILULA, bg);
            let mut pos = rect.center() - galley.size() / 2.0;
            if self.ponto {
                pos.x += largura_ponto / 2.0;
                ui.painter().circle_filled(
                    egui::pos2(rect.left() + padding.x + 3.0, rect.center().y),
                    3.0,
                    fg,
                );
            }
            ui.painter().galley(pos, galley, fg);
        }
        resp
    }
}
