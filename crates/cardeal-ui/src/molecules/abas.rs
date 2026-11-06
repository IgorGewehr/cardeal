//! Camada 2 (molecules) — `Abas`: a barra de abas do design system, com a pílula do
//! indicador **deslizando** entre as opções (`docs/12-ui-ux.md` §1.5 / §5 — animação explica
//! a transição de estado).
//!
//! Quando as abas não cabem na largura disponível, a barra **quebra em mais linhas** (a pílula
//! desliza também entre elas). Antes ela passava da borda: numa tela de notebook (1280 px
//! lógicos, Windows a 150%) as 13 abas do Financeiro alargavam a tela inteira, e os cartões e
//! as faixas de KPI abaixo saíam cortados à direita.
//!
//! Dois estilos: **segmentos** (o padrão — a pílula numa trilha, para escolher período, visão,
//! filtro) e **navegação** ([`Abas::navegacao`] — as abas de primeiro nível de uma tela: texto
//! com um traço da marca sob a ativa e uma linha sob a barra). Com os dois iguais, a tela do
//! Financeiro empilhava quatro barras idênticas e não dava para ver qual mandava em qual.
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
    navegacao: bool,
}

impl<'a, T: PartialEq + Copy> Abas<'a, T> {
    /// Uma barra com os pares `(valor, rótulo)` dados.
    pub fn nova(itens: &'a [(T, &'a str)]) -> Self {
        Self {
            itens,
            selecionada: None,
            id_salt: "abas",
            navegacao: false,
        }
    }

    /// As abas de primeiro nível de uma tela: sem trilha, com um traço da marca sob a ativa e
    /// uma linha sob a barra inteira, que ocupa a largura toda. Os seletores de segundo nível
    /// continuam segmentos.
    pub fn navegacao(mut self) -> Self {
        self.navegacao = true;
        self
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
        // Segmentos: pílula numa trilha com margem; navegação: rótulos soltos, linhas mais altas
        // (o traço da ativa mora na base da linha).
        let (pad_x, altura, margem) = if self.navegacao {
            (12.0_f32, 36.0_f32, 0.0_f32)
        } else {
            (14.0_f32, 30.0_f32, 3.0_f32)
        };

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
                    + pad_x * 2.0
            })
            .collect();
        let cabe = (ui.available_width() - margem * 2.0).max(0.0);
        let (posicoes, largura_max, linhas) = quebrar_em_linhas(&larguras, cabe, altura);
        let largura_total = if self.navegacao {
            ui.available_width().max(largura_max)
        } else {
            largura_max + margem * 2.0
        };
        let altura_total = altura * linhas + margem * 2.0;

        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(largura_total, altura_total), Sense::hover());
        if !ui.is_rect_visible(rect) {
            return None;
        }

        let idx_sel = self
            .selecionada
            .and_then(|s| self.itens.iter().position(|(v, _)| *v == s))
            .unwrap_or(0);

        // Posição-alvo da pílula = a do segmento ativo (linha e coluna).
        let alvo = posicoes[idx_sel];
        let id = ui.id().with(("abas-pilula", self.id_salt));
        let x_anim = perseguir(ui, id, alvo.x, Mov::CALMA);
        let y_anim = {
            let id_y = ui.id().with(("abas-linha", self.id_salt));
            perseguir(ui, id_y, alvo.y, Mov::CALMA)
        };
        let larg_anim = {
            let id_w = ui.id().with(("abas-larg", self.id_salt));
            perseguir(ui, id_w, larguras[idx_sel], Mov::CALMA)
        };

        let pilula = Rect::from_min_size(
            egui::pos2(rect.left() + margem + x_anim, rect.top() + margem + y_anim),
            egui::vec2(larg_anim, altura),
        );
        desenhar_fundo(ui, rect, pilula, self.navegacao, pad_x);

        // Segmentos clicáveis.
        let mut clicada = None;
        let origem = rect.min + egui::vec2(margem, margem);
        for (i, (((valor, rotulo), larg), pos)) in
            self.itens.iter().zip(&larguras).zip(&posicoes).enumerate()
        {
            let seg = Rect::from_min_size(origem + *pos, egui::vec2(*larg, altura));
            let resp = ui.interact(seg, id.with(i), Sense::click());
            let ativa = i == idx_sel;
            // A ativa leva a cor da marca; a inativa acende um fundo leve no hover — o
            // segmento diz que é clicável antes do clique.
            if !ativa && resp.hovered() {
                let fundo = if self.navegacao {
                    seg.shrink2(egui::vec2(2.0, 5.0))
                } else {
                    seg.shrink(1.0)
                };
                ui.painter()
                    .rect_filled(fundo, Raio::CAMPO, cores.superficie_hover);
            }
            let cor = if ativa {
                cores.rubro
            } else if resp.hovered() {
                cores.texto_forte
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
        }

        ui.add_space(Espaco::E4);
        clicada
    }
}

/// A posição de cada segmento: vai para a linha seguinte quando passaria de `cabe` (o primeiro de
/// cada linha fica, mesmo que sozinho não caiba). Devolve também a largura da linha mais larga e
/// quantas linhas há.
fn quebrar_em_linhas(larguras: &[f32], cabe: f32, altura: f32) -> (Vec<egui::Vec2>, f32, f32) {
    let mut posicoes = Vec::with_capacity(larguras.len());
    let (mut x, mut linha, mut largura_max) = (0.0_f32, 0.0_f32, 0.0_f32);
    for larg in larguras {
        if x > 0.0 && x + larg > cabe {
            x = 0.0;
            linha += 1.0;
        }
        posicoes.push(egui::vec2(x, linha * altura));
        x += larg;
        largura_max = largura_max.max(x);
    }
    (posicoes, largura_max, linha + 1.0)
}

/// O fundo da barra e o indicador da ativa (`pilula`: o retângulo animado do segmento ativo).
/// Segmentos: trilha cinza e pílula clara; navegação: linha sob a barra e o traço da marca sob
/// o rótulo ativo, na base da linha dele.
fn desenhar_fundo(ui: &Ui, rect: Rect, pilula: Rect, navegacao: bool, pad_x: f32) {
    let cores = ui.cores();
    let pintor = ui.painter();
    if navegacao {
        pintor.hline(
            rect.x_range(),
            rect.bottom() - 0.5,
            egui::Stroke::new(1.0_f32, cores.borda),
        );
        let recuo = pad_x - 4.0;
        let traco = Rect::from_min_max(
            egui::pos2(pilula.left() + recuo, pilula.bottom() - 2.0),
            egui::pos2(pilula.right() - recuo, pilula.bottom()),
        );
        pintor.rect_filled(traco, 1.0, cores.rubro);
    } else {
        pintor.rect_filled(rect, Raio::ITEM, cores.superficie_2);
        pintor.rect_filled(pilula, Raio::CAMPO, cores.superficie);
        pintor.rect_stroke(pilula, Raio::CAMPO, egui::Stroke::new(1.0_f32, cores.borda));
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// O retângulo que a barra ocupa num painel de `largura` px, e a largura que cabia.
    fn ocupado(largura: f32, rotulos: &[&str]) -> (Rect, f32) {
        ocupado_em(largura, rotulos, false)
    }

    fn ocupado_em(largura: f32, rotulos: &[&str], navegacao: bool) -> (Rect, f32) {
        let ctx = egui::Context::default();
        crate::tokens::instalar_fontes(&ctx);
        let entrada = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(largura, 600.0),
            )),
            ..Default::default()
        };
        let itens: Vec<(usize, &str)> = rotulos.iter().copied().enumerate().collect();
        let mut r = (Rect::NOTHING, 0.0);
        let _ = ctx.run(entrada, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let cabe = ui.available_width();
                let usado = ui
                    .scope(|ui| {
                        let abas = Abas::nova(&itens).selecionada(9);
                        if navegacao {
                            abas.navegacao().mostrar(ui)
                        } else {
                            abas.mostrar(ui)
                        }
                    })
                    .response
                    .rect;
                r = (usado, cabe);
            });
        });
        r
    }

    #[test]
    fn cabe_numa_linha_quando_ha_espaco() {
        let (r, _) = ocupado(800.0, &["Lista", "Gráfico"]);
        assert!(r.height() < 60.0, "{r:?}");
    }

    #[test]
    fn quebra_em_linhas_sem_passar_da_largura() {
        let rotulos = [
            "Visão geral",
            "A receber",
            "A pagar",
            "Custos por categoria",
            "Cobrança",
            "Resultado e caixa",
            "Relatórios",
            "Plano de contas",
            "Conciliação",
            "Cheques",
            "Fluxo de caixa",
            "Contas bancárias",
        ];
        let (r, cabe) = ocupado(420.0, &rotulos);
        assert!(r.width() <= cabe + 0.5, "{r:?} > {cabe}");
        assert!(r.height() > 90.0, "deveria ter 3+ linhas: {r:?}");

        // A navegação quebra do mesmo jeito e ocupa a largura toda (a linha sob a barra).
        let (r, cabe) = ocupado_em(420.0, &rotulos, true);
        assert!((r.width() - cabe).abs() < 0.5, "{r:?} != {cabe}");
        assert!(r.height() > 100.0, "deveria ter 3+ linhas: {r:?}");
    }

    #[test]
    fn navegacao_numa_linha_ocupa_a_largura_toda() {
        let (r, cabe) = ocupado_em(800.0, &["Lista", "Gráfico"], true);
        assert!((r.width() - cabe).abs() < 0.5, "{r:?} != {cabe}");
        assert!(r.height() < 60.0, "{r:?}");
    }
}
