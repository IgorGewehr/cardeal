//! Camada 2 (molecules) — `Abas`: a barra de abas do design system, com a pílula do
//! indicador **deslizando** entre as opções (`docs/12-ui-ux.md` §1.5 / §5 — animação explica
//! a transição de estado).
//!
//! Substitui o `egui::Frame` + `Botao::primario/fantasma().pequeno()` copiado à mão em cada
//! tela (`tela_financeiro`, `tela_settings`, `tela_os`). Genérica sobre o tipo da aba
//! (`enum`, `usize`…), desde que `PartialEq + Copy`.
//!
//! ```ignore
//! if let Some(nova) = Abas::nova(&[(Aba::Lista, "Lista"), (Aba::Grafico, "Gráfico")])
//!     .selecionada(estado.aba)
//!     .mostrar(ui)
//! {
//!     estado.aba = nova;
//! }
//! ```

use egui::{CursorIcon, Rect, Sense, Ui};

use crate::tokens::{perseguir, Espaco, Mov, Papel, Raio, TemaUi};

/// Uma barra de abas com indicador deslizante.
#[must_use]
pub struct Abas<'a, T> {
    itens: &'a [(T, &'a str)],
    selecionada: Option<T>,
    id_salt: &'a str,
}

impl<'a, T: PartialEq + Copy> Abas<'a, T> {
    /// Uma barra com os pares `(valor, rótulo)` dados.
    pub fn nova(itens: &'a [(T, &'a str)]) -> Self {
        Self {
            itens,
            selecionada: None,
            id_salt: "abas",
        }
    }

    /// A aba ativa (a pílula desliza para ela).
    pub fn selecionada(mut self, valor: T) -> Self {
        self.selecionada = Some(valor);
        self
    }

    /// Um sufixo de id, quando há mais de uma barra de abas na mesma tela.
    pub fn id_salt(mut self, s: &'a str) -> Self {
        self.id_salt = s;
        self
    }

    /// Desenha a barra. Devolve `Some(valor)` quando o usuário clica numa aba diferente.
    pub fn mostrar(self, ui: &mut Ui) -> Option<T> {
        const PAD_X: f32 = 14.0;
        const ALTURA: f32 = 30.0;
        const MARGEM: f32 = 3.0;

        if self.itens.is_empty() {
            return None;
        }
        let cores = ui.cores();
        let fonte = Papel::RotuloCampo.font_id();

        // Larguras de cada segmento.
        let larguras: Vec<f32> = self
            .itens
            .iter()
            .map(|(_, rot)| {
                ui.painter()
                    .layout_no_wrap((*rot).to_owned(), fonte.clone(), cores.texto)
                    .size()
                    .x
                    + PAD_X * 2.0
            })
            .collect();
        let largura_total: f32 = larguras.iter().sum::<f32>() + MARGEM * 2.0;
        let altura_total = ALTURA + MARGEM * 2.0;

        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(largura_total, altura_total), Sense::hover());
        if !ui.is_rect_visible(rect) {
            return None;
        }

        ui.painter()
            .rect_filled(rect, Raio::ITEM, cores.superficie_2);

        let idx_sel = self
            .selecionada
            .and_then(|s| self.itens.iter().position(|(v, _)| *v == s))
            .unwrap_or(0);

        // Posição-alvo da pílula = soma das larguras anteriores.
        let alvo_x: f32 = larguras[..idx_sel].iter().sum::<f32>();
        let id = ui.id().with(("abas-pilula", self.id_salt));
        let x_anim = perseguir(ui, id, alvo_x, Mov::CALMA);
        let larg_anim = {
            let id_w = ui.id().with(("abas-larg", self.id_salt));
            perseguir(ui, id_w, larguras[idx_sel], Mov::CALMA)
        };

        let pilula = Rect::from_min_size(
            egui::pos2(rect.left() + MARGEM + x_anim, rect.top() + MARGEM),
            egui::vec2(larg_anim, ALTURA),
        );
        ui.painter()
            .rect_filled(pilula, Raio::CAMPO, cores.superficie);
        ui.painter()
            .rect_stroke(pilula, Raio::CAMPO, egui::Stroke::new(1.0_f32, cores.borda));

        // Segmentos clicáveis.
        let mut clicada = None;
        let mut x = rect.left() + MARGEM;
        for (i, ((valor, rotulo), larg)) in self.itens.iter().zip(&larguras).enumerate() {
            let seg = Rect::from_min_size(
                egui::pos2(x, rect.top() + MARGEM),
                egui::vec2(*larg, ALTURA),
            );
            let resp = ui.interact(seg, id.with(i), Sense::click());
            let ativa = i == idx_sel;
            let cor = if ativa {
                cores.texto_forte
            } else if resp.hovered() {
                cores.texto
            } else {
                cores.texto_medio
            };
            let galley = ui
                .painter()
                .layout_no_wrap((*rotulo).to_owned(), fonte.clone(), cor);
            let pos = seg.center() - galley.size() / 2.0;
            ui.painter().galley(pos, galley, cor);
            if resp.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            if resp.clicked() && !ativa {
                clicada = Some(*valor);
            }
            x += *larg;
        }

        ui.add_space(Espaco::E4);
        clicada
    }
}
