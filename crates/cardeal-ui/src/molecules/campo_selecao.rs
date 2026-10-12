//! Camada 2 (molecules) — `CampoSelecao`: a caixa de um seletor que abre um painel (popup) em
//! vez de uma lista — rótulo, o valor escolhido (ou um texto de apoio) e o chevron. Só mostra
//! e informa o clique; quem abre o painel é o organismo
//! ([`SeletorEmArvore`](crate::organisms::SeletorEmArvore)). `docs/12-ui-ux.md` §7.

use egui::{Response, Sense, Ui, Vec2};

use crate::atoms::{moldura_foco_campo, Rotulo};
use crate::tokens::{Espaco, Papel, Raio, TemaUi};

/// A caixa clicável de um seletor com painel.
#[must_use]
pub struct CampoSelecao {
    rotulo: Option<String>,
    valor: Option<String>,
    marcador: String,
}

impl CampoSelecao {
    /// Uma caixa com o `valor` escolhido (`None` mostra o marcador).
    pub fn nova(valor: Option<String>) -> Self {
        Self {
            rotulo: None,
            valor,
            marcador: "Selecione…".to_owned(),
        }
    }

    /// O rótulo acima da caixa.
    pub fn rotulo(mut self, rotulo: impl Into<String>) -> Self {
        self.rotulo = Some(rotulo.into());
        self
    }

    /// O texto quando nada está escolhido.
    pub fn marcador(mut self, marcador: impl Into<String>) -> Self {
        self.marcador = marcador.into();
        self
    }

    /// Desenha; a `Response` é clicável (abre o painel) e recebe foco.
    pub fn mostrar(self, ui: &mut Ui) -> Response {
        let cores = ui.cores();
        ui.vertical(|ui| {
            if let Some(r) = &self.rotulo {
                ui.add(Rotulo::campo(r));
                ui.add_space(Espaco::E4);
            }
            let altura = 36.0_f32;
            let largura = (ui.available_width() - Espaco::E8).max(120.0_f32);
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(largura, altura), Sense::click());
            let p = ui.painter();
            p.rect(
                rect,
                Raio::CAMPO,
                cores.superficie,
                egui::Stroke::new(1.0_f32, cores.borda_forte),
            );
            let (texto, cor) = self.valor.as_ref().map_or_else(
                || (self.marcador.clone(), cores.texto_fraco),
                |v| (v.clone(), cores.texto),
            );
            // O caminho pode ser mais longo que a caixa: corta à esquerda do chevron.
            let area = egui::Rect::from_min_max(
                rect.left_center() + Vec2::new(Espaco::E12, -altura / 2.0),
                rect.right_center() + Vec2::new(-28.0_f32, altura / 2.0),
            );
            let galeria = p.layout_no_wrap(texto, Papel::Interface.font_id(), cor);
            let pos = egui::pos2(area.left(), rect.center().y - galeria.size().y / 2.0);
            p.with_clip_rect(area).galley(pos, galeria, cor);
            // Chevron para baixo.
            let c = rect.right_center() - Vec2::new(16.0_f32, 0.0);
            p.add(egui::Shape::convex_polygon(
                vec![
                    c + Vec2::new(-4.0_f32, -2.0_f32),
                    c + Vec2::new(4.0_f32, -2.0_f32),
                    c + Vec2::new(0.0_f32, 3.0_f32),
                ],
                cores.texto_medio,
                egui::Stroke::NONE,
            ));
            moldura_foco_campo(ui, &resp, Vec2::ZERO, None);
            resp
        })
        .inner
    }
}
