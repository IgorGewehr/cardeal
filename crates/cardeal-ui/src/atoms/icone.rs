//! Camada 1 (atoms) — o conjunto de ícones vetoriais da marca.
//!
//! `cardeal-modkit` só declara a identidade (`Icone::Estoque`); o traço é aqui
//! (`docs/12-ui-ux.md` §7). Substitui a ponte de emoji que existia antes: agora cada ícone é
//! um traço próprio, 1.5px, desenhado no `painter` — mesma linguagem visual em qualquer
//! tamanho, sem depender da fonte de emoji do sistema.

// Módulo de desenho: índices de laço pequenos viram `f32` o tempo todo.
#![allow(clippy::cast_precision_loss)]

use cardeal_modkit::Icone;
use egui::{Color32, Pos2, Sense, Shape, Stroke, Ui, Vec2};

/// Desenha `icone` num quadrado de lado `tamanho`, alocando o espaço no `ui`.
pub fn desenhar(ui: &mut Ui, icone: Icone, tamanho: f32, cor: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(tamanho), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let largura = (tamanho / 13.0).clamp(1.25, 2.5);
    let t = Stroke::new(largura, cor);
    let d = Desenho {
        origem: rect.min,
        lado: rect.size(),
        t,
    };
    d.icone(ui, icone);
}

struct Desenho {
    origem: Pos2,
    lado: Vec2,
    t: Stroke,
}

impl Desenho {
    /// Converte coordenadas normalizadas (0..1) para a tela.
    fn p(&self, x: f32, y: f32) -> Pos2 {
        self.origem + Vec2::new(x * self.lado.x, y * self.lado.y)
    }

    fn linha(&self, ui: &Ui, a: (f32, f32), b: (f32, f32)) {
        ui.painter()
            .line_segment([self.p(a.0, a.1), self.p(b.0, b.1)], self.t);
    }

    fn poli(&self, ui: &Ui, pts: &[(f32, f32)]) {
        let pontos = pts.iter().map(|&(x, y)| self.p(x, y)).collect();
        ui.painter().add(Shape::line(pontos, self.t));
    }

    fn fechada(&self, ui: &Ui, pts: &[(f32, f32)]) {
        let pontos = pts.iter().map(|&(x, y)| self.p(x, y)).collect();
        ui.painter().add(Shape::closed_line(pontos, self.t));
    }

    fn circulo(&self, ui: &Ui, c: (f32, f32), r: f32) {
        ui.painter()
            .circle_stroke(self.p(c.0, c.1), r * self.lado.x, self.t);
    }

    fn arco(&self, ui: &Ui, c: (f32, f32), r: f32, a0: f32, a1: f32) {
        let centro = self.p(c.0, c.1);
        let raio = r * self.lado.x;
        let n = 20;
        let pts = (0..=n)
            .map(|i| {
                let a = a0 + (a1 - a0) * (i as f32 / n as f32);
                centro + Vec2::angled(a) * raio
            })
            .collect();
        ui.painter().add(Shape::line(pts, self.t));
    }

    #[allow(clippy::too_many_lines)]
    fn icone(&self, ui: &Ui, icone: Icone) {
        use std::f32::consts::PI;
        match icone {
            Icone::Pulso => self.poli(
                ui,
                &[
                    (0.06, 0.52),
                    (0.30, 0.52),
                    (0.40, 0.24),
                    (0.52, 0.80),
                    (0.62, 0.52),
                    (0.94, 0.52),
                ],
            ),
            Icone::Dinheiro => {
                self.circulo(ui, (0.5, 0.5), 0.42);
                self.linha(ui, (0.5, 0.22), (0.5, 0.78));
                self.arco(ui, (0.5, 0.38), 0.13, 0.35 * PI, 1.6 * PI);
                self.arco(ui, (0.5, 0.62), 0.13, 1.35 * PI, 2.6 * PI);
            }
            Icone::Carrinho => {
                self.poli(
                    ui,
                    &[
                        (0.05, 0.12),
                        (0.20, 0.12),
                        (0.33, 0.62),
                        (0.83, 0.62),
                        (0.92, 0.26),
                        (0.27, 0.26),
                    ],
                );
                self.circulo(ui, (0.40, 0.82), 0.07);
                self.circulo(ui, (0.76, 0.82), 0.07);
            }
            Icone::Caixa => {
                self.fechada(
                    ui,
                    &[(0.12, 0.40), (0.88, 0.40), (0.88, 0.88), (0.12, 0.88)],
                );
                self.fechada(
                    ui,
                    &[(0.22, 0.16), (0.60, 0.16), (0.60, 0.40), (0.22, 0.40)],
                );
                self.linha(ui, (0.68, 0.54), (0.80, 0.54));
                self.linha(ui, (0.68, 0.68), (0.80, 0.68));
            }
            Icone::Pessoas => {
                self.circulo(ui, (0.38, 0.32), 0.14);
                self.poli(
                    ui,
                    &[(0.16, 0.86), (0.20, 0.62), (0.52, 0.62), (0.58, 0.86)],
                );
                self.circulo(ui, (0.70, 0.36), 0.11);
                self.poli(
                    ui,
                    &[(0.62, 0.60), (0.72, 0.52), (0.86, 0.56), (0.88, 0.80)],
                );
            }
            Icone::Estoque => {
                self.fechada(
                    ui,
                    &[(0.16, 0.40), (0.58, 0.40), (0.58, 0.86), (0.16, 0.86)],
                );
                self.poli(
                    ui,
                    &[(0.16, 0.40), (0.38, 0.20), (0.80, 0.20), (0.58, 0.40)],
                );
                self.poli(
                    ui,
                    &[(0.58, 0.40), (0.80, 0.20), (0.80, 0.66), (0.58, 0.86)],
                );
            }
            Icone::Nota => {
                self.fechada(
                    ui,
                    &[
                        (0.22, 0.10),
                        (0.62, 0.10),
                        (0.78, 0.26),
                        (0.78, 0.90),
                        (0.22, 0.90),
                    ],
                );
                self.poli(ui, &[(0.62, 0.10), (0.62, 0.26), (0.78, 0.26)]);
                self.linha(ui, (0.33, 0.46), (0.67, 0.46));
                self.linha(ui, (0.33, 0.60), (0.67, 0.60));
                self.linha(ui, (0.33, 0.74), (0.55, 0.74));
            }
            Icone::Grafico => {
                self.poli(ui, &[(0.14, 0.10), (0.14, 0.86), (0.92, 0.86)]);
                for (x0, y0) in [(0.26, 0.58), (0.45, 0.40), (0.64, 0.24)] {
                    self.fechada(
                        ui,
                        &[(x0, y0), (x0 + 0.12, y0), (x0 + 0.12, 0.86), (x0, 0.86)],
                    );
                }
            }
            Icone::Agenda => {
                self.fechada(
                    ui,
                    &[(0.14, 0.20), (0.86, 0.20), (0.86, 0.88), (0.14, 0.88)],
                );
                self.linha(ui, (0.14, 0.38), (0.86, 0.38));
                self.linha(ui, (0.32, 0.12), (0.32, 0.26));
                self.linha(ui, (0.68, 0.12), (0.68, 0.26));
                self.circulo(ui, (0.5, 0.62), 0.04);
            }
            Icone::Ferramenta => {
                self.poli(
                    ui,
                    &[
                        (0.58, 0.14),
                        (0.80, 0.20),
                        (0.84, 0.40),
                        (0.66, 0.46),
                        (0.52, 0.34),
                        (0.56, 0.22),
                    ],
                );
                self.linha(ui, (0.60, 0.40), (0.24, 0.82));
                self.circulo(ui, (0.22, 0.84), 0.06);
            }
            Icone::Chave => {
                self.circulo(ui, (0.32, 0.36), 0.18);
                self.linha(ui, (0.44, 0.48), (0.84, 0.86));
                self.linha(ui, (0.74, 0.78), (0.82, 0.68));
                self.linha(ui, (0.64, 0.66), (0.72, 0.56));
            }
            Icone::Config => {
                self.circulo(ui, (0.5, 0.5), 0.34);
                self.circulo(ui, (0.5, 0.5), 0.14);
                for k in 0..8 {
                    let a = (k as f32) * PI / 4.0;
                    let d = Vec2::angled(a);
                    let c = self.p(0.5, 0.5);
                    ui.painter().line_segment(
                        [c + d * 0.34 * self.lado.x, c + d * 0.46 * self.lado.x],
                        self.t,
                    );
                }
            }
            Icone::Banco => {
                self.poli(ui, &[(0.08, 0.42), (0.5, 0.16), (0.92, 0.42)]);
                self.linha(ui, (0.12, 0.46), (0.88, 0.46));
                for x in [0.22, 0.40, 0.60, 0.78] {
                    self.linha(ui, (x, 0.48), (x, 0.82));
                }
                self.linha(ui, (0.08, 0.86), (0.92, 0.86));
            }
            Icone::Conciliar => {
                self.arco(ui, (0.5, 0.5), 0.34, 0.9 * PI, -0.15 * PI);
                self.poli(ui, &[(0.72, 0.06), (0.86, 0.20), (0.66, 0.26)]);
                self.arco(ui, (0.5, 0.5), 0.34, 1.9 * PI, 0.85 * PI);
                self.poli(ui, &[(0.28, 0.94), (0.14, 0.80), (0.34, 0.74)]);
            }
            Icone::Alerta => {
                self.fechada(ui, &[(0.5, 0.10), (0.92, 0.86), (0.08, 0.86)]);
                self.linha(ui, (0.5, 0.36), (0.5, 0.62));
                self.circulo(ui, (0.5, 0.74), 0.02);
            }
            _ => self.circulo(ui, (0.5, 0.5), 0.12),
        }
    }
}
