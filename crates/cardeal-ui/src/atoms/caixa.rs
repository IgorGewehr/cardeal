//! Camada 1 (atoms) — `Caixa`: a caixa de seleção do design system (liga/desliga com rótulo).
//!
//! Substitui o `ui.checkbox()` cru das telas (ADR-0015). Clica-se na caixa **ou no rótulo**;
//! com o teclado, `Tab` foca e `Espaço` alterna (o `egui` trata `Espaço`/`Enter` num widget
//! focado como clique). O foco aparece como o mesmo anel dos botões (`docs/12-ui-ux.md` §9).

use egui::{Color32, CursorIcon, Response, Sense, Stroke, Ui, Vec2, Widget};

use crate::tokens::{ativar, lerp_cor, Mov, Papel, Raio, Rubro, TemaUi};

/// Lado da caixa, em pixels.
const LADO: f32 = 20.0;
/// Espaço entre a caixa e o rótulo.
const RESPIRO: f32 = 10.0;

/// Uma caixa de seleção ligada a um `bool`.
#[must_use]
pub struct Caixa<'a> {
    valor: &'a mut bool,
    rotulo: String,
}

impl<'a> Caixa<'a> {
    /// Uma caixa com o rótulo dado, ligada a `valor`.
    pub fn nova(valor: &'a mut bool, rotulo: impl Into<String>) -> Self {
        Self {
            valor,
            rotulo: rotulo.into(),
        }
    }
}

impl Widget for Caixa<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let cores = ui.cores();
        let galley = ui.painter().layout_no_wrap(
            self.rotulo,
            Papel::Interface.font_id(),
            Color32::PLACEHOLDER,
        );
        let tamanho = Vec2::new(
            LADO + RESPIRO + galley.size().x,
            LADO.max(galley.size().y) + 8.0,
        );
        let (rect, mut resp) = ui.allocate_exact_size(tamanho, Sense::click());
        if resp.clicked() {
            *self.valor = !*self.valor;
            resp.mark_changed();
        }

        if ui.is_rect_visible(rect) {
            let ligada = ativar(ui, resp.id.with("ligada"), *self.valor, Mov::RAPIDO);
            let th = ativar(ui, resp.id.with("hover"), resp.hovered(), Mov::RAPIDO);
            let tf = ativar(ui, resp.id.with("foco"), resp.has_focus(), Mov::PADRAO);

            let caixa = egui::Rect::from_min_size(
                egui::pos2(rect.left(), rect.center().y - LADO / 2.0),
                Vec2::splat(LADO),
            );
            let painter = ui.painter();
            if tf > 0.001_f32 {
                painter.rect_stroke(
                    caixa.expand(3.0_f32),
                    Raio::CAMPO * 0.5 + 3.0_f32,
                    Stroke::new(3.0_f32, Rubro::R500.gamma_multiply(0.25_f32 * tf)),
                );
            }
            let borda = lerp_cor(
                lerp_cor(cores.borda_forte, cores.texto_fraco, th),
                Rubro::R500,
                ligada,
            );
            painter.rect_filled(
                caixa,
                Raio::CAMPO * 0.5,
                lerp_cor(cores.superficie, Rubro::R500, ligada),
            );
            painter.rect_stroke(caixa, Raio::CAMPO * 0.5, Stroke::new(1.5_f32, borda));
            if ligada > 0.001_f32 {
                // O "✓", desenhado (não um glifo: não depende de a fonte ter o caractere).
                let c = caixa.center();
                let traco = Stroke::new(2.0_f32, Rubro::CONTRASTE.gamma_multiply(ligada));
                painter.line_segment(
                    [
                        egui::pos2(c.x - 4.5, c.y + 0.5),
                        egui::pos2(c.x - 1.5, c.y + 3.5),
                    ],
                    traco,
                );
                painter.line_segment(
                    [
                        egui::pos2(c.x - 1.5, c.y + 3.5),
                        egui::pos2(c.x + 4.5, c.y - 3.5),
                    ],
                    traco,
                );
            }
            painter.galley(
                egui::pos2(
                    caixa.right() + RESPIRO,
                    rect.center().y - galley.size().y / 2.0,
                ),
                galley,
                cores.texto,
            );
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        resp
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn contexto() -> egui::Context {
        let ctx = egui::Context::default();
        crate::tokens::instalar_fontes(&ctx);
        crate::tokens::instalar_estilo(&ctx, crate::tokens::Tema::Claro);
        ctx
    }

    /// Roda um quadro com uma `Caixa` e devolve o retângulo dela.
    fn quadro(ctx: &egui::Context, eventos: Vec<egui::Event>, valor: &mut bool) -> egui::Rect {
        let entrada = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 300.0),
            )),
            events: eventos,
            ..Default::default()
        };
        let mut rect = egui::Rect::NOTHING;
        let _ = ctx.run(entrada, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                rect = ui.add(Caixa::nova(valor, "Receber por Pix")).rect;
            });
        });
        rect
    }

    fn clique(pos: egui::Pos2, apertado: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: apertado,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn clicar_no_rotulo_alterna() {
        let ctx = contexto();
        let mut v = false;
        let rect = quadro(&ctx, vec![], &mut v);
        // O clique cai no texto, longe da caixinha: o rótulo também é alvo.
        let alvo = egui::pos2(rect.right() - 5.0, rect.center().y);
        quadro(&ctx, vec![egui::Event::PointerMoved(alvo)], &mut v);
        quadro(&ctx, vec![clique(alvo, true)], &mut v);
        quadro(&ctx, vec![clique(alvo, false)], &mut v);
        assert!(v, "o clique tem que ligar");
        let alvo = egui::pos2(rect.left() + 5.0, rect.center().y);
        quadro(&ctx, vec![egui::Event::PointerMoved(alvo)], &mut v);
        quadro(&ctx, vec![clique(alvo, true)], &mut v);
        quadro(&ctx, vec![clique(alvo, false)], &mut v);
        assert!(!v, "e desligar");
    }

    #[test]
    fn tab_foca_e_espaco_alterna() {
        let ctx = contexto();
        let mut v = false;
        let tecla = |k| egui::Event::Key {
            key: k,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        quadro(&ctx, vec![], &mut v);
        quadro(&ctx, vec![tecla(egui::Key::Tab)], &mut v);
        quadro(&ctx, vec![tecla(egui::Key::Space)], &mut v);
        assert!(v, "Espaço numa caixa focada tem que ligar");
    }
}
