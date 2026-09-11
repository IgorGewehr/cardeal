//! Camada 1 (atoms) — `CampoTexto`. `docs/12-ui-ux.md` §7 (base para `CampoMoeda`,
//! `CampoDocumento`, `CampoData`) e §9 (foco sempre visível: anel `rubro-500`).
//!
//! O visual do `TextEdit` (fundo, borda, borda de foco, arredondamento) já vem de
//! [`instalar_estilo`](crate::tokens::instalar_estilo); aqui só montamos rótulo + campo e o
//! preenchimento de largura.

use egui::{Color32, Response, Ui, Vec2, Widget};

use crate::tokens::{ativar, lerp_cor, Mov, Papel, Raio, TemaUi};

/// A margem interna padrão de um campo (`TextEdit`, seletor) — o `frame_rect` do egui é o
/// `rect` da resposta expandido por esta margem. Aumentada em 2026-09-11 (era `(10.0, 8.0)`)
/// junto da escala tipográfica — campo e dropdown ganham a mesma altura de toque confortável.
pub(crate) const MARGEM_CAMPO: Vec2 = Vec2::new(12.0, 10.0);

/// Desenha a **moldura responsiva** de um campo por cima da borda crua do `egui`: no hover a
/// borda ganha contraste; no foco ela vira `acento` (a marca, ou `negativo` num campo com
/// erro) e um halo suave cresce ao redor. `docs/12-ui-ux.md` §9 (foco sempre visível).
///
/// Em repouso não desenha nada — a borda de fábrica do `egui` fica. Só pede repaint enquanto
/// a transição corre (Pilar I).
pub(crate) fn moldura_foco_campo(ui: &Ui, resp: &Response, margem: Vec2, acento: Option<Color32>) {
    let cores = ui.cores();
    let alvo = acento.unwrap_or(cores.rubro);

    let tf = ativar(
        ui,
        resp.id.with("campo-foco"),
        resp.has_focus(),
        Mov::PADRAO,
    );
    let th = ativar(ui, resp.id.with("campo-hover"), resp.hovered(), Mov::RAPIDO);
    if tf <= 0.001_f32 && th <= 0.001_f32 {
        return;
    }

    let frame = resp.rect.expand2(margem);
    if tf > 0.001_f32 {
        ui.painter().rect_stroke(
            frame.expand(3.0_f32),
            Raio::CAMPO + 3.0_f32,
            egui::Stroke::new(4.0_f32, alvo.gamma_multiply(0.14_f32 * tf)),
        );
    }
    let cor = lerp_cor(
        lerp_cor(cores.borda_forte, cores.texto_medio, th * 0.6_f32),
        alvo,
        tf,
    );
    ui.painter()
        .rect_stroke(frame, Raio::CAMPO, egui::Stroke::new(1.0_f32 + tf, cor));
}

/// Um campo de texto de uma linha, com rótulo opcional.
#[must_use]
pub struct CampoTexto<'a> {
    valor: &'a mut String,
    rotulo: Option<String>,
    marcador: Option<String>,
    senha: bool,
    preenche_largura: bool,
}

impl<'a> CampoTexto<'a> {
    /// Um campo vazio, sem rótulo nem marcador.
    pub fn novo(valor: &'a mut String) -> Self {
        Self {
            valor,
            rotulo: None,
            marcador: None,
            senha: false,
            preenche_largura: true,
        }
    }

    /// Rótulo mostrado acima do campo (`docs/12-ui-ux.md` §3, papel `RotuloCampo`).
    pub fn rotulo(mut self, rotulo: impl Into<String>) -> Self {
        self.rotulo = Some(rotulo.into());
        self
    }

    /// Texto de exemplo mostrado quando o campo está vazio.
    pub fn marcador(mut self, marcador: impl Into<String>) -> Self {
        self.marcador = Some(marcador.into());
        self
    }

    /// Mascara o conteúdo digitado (campo de senha).
    pub const fn senha(mut self, v: bool) -> Self {
        self.senha = v;
        self
    }

    /// Se o campo ocupa toda a largura disponível (padrão: sim).
    pub const fn preenche_largura(mut self, v: bool) -> Self {
        self.preenche_largura = v;
        self
    }
}

impl Widget for CampoTexto<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let cores = ui.cores();
        ui.vertical(|ui| {
            if let Some(rotulo) = &self.rotulo {
                ui.add(crate::atoms::Rotulo::campo(rotulo.clone()));
                ui.add_space(crate::tokens::Espaco::E4);
            }

            let mut edicao = egui::TextEdit::singleline(self.valor)
                .password(self.senha)
                .font(Papel::Interface.font_id())
                .text_color(cores.texto)
                .margin(egui::Margin::symmetric(MARGEM_CAMPO.x, MARGEM_CAMPO.y));
            if self.preenche_largura {
                edicao = edicao.desired_width(f32::INFINITY);
            }
            if let Some(marcador) = &self.marcador {
                edicao = edicao.hint_text(marcador.clone());
            }
            let resp = ui.add(edicao);
            moldura_foco_campo(ui, &resp, MARGEM_CAMPO, None);
            resp
        })
        .inner
    }
}
