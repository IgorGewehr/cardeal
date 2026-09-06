//! Camada 3 (organisms) — `AgendaMes`: a visão mensal do calendário. Complementa
//! [`AgendaCalendario`](super::AgendaCalendario) (dia/semana). `docs/modulos/agenda.md` §6.
//!
//! Grade de 6×7 células de dia; cada célula lista os compromissos como pílulas coloridas
//! (com horário), com "+N" quando não cabem. Clicar no número do dia abre a visão diária;
//! clicar numa pílula abre o compromisso; clicar no vazio propõe um novo às 9h.

#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::many_single_char_names)]

use cardeal_kernel::{Data, Fuso, Hora};
use egui::{CursorIcon, Rect, Sense, Ui};

use super::agenda_calendario::{AcaoAgenda, BlocoAgenda};
use crate::tokens::{Papel, Raio, Rubro, TemaUi};

/// A grade mensal.
pub struct AgendaMes<'a> {
    /// Qualquer dia do mês a exibir.
    base: Data,
    blocos: &'a [BlocoAgenda<'a>],
}

impl<'a> AgendaMes<'a> {
    /// Uma grade do mês de `base`.
    #[must_use]
    pub const fn nova(base: Data, blocos: &'a [BlocoAgenda<'a>]) -> Self {
        Self { base, blocos }
    }

    /// Desenha e devolve a interação.
    #[allow(clippy::too_many_lines)]
    pub fn mostrar(self, ui: &mut Ui) -> AcaoAgenda {
        let cores = ui.cores();
        let hoje = Data::hoje(Fuso::BRASILIA);
        let primeiro = self.base.inicio_do_mes();
        let mes_atual = self.base.mes();

        // Começa na segunda-feira on-or-before o dia 1.
        let dow = primeiro.dia_da_semana() as i32; // Domingo = 0
        let inicio = primeiro.mais_dias(-((dow + 6) % 7));

        let cab = 30.0_f32;
        let largura = ui.available_width().max(1.0);
        let col_w = largura / 7.0;
        let alt_total = ui.available_height().max(420.0_f32);
        let row_h = ((alt_total - cab) / 6.0).max(72.0);

        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(largura, cab + row_h * 6.0),
            Sense::hover(),
        );
        let p = ui.painter_at(rect);
        let mut acao = AcaoAgenda::Nenhuma;

        // Cabeçalho dos dias da semana.
        for (i, nome) in ["Seg", "Ter", "Qua", "Qui", "Sex", "Sáb", "Dom"].iter().enumerate() {
            p.text(
                egui::pos2(rect.left() + col_w * i as f32 + 8.0, rect.top() + 8.0),
                egui::Align2::LEFT_TOP,
                *nome,
                Papel::RotuloCampo.font_id(),
                cores.texto_fraco,
            );
        }

        for semana in 0..6 {
            for dow_i in 0..7 {
                let dia = inicio.mais_dias(semana * 7 + dow_i);
                let x = rect.left() + col_w * dow_i as f32;
                let y = rect.top() + cab + row_h * semana as f32;
                let cel = Rect::from_min_size(egui::pos2(x, y), egui::vec2(col_w, row_h));

                let do_mes = dia.mes() == mes_atual;
                let e_hoje = dia == hoje;

                // Fundo e borda da célula.
                p.rect_stroke(cel, 0.0, egui::Stroke::new(1.0_f32, cores.borda));
                if !do_mes {
                    p.rect_filled(cel, 0.0, cores.superficie_2.gamma_multiply(0.5));
                }

                // Número do dia (clicável → abre a visão diária).
                let num_rect = Rect::from_min_size(cel.min + egui::vec2(4.0, 4.0), egui::vec2(26.0, 20.0));
                if e_hoje {
                    p.circle_filled(num_rect.center() + egui::vec2(2.0, 1.0), 11.0, Rubro::R500);
                }
                p.text(
                    num_rect.center() + egui::vec2(2.0, 1.0),
                    egui::Align2::CENTER_CENTER,
                    dia.dia().to_string(),
                    Papel::RotuloCampo.font_id(),
                    if e_hoje {
                        Rubro::CONTRASTE
                    } else if do_mes {
                        cores.texto
                    } else {
                        cores.texto_fraco
                    },
                );
                let num_resp = ui.interact(num_rect, ui.id().with(("mes-num", semana, dow_i)), Sense::click());
                if num_resp.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
                if num_resp.clicked() {
                    acao = AcaoAgenda::AbrirDia(dia);
                }

                // Pílulas dos compromissos do dia.
                let mut do_dia: Vec<&BlocoAgenda> = self
                    .blocos
                    .iter()
                    .filter(|b| b.inicio.data(Fuso::BRASILIA) == dia)
                    .collect();
                do_dia.sort_by_key(|b| b.inicio.em_micros());

                let pill_h = 17.0_f32;
                let cabem = (((row_h - 28.0) / (pill_h + 2.0)).floor() as usize).max(1);
                let mostrar_n = do_dia.len().min(if do_dia.len() > cabem { cabem - 1 } else { cabem });

                for (k, b) in do_dia.iter().take(mostrar_n).enumerate() {
                    let py = cel.top() + 26.0 + k as f32 * (pill_h + 2.0);
                    let pill = Rect::from_min_size(
                        egui::pos2(cel.left() + 4.0, py),
                        egui::vec2(col_w - 8.0, pill_h),
                    );
                    let (fill, barra) = super::agenda_calendario::cor_tag_pub(b.tag, &cores);
                    p.rect_filled(pill, Raio::CAMPO, fill);
                    p.rect_filled(
                        Rect::from_min_size(pill.min, egui::vec2(2.5, pill.height())),
                        0.0,
                        barra,
                    );
                    let hora = b.inicio.hora(Fuso::BRASILIA);
                    let g = ui.painter().layout(
                        format!("{} {}", hora.formatar(), b.titulo),
                        Papel::RotuloCampo.font_id(),
                        cores.texto_forte,
                        (pill.width() - 8.0).max(1.0),
                    );
                    p.galley(pill.min + egui::vec2(6.0, 2.0), g, cores.texto_forte);

                    let r = ui.interact(pill, ui.id().with(("mes-pill", b.id)), Sense::click());
                    if r.hovered() {
                        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    }
                    if r.clicked() {
                        acao = AcaoAgenda::Bloco(b.id.to_owned());
                    }
                }

                if do_dia.len() > mostrar_n {
                    let py = cel.top() + 26.0 + mostrar_n as f32 * (pill_h + 2.0);
                    p.text(
                        egui::pos2(cel.left() + 8.0, py),
                        egui::Align2::LEFT_TOP,
                        format!("+ mais {}", do_dia.len() - mostrar_n),
                        Papel::RotuloCampo.font_id(),
                        cores.texto_medio,
                    );
                }

                // Clique no vazio da célula → novo compromisso às 9h.
                let resto = Rect::from_min_max(
                    egui::pos2(cel.left(), cel.top() + 26.0),
                    cel.max,
                );
                let vazio = ui.interact(resto, ui.id().with(("mes-cel", semana, dow_i)), Sense::click());
                if vazio.clicked() && do_dia.len() <= mostrar_n {
                    if let Ok(hora) = Hora::de_hms(9, 0, 0) {
                        acao = AcaoAgenda::Vazio { data: dia, hora };
                    }
                }
            }
        }

        acao
    }
}
