//! Camada 1 (atoms) — `Etiqueta`: uma pílula de estado (badge). `docs/12-ui-ux.md` §2.3.
//!
//! Um rótulo curto com fundo `*_suave` e texto na cor semântica — o jeito do design system
//! de mostrar o estado de uma linha (orçamento "Enviado", título "Vencido", OS "Em
//! execução") sem recorrer a texto colorido solto numa grade.
//!
//! É um `Widget`: `ui.add(Etiqueta::nova("Aprovado", Tom::Positivo))`.

use egui::{Response, Sense, Ui, Widget};

use crate::tokens::{Papel, Raio, TemaUi};

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

/// Uma pílula de estado.
#[must_use]
pub struct Etiqueta {
    texto: String,
    tom: Tom,
}

impl Etiqueta {
    /// Uma etiqueta com texto e tom.
    pub fn nova(texto: impl Into<String>, tom: Tom) -> Self {
        Self {
            texto: texto.into(),
            tom,
        }
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
        let (fg, bg) = match self.tom {
            Tom::Neutro => (cores.texto_medio, cores.superficie_2),
            Tom::Positivo => (cores.positivo, cores.positivo_suave),
            Tom::Atencao => (cores.atencao, cores.atencao_suave),
            Tom::Negativo => (cores.negativo, cores.negativo_suave),
            Tom::Info => (cores.info, cores.info_suave),
        };

        let fonte = Papel::RotuloCampo.font_id();
        let galley = ui.painter().layout_no_wrap(self.texto, fonte, fg);

        let padding = egui::vec2(9.0, 3.5);
        let tamanho = egui::vec2(
            galley.size().x + padding.x * 2.0,
            (galley.size().y + padding.y * 2.0).max(20.0),
        );
        let (rect, resp) = ui.allocate_exact_size(tamanho, Sense::hover());
        if ui.is_rect_visible(rect) {
            ui.painter().rect_filled(rect, Raio::PILULA, bg);
            let pos = rect.center() - galley.size() / 2.0;
            ui.painter().galley(pos, galley, fg);
        }
        resp
    }
}
