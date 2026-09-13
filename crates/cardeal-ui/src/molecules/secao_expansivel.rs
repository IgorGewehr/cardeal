//! Camada 2 (molecules) — `SecaoExpansivel`: um bloco de campos opcionais/secundários que
//! começa recolhido. `docs/12-ui-ux.md` §7 e a revisão de UI/UX de 2026-09-13: um formulário
//! de cadastro rápido (abrir OS, cadastrar produto) não deveria expor de cara os campos que
//! só importam às vezes (endereço do cliente, detalhes técnicos de uma peça) — eles ficam
//! aqui, a um clique de distância, sem competir visualmente com os poucos campos que de fato
//! bloqueiam o salvamento.
//!
//! Fino sobre `egui::CollapsingHeader`: só amarra a tipografia do título ao papel
//! `RotuloCampo` do design system (o padrão do egui usa o texto de corpo cru, fora da escala
//! do resto do formulário).

use egui::Ui;

use crate::tokens::{Papel, TemaUi};

/// Uma seção de formulário recolhível, com o título tipografado pelo design system.
#[must_use]
pub struct SecaoExpansivel {
    titulo: String,
    aberta_por_padrao: bool,
}

impl SecaoExpansivel {
    /// Uma seção com o título dado, recolhida por padrão.
    pub fn nova(titulo: impl Into<String>) -> Self {
        Self {
            titulo: titulo.into(),
            aberta_por_padrao: false,
        }
    }

    /// Começa aberta — para a seção opcional que ainda assim é preenchida com frequência.
    pub const fn aberta_por_padrao(mut self, v: bool) -> Self {
        self.aberta_por_padrao = v;
        self
    }

    /// Desenha a seção; `conteudo` monta os campos de dentro.
    pub fn mostrar(self, ui: &mut Ui, conteudo: impl FnOnce(&mut Ui)) {
        let cores = ui.cores();
        egui::CollapsingHeader::new(
            egui::RichText::new(self.titulo.clone())
                .font(Papel::RotuloCampo.font_id())
                .color(cores.texto_medio),
        )
        .id_salt(("secao-expansivel", self.titulo))
        .default_open(self.aberta_por_padrao)
        .show(ui, conteudo);
    }
}
