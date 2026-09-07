//! Camada 3 (organisms) — `GraficoBarras`: barras verticais agrupadas para as telas de
//! análise (`docs/modulos/financeiro.md` §11.9). Sem biblioteca externa — `egui::Painter`
//! puro, tokens do tema, grade leve e rótulo no hover.
//!
//! Genérico sobre o domínio: recebe rótulos do eixo X e uma ou mais [`SerieBarras`]
//! (rótulo + cor + um valor por categoria) e devolve nada — é leitura.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use egui::{Color32, Rect, Sense, Ui};

use crate::tokens::{suave, Papel, Raio, TemaUi};

/// Uma série de valores — uma barra por categoria do eixo X.
pub struct SerieBarras {
    /// O nome exibido na legenda.
    pub rotulo: String,
    /// A cor de preenchimento das barras desta série.
    pub cor: Color32,
    /// Um valor por categoria, na mesma ordem do eixo X. Valores faltando contam como zero.
    pub valores: Vec<f64>,
}

/// Barras verticais agrupadas por categoria.
#[must_use]
pub struct GraficoBarras<'a> {
    eixo_x: &'a [String],
    series: &'a [SerieBarras],
    altura: f32,
    /// Formata os rótulos do eixo Y e do hover como dinheiro (milhares com `k`).
    moeda: bool,
}

impl<'a> GraficoBarras<'a> {
    /// Um gráfico com as categorias e séries dadas, 240px de altura.
    pub const fn novo(eixo_x: &'a [String], series: &'a [SerieBarras]) -> Self {
        Self {
            eixo_x,
            series,
            altura: 240.0,
            moeda: true,
        }
    }

    /// Ajusta a altura total (inclui eixo e legenda).
    pub const fn altura(mut self, px: f32) -> Self {
        self.altura = px;
        self
    }

    /// Rótulos numéricos crus em vez de dinheiro.
    pub const fn numerico(mut self) -> Self {
        self.moeda = false;
        self
    }

    /// Desenha o gráfico ocupando toda a largura disponível.
    #[allow(clippy::too_many_lines)]
    pub fn mostrar(self, ui: &mut Ui) {
        let cores = ui.cores();
        // As barras crescem da linha de base ao aparecer pela primeira vez (Pilar I: anima
        // uma vez e dorme — `animate_bool` com alvo fixo `true` não re-anima em revisitas).
        let crescer = suave(
            ui.ctx()
                .animate_bool_with_time(ui.id().with("grafico-crescer"), true, 0.5_f32),
        );
        let largura = ui.available_width().max(1.0);
        let altura_legenda = if self.series.len() > 1 { 22.0 } else { 4.0 };
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(largura, self.altura),
            Sense::hover(),
        );
        let p = ui.painter_at(rect);

        let gutter_e = 56.0_f32;
        let pad_topo = 8.0_f32;
        let pad_base = 22.0_f32;
        let plot = Rect::from_min_max(
            egui::pos2(rect.left() + gutter_e, rect.top() + pad_topo),
            egui::pos2(
                rect.right() - 8.0,
                rect.bottom() - pad_base - altura_legenda,
            ),
        );
        if plot.width() < 1.0 || plot.height() < 1.0 {
            return;
        }

        // Máximo "bonito" para o topo do eixo.
        let bruto = self
            .series
            .iter()
            .flat_map(|s| s.valores.iter().copied())
            .fold(0.0_f64, f64::max)
            .max(1.0);
        let teto = teto_bonito(bruto);

        // Grade + rótulos do eixo Y (4 divisões).
        for i in 0..=4 {
            let f = f64::from(i) / 4.0;
            let y = plot.bottom() - (f as f32) * plot.height();
            p.hline(
                plot.left()..=plot.right(),
                y,
                egui::Stroke::new(1.0_f32, mistura_local(cores.borda, cores.superficie, 0.4)),
            );
            let valor = teto * f;
            p.text(
                egui::pos2(plot.left() - 8.0, y),
                egui::Align2::RIGHT_CENTER,
                if self.moeda {
                    rotulo_moeda(valor)
                } else {
                    format!("{valor:.0}")
                },
                Papel::RotuloCampo.font_id(),
                cores.texto_fraco,
            );
        }

        // Barras.
        let n_cat = self.eixo_x.len().max(1);
        let larg_slot = plot.width() / n_cat as f32;
        let n_series = self.series.len().max(1) as f32;
        let larg_grupo = (larg_slot * 0.62).max(6.0);
        let larg_barra = (larg_grupo / n_series - 3.0).max(3.0);

        for (ci, rotulo_x) in self.eixo_x.iter().enumerate() {
            let cx = plot.left() + larg_slot * (ci as f32 + 0.5);
            let grupo_esq = cx - larg_grupo / 2.0;
            for (si, serie) in self.series.iter().enumerate() {
                let v = serie.valores.get(ci).copied().unwrap_or(0.0).max(0.0);
                let h = ((v / teto) as f32 * plot.height()).max(0.0) * crescer;
                let x0 = grupo_esq + si as f32 * (larg_barra + 3.0);
                let barra = Rect::from_min_max(
                    egui::pos2(x0, plot.bottom() - h),
                    egui::pos2(x0 + larg_barra, plot.bottom()),
                );
                let arred = egui::Rounding {
                    nw: 3.0,
                    ne: 3.0,
                    sw: 0.0,
                    se: 0.0,
                };
                p.rect_filled(barra, arred, serie.cor);
                let alvo = barra.expand2(egui::vec2(1.5, 0.0));
                let resp = ui.interact(
                    alvo,
                    ui.id().with(("grafico-barra", ci, si)),
                    Sense::hover(),
                );
                if resp.hovered() {
                    p.rect_filled(barra, arred, serie.cor.gamma_multiply(1.2));
                    let dica = if self.moeda {
                        rotulo_moeda_exato(v)
                    } else {
                        format!("{v:.0}")
                    };
                    resp.on_hover_text(format!("{} · {dica}", serie.rotulo));
                }
            }
            p.text(
                egui::pos2(cx, plot.bottom() + 6.0),
                egui::Align2::CENTER_TOP,
                rotulo_x,
                Papel::RotuloCampo.font_id(),
                cores.texto_medio,
            );
        }

        // Linha de base.
        p.hline(
            plot.left()..=plot.right(),
            plot.bottom(),
            egui::Stroke::new(1.0_f32, cores.borda),
        );

        // Legenda.
        if self.series.len() > 1 {
            let mut x = plot.left();
            let y = rect.bottom() - altura_legenda / 2.0;
            for serie in self.series {
                let quad = Rect::from_center_size(egui::pos2(x + 6.0, y), egui::vec2(10.0, 10.0));
                p.rect_filled(quad, Raio::CAMPO, serie.cor);
                let galley = p.layout_no_wrap(
                    serie.rotulo.clone(),
                    Papel::RotuloCampo.font_id(),
                    cores.texto_medio,
                );
                let largura_txt = galley.size().x;
                p.galley(egui::pos2(x + 16.0, y - galley.size().y / 2.0), galley, cores.texto_medio);
                x += 16.0 + largura_txt + 18.0;
            }
        }
    }
}

fn mistura_local(a: Color32, b: Color32, t: f32) -> Color32 {
    let a = egui::Rgba::from(a);
    let b = egui::Rgba::from(b);
    Color32::from(a * (1.0 - t) + b * t)
}

/// Arredonda `v` para cima até 1/2/5 × 10ⁿ — um topo de eixo legível.
fn teto_bonito(v: f64) -> f64 {
    if v <= 0.0 {
        return 1.0;
    }
    let exp = v.log10().floor();
    let base = 10.0_f64.powf(exp_ou_zero(exp));
    let norm = v / base;
    let passo = if norm <= 1.0 {
        1.0
    } else if norm <= 2.0 {
        2.0
    } else if norm <= 5.0 {
        5.0
    } else {
        10.0
    };
    passo * base
}

fn exp_ou_zero(e: f64) -> f64 {
    if e.is_finite() {
        e
    } else {
        0.0
    }
}

/// Rótulo de eixo — `v` em reais. Milhares viram `k`.
fn rotulo_moeda(v: f64) -> String {
    if v.abs() >= 10_000.0 {
        format!("{:.0}k", v / 1000.0)
    } else if v.abs() >= 1000.0 {
        format!("{:.1}k", v / 1000.0)
    } else {
        format!("{v:.0}")
    }
}

/// Rótulo de hover — `v` em reais, com símbolo e centavos.
fn rotulo_moeda_exato(v: f64) -> String {
    format!("R$ {v:.2}")
}
