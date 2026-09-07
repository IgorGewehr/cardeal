//! Camada 1 (atoms) — `Botao`. `docs/12-ui-ux.md` §7.
//!
//! Resolve a queixa central do anti-exemplo (Digisat): lá um "botão" é texto azul sublinhado
//! sem estado visual. Aqui todo botão tem preenchimento ou contorno reais, muda em
//! hover/pressionado/desabilitado, troca o cursor para a mãozinha e mostra o anel de foco de
//! teclado — `docs/12-ui-ux.md` §9.
//!
//! É um `Widget`: `ui.add(Botao::primario("Finalizar venda").atalho("F2"))`.

use egui::{Color32, CursorIcon, Response, Sense, Stroke, Ui, Vec2, Widget};

use crate::atoms::Spinner;
use crate::tokens::{Papel, Raio, Rubro, TemaUi};

/// A variante visual do botão — `docs/12-ui-ux.md` §7 e §2.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarianteBotao {
    /// Preenchido em `rubro-500`. A ação principal da tela — no máximo um por contexto.
    Primario,
    /// Contorno neutro, fundo transparente. A ação secundária.
    Secundario,
    /// Sem contorno nem preenchimento em repouso; ganha fundo sutil só no hover.
    Fantasma,
    /// Contorno `rubro-700`, nunca preenchido — para que "confirmar" nunca se confunda com
    /// "excluir" (`docs/12-ui-ux.md` §2.3, regra crítica).
    Destrutivo,
}

/// Um botão do design system Rubro.
// `habilitado`/`preenche_largura`/`pequeno`/`carregando` são flags de aparência
// independentes, não um enum de estado disfarçado.
#[allow(clippy::struct_excessive_bools)]
#[must_use]
pub struct Botao {
    rotulo: String,
    variante: VarianteBotao,
    atalho: Option<String>,
    habilitado: bool,
    preenche_largura: bool,
    pequeno: bool,
    carregando: bool,
}

impl Botao {
    /// Botão primário (`rubro-500`) — a ação principal da tela.
    pub fn primario(rotulo: impl Into<String>) -> Self {
        Self::nova(rotulo, VarianteBotao::Primario)
    }

    /// Botão secundário (contorno neutro).
    pub fn secundario(rotulo: impl Into<String>) -> Self {
        Self::nova(rotulo, VarianteBotao::Secundario)
    }

    /// Botão fantasma (sem contorno em repouso).
    pub fn fantasma(rotulo: impl Into<String>) -> Self {
        Self::nova(rotulo, VarianteBotao::Fantasma)
    }

    /// Botão destrutivo (contorno `rubro-700`, nunca preenchido).
    pub fn destrutivo(rotulo: impl Into<String>) -> Self {
        Self::nova(rotulo, VarianteBotao::Destrutivo)
    }

    fn nova(rotulo: impl Into<String>, variante: VarianteBotao) -> Self {
        Self {
            rotulo: rotulo.into(),
            variante,
            atalho: None,
            habilitado: true,
            preenche_largura: false,
            pequeno: false,
            carregando: false,
        }
    }

    /// Anota o atalho de teclado ao lado do rótulo (ex.: `F2`) — `docs/12-ui-ux.md` §8.
    pub fn atalho(mut self, tecla: impl Into<String>) -> Self {
        self.atalho = Some(tecla.into());
        self
    }

    /// Desabilita o botão — sem hover/clique, aparência esmaecida.
    pub const fn habilitado(mut self, v: bool) -> Self {
        self.habilitado = v;
        self
    }

    /// Faz o botão ocupar toda a largura disponível (formulários, gaveta).
    pub const fn preenche_largura(mut self) -> Self {
        self.preenche_largura = true;
        self
    }

    /// Versão compacta (~34px de altura) — barras de ferramentas densas, controles
    /// segmentados, navegação de calendário. Não usar como ação principal de formulário.
    pub const fn pequeno(mut self) -> Self {
        self.pequeno = true;
        self
    }

    /// Estado de carregamento: troca o rótulo por um spinner e desabilita o clique,
    /// mantendo o preenchimento da variante (não fica com cara de desabilitado).
    pub const fn carregando(mut self, v: bool) -> Self {
        self.carregando = v;
        self
    }

    fn paleta(&self, resp: &Response, cores: &crate::tokens::Cores) -> (Option<Color32>, Option<Color32>, Color32) {
        if !self.habilitado {
            return match self.variante {
                VarianteBotao::Primario => (Some(Rubro::R500.gamma_multiply(0.4)), None, Rubro::CONTRASTE.gamma_multiply(0.7)),
                VarianteBotao::Secundario => (None, Some(cores.borda_forte.gamma_multiply(0.5)), cores.texto_fraco),
                VarianteBotao::Fantasma => (None, None, cores.texto_fraco),
                VarianteBotao::Destrutivo => (None, Some(cores.negativo.gamma_multiply(0.4)), cores.negativo.gamma_multiply(0.5)),
            };
        }

        let hover = resp.hovered() && !self.carregando;
        let pressed = resp.is_pointer_button_down_on() && !self.carregando;

        match self.variante {
            VarianteBotao::Primario => {
                let fundo = if pressed {
                    Rubro::R600
                } else if hover {
                    Rubro::R400
                } else {
                    Rubro::R500
                };
                (Some(fundo), None, Rubro::CONTRASTE)
            }
            VarianteBotao::Secundario => {
                let fundo = (hover || pressed).then_some(cores.superficie_2);
                (fundo, Some(cores.borda_forte), cores.texto)
            }
            VarianteBotao::Fantasma => {
                let fundo = (hover || pressed).then_some(cores.superficie_2);
                (fundo, None, cores.texto)
            }
            VarianteBotao::Destrutivo => {
                let fundo = (hover || pressed).then_some(cores.negativo_suave);
                (fundo, Some(cores.negativo), cores.negativo)
            }
        }
    }
}

impl Widget for Botao {
    fn ui(self, ui: &mut Ui) -> Response {
        let cores = ui.cores();
        let fonte = Papel::Interface.font_id();
        let texto = match &self.atalho {
            Some(a) => format!("{}    {a}", self.rotulo),
            None => self.rotulo.clone(),
        };
        let galley = ui
            .painter()
            .layout_no_wrap(texto, fonte, Color32::PLACEHOLDER);

        let (padding, altura_min) = if self.pequeno {
            (Vec2::new(12.0_f32, 6.0_f32), 34.0_f32)
        } else {
            (Vec2::new(20.0_f32, 11.0_f32), 44.0_f32)
        };
        let largura = if self.preenche_largura {
            ui.available_width()
        } else {
            galley.size().x + padding.x * 2.0
        };
        let tamanho = Vec2::new(largura, altura_min.max(galley.size().y + padding.y * 2.0));

        let interativo = self.habilitado && !self.carregando;
        let sense = if interativo {
            Sense::click()
        } else {
            Sense::hover()
        };
        let (rect_total, resp) = ui.allocate_exact_size(tamanho, sense);

        if ui.is_rect_visible(rect_total) {
            // "press-scale": encolhe 1px enquanto pressionado (referência gestao-raiz).
            let rect = if resp.is_pointer_button_down_on() {
                rect_total.shrink(1.0_f32)
            } else {
                rect_total
            };
            let (fill, stroke, fg) = self.paleta(&resp, &cores);
            {
                let painter = ui.painter();
                if let Some(f) = fill {
                    painter.rect_filled(rect, Raio::ITEM, f);
                }
                if let Some(s) = stroke {
                    painter.rect_stroke(rect, Raio::ITEM, Stroke::new(1.0_f32, s));
                }
                if resp.has_focus() {
                    painter.rect_stroke(
                        rect.expand(2.0_f32),
                        Raio::ITEM + 2.0_f32,
                        Stroke::new(2.0_f32, Rubro::R500),
                    );
                }
            }
            if self.carregando {
                let mut filho = ui.new_child(
                    egui::UiBuilder::new().max_rect(rect).layout(
                        egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                    ),
                );
                filho.add(Spinner::novo().pequeno().cor(fg));
            } else {
                let pos = rect.center() - galley.size() / 2.0;
                ui.painter().galley(pos, galley, fg);
            }
        }

        if interativo && resp.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        resp
    }
}
