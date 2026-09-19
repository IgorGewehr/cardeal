//! Camada 1 (atoms) — `Botao`. `docs/12-ui-ux.md` §7.
//!
//! Resolve a queixa central do anti-exemplo (Digisat): lá um "botão" é texto azul sublinhado
//! sem estado visual. Aqui todo botão tem preenchimento ou contorno reais, muda em
//! hover/pressionado/desabilitado, troca o cursor para a mãozinha e mostra o anel de foco de
//! teclado — `docs/12-ui-ux.md` §9.
//!
//! É um `Widget`: `ui.add(Botao::primario("Finalizar venda").atalho("F2"))`.

use egui::{
    Color32, CursorIcon, Key, KeyboardShortcut, Modifiers, Rect, Response, Sense, Stroke, Ui, Vec2,
    Widget,
};

use crate::atoms::Spinner;
use crate::tokens::{ativar, lerp_cor, modal_aberto, Mov, Papel, Raio, Rubro, TemaUi};

/// `Ctrl+N` (`Cmd+N` no macOS) — "novo": o atalho de criar em toda tela de listagem.
pub const ATALHO_NOVO: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::N);

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
    tecla: Option<KeyboardShortcut>,
    habilitado: bool,
    preenche_largura: bool,
    pequeno: bool,
    carregando: bool,
    cor: Option<Color32>,
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
            tecla: None,
            habilitado: true,
            preenche_largura: false,
            pequeno: false,
            carregando: false,
            cor: None,
        }
    }

    /// Sobrescreve a cor do texto (e do contorno, nas variantes que têm) — pensado pra
    /// `Fantasma`/`Secundario` ganharem um destaque de marca sem virar `Primario`/
    /// `Destrutivo` (ex.: "Editar" numa linha de grade, na cor `rubro` em vez do texto
    /// padrão). Não mexe no preenchimento, então não força a variante a parecer outra coisa.
    pub const fn cor(mut self, cor: Color32) -> Self {
        self.cor = Some(cor);
        self
    }

    /// Anota o atalho de teclado ao lado do rótulo (ex.: `F2`) — `docs/12-ui-ux.md` §8.
    pub fn atalho(mut self, tecla: impl Into<String>) -> Self {
        self.atalho = Some(tecla.into());
        self
    }

    /// Liga o botão a uma tecla: mostra o atalho ao lado do rótulo **e** o trata — apertar a
    /// tecla é o mesmo que clicar (`.clicked()` devolve `true`). É o jeito de anotar um atalho
    /// que a tela não trata em outro lugar: o rótulo nunca promete o que não funciona. A tecla
    /// não dispara com o botão desabilitado nem com um diálogo aberto por cima (reabriria o
    /// formulário e apagaria o que o usuário digitava).
    pub const fn tecla(mut self, atalho: KeyboardShortcut) -> Self {
        self.tecla = Some(atalho);
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

    /// A paleta desabilitada — estática, sem hover/press/foco.
    fn paleta_inerte(
        &self,
        cores: &crate::tokens::Cores,
    ) -> (Option<Color32>, Option<Stroke>, Color32) {
        match self.variante {
            VarianteBotao::Primario => (
                Some(Rubro::R500.gamma_multiply(0.4_f32)),
                None,
                Rubro::CONTRASTE.gamma_multiply(0.7_f32),
            ),
            VarianteBotao::Secundario => (
                None,
                Some(Stroke::new(
                    1.0_f32,
                    cores.borda_forte.gamma_multiply(0.5_f32),
                )),
                cores.texto_fraco,
            ),
            VarianteBotao::Fantasma => (None, None, cores.texto_fraco),
            VarianteBotao::Destrutivo => (
                None,
                Some(Stroke::new(1.0_f32, cores.negativo.gamma_multiply(0.4_f32))),
                cores.negativo.gamma_multiply(0.5_f32),
            ),
        }
    }

    /// A paleta viva: interpola repouso → hover → pressionado por `th`/`tp` (0..=1). O
    /// `th`/`tp` vêm de [`ativar`], então a cor **desliza** entre estados em vez de cortar.
    fn paleta_viva(
        &self,
        cores: &crate::tokens::Cores,
        th: f32,
        tp: f32,
    ) -> (Option<Color32>, Option<Stroke>, Color32) {
        match self.variante {
            VarianteBotao::Primario => {
                let fundo = lerp_cor(lerp_cor(Rubro::R500, Rubro::R400, th), Rubro::R600, tp);
                (Some(fundo), None, Rubro::CONTRASTE)
            }
            VarianteBotao::Secundario => {
                let realce = th.max(tp);
                let fundo = cores.superficie_2.gamma_multiply(realce);
                let borda = lerp_cor(cores.borda_forte, cores.texto_fraco, realce);
                (
                    Some(fundo),
                    Some(Stroke::new(1.0_f32 + 0.25_f32 * realce, borda)),
                    cores.texto,
                )
            }
            VarianteBotao::Fantasma => {
                let fundo = cores.superficie_2.gamma_multiply(th.max(tp));
                (Some(fundo), None, cores.texto)
            }
            VarianteBotao::Destrutivo => {
                let realce = th.max(tp);
                let fundo = cores.negativo_suave.gamma_multiply(realce);
                (
                    Some(fundo),
                    Some(Stroke::new(1.0_f32 + 0.4_f32 * realce, cores.negativo)),
                    cores.negativo,
                )
            }
        }
    }

    /// O atalho a mostrar ao lado do rótulo: o texto dado por `.atalho()` ou a tecla de `.tecla()`.
    fn dica_de_atalho(&self, ui: &Ui) -> Option<String> {
        self.atalho
            .clone()
            .or_else(|| self.tecla.map(|t| ui.ctx().format_shortcut(&t)))
    }

    /// Se a tecla ligada ao botão foi apertada neste quadro (e vale: botão ativo, sem modal por
    /// cima). Consome a tecla — nenhum outro widget a vê.
    fn ativado_por_tecla(&self, ui: &Ui, interativo: bool) -> bool {
        self.tecla.is_some_and(|t| {
            interativo && !modal_aberto(ui.ctx()) && ui.input_mut(|i| i.consume_shortcut(&t))
        })
    }

    /// Verdadeiro para as variantes que "levantam" de leve no hover (só a ação de peso).
    const fn levanta_no_hover(&self) -> bool {
        matches!(
            self.variante,
            VarianteBotao::Primario | VarianteBotao::Destrutivo
        )
    }
}

impl Widget for Botao {
    fn ui(self, ui: &mut Ui) -> Response {
        let cores = ui.cores();
        let fonte = Papel::Interface.font_id();
        let texto = match &self.dica_de_atalho(ui) {
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
        let (rect_total, mut resp) = ui.allocate_exact_size(tamanho, sense);

        if ui.is_rect_visible(rect_total) {
            // Transições de estado: o hover acende, o press afunda, o foco cresce — cada um
            // deslizando entre 0 e 1 em vez de cortar. `ativar` só pede repaint enquanto o
            // valor está em movimento (Pilar I).
            let (th, tp, tf) = if interativo {
                (
                    ativar(ui, resp.id.with("hover"), resp.hovered(), Mov::RAPIDO),
                    ativar(
                        ui,
                        resp.id.with("press"),
                        resp.is_pointer_button_down_on(),
                        Mov::RAPIDO,
                    ),
                    ativar(ui, resp.id.with("foco"), resp.has_focus(), Mov::PADRAO),
                )
            } else {
                (0.0, 0.0, 0.0)
            };

            // press-scale (afunda 3%) + hover-lift (sobe 1%, só a ação de peso) —
            // `gestao-raiz`: `whileTap .97`, `whileHover 1.01`.
            let lift = if self.levanta_no_hover() {
                0.01_f32
            } else {
                0.0_f32
            };
            let escala = 1.0_f32 + lift * th - 0.03_f32 * tp;
            let rect = Rect::from_center_size(rect_total.center(), rect_total.size() * escala);

            let (fill, stroke, fg) = if interativo {
                self.paleta_viva(&cores, th, tp)
            } else if self.habilitado {
                // Carregando, mas habilitado: mantém o preenchimento cheio da variante —
                // só o clique fica desabilitado, não a cor (`carregando` não é `desabilitado`).
                self.paleta_viva(&cores, 0.0_f32, 0.0_f32)
            } else {
                self.paleta_inerte(&cores)
            };
            let fg = self
                .cor
                .map_or(fg, |c| if self.habilitado { c } else { fg });
            {
                let painter = ui.painter();
                // Anel de foco de teclado: um halo externo suave + o anel nítido, ambos
                // surgindo com `tf` (`docs/12-ui-ux.md` §9 — foco sempre visível).
                if tf > 0.001_f32 {
                    painter.rect_stroke(
                        rect.expand(4.0_f32),
                        Raio::ITEM + 4.0_f32,
                        Stroke::new(4.0_f32, Rubro::R500.gamma_multiply(0.18_f32 * tf)),
                    );
                    painter.rect_stroke(
                        rect.expand(2.0_f32),
                        Raio::ITEM + 2.0_f32,
                        Stroke::new(2.0_f32, Rubro::R500.gamma_multiply(tf)),
                    );
                }
                if let Some(f) = fill {
                    painter.rect_filled(rect, Raio::ITEM, f);
                }
                if let Some(s) = stroke {
                    painter.rect_stroke(rect, Raio::ITEM, s);
                }
            }
            if self.carregando {
                let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(
                    egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                ));
                filho.add(Spinner::novo().pequeno().cor(fg));
            } else {
                let pos = rect.center() - galley.size() / 2.0;
                ui.painter().galley(pos, galley, fg);
            }
        }

        if interativo && resp.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        if self.ativado_por_tecla(ui, interativo) {
            resp.fake_primary_click = true;
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

    /// Roda um quadro com um botão `tecla(Ctrl+N)` e devolve se ele foi "clicado".
    fn quadro(
        ctx: &egui::Context,
        eventos: Vec<egui::Event>,
        habilitado: bool,
        modal: bool,
    ) -> bool {
        let entrada = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            events: eventos,
            ..Default::default()
        };
        let mut clicado = false;
        let _ = ctx.run(entrada, |ctx| {
            if modal {
                crate::tokens::marcar_modal(ctx);
            }
            egui::CentralPanel::default().show(ctx, |ui| {
                clicado = ui
                    .add(
                        Botao::primario("Novo")
                            .tecla(ATALHO_NOVO)
                            .habilitado(habilitado),
                    )
                    .clicked();
            });
        });
        clicado
    }

    fn ctrl_n() -> Vec<egui::Event> {
        vec![egui::Event::Key {
            key: Key::N,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        }]
    }

    #[test]
    fn a_tecla_equivale_ao_clique() {
        let ctx = contexto();
        assert!(!quadro(&ctx, vec![], true, false), "sem tecla, sem clique");
        assert!(quadro(&ctx, ctrl_n(), true, false), "Ctrl+N clica o botão");
    }

    #[test]
    fn a_tecla_nao_dispara_com_o_botao_desabilitado() {
        let ctx = contexto();
        assert!(!quadro(&ctx, ctrl_n(), false, false));
    }

    #[test]
    fn a_tecla_nao_dispara_com_um_modal_aberto() {
        // Reabrir o formulário por baixo de um diálogo apagaria o que o usuário digita.
        let ctx = contexto();
        assert!(!quadro(&ctx, ctrl_n(), true, true));
        // O modal fechou: no quadro seguinte ao último desenho dele a tecla ainda é
        // ignorada; um quadro depois, volta a valer.
        assert!(!quadro(&ctx, ctrl_n(), true, false));
        assert!(quadro(&ctx, ctrl_n(), true, false));
    }
}
