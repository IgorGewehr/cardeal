//! Camada 3 (organisms) — `AgendaMes`: a visão mensal do calendário. Complementa
//! [`AgendaCalendario`](super::AgendaCalendario) (dia/semana). `docs/modulos/agenda.md` §6.
//!
//! Grade de 6×7 células de dia; cada célula lista os compromissos como pílulas coloridas
//! (com horário), com "+N" quando não cabem. Clicar no número do dia abre a visão diária;
//! clicar numa pílula abre o compromisso; clicar no vazio propõe um novo às 9h.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::many_single_char_names
)]

use cardeal_kernel::{Data, Fuso, Hora};
use egui::{CursorIcon, Rect, Sense, Ui};

use super::agenda_calendario::{mistura, AcaoAgenda, BlocoAgenda};
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

        let cab = 34.0_f32;
        let largura = ui.available_width().max(1.0);
        let col_w = largura / 7.0;
        let alt_total = ui.available_height().max(460.0_f32);
        let row_h = ((alt_total - cab) / 6.0).max(88.0);

        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(largura, cab + row_h * 6.0), Sense::hover());
        let p = ui.painter_at(rect);
        let mut acao = AcaoAgenda::Nenhuma;

        let linha = mistura(cores.borda, cores.superficie, 0.35);

        // ── Moldura do cartão ─────────────────────────────────────────────
        p.rect(
            rect,
            Raio::CARTAO,
            cores.superficie,
            egui::Stroke::new(1.0_f32, cores.borda),
        );

        // ── Cabeçalho dos dias da semana ──────────────────────────────────
        for (i, nome) in ["SEG", "TER", "QUA", "QUI", "SEX", "SÁB", "DOM"]
            .iter()
            .enumerate()
        {
            p.text(
                egui::pos2(rect.left() + col_w * (i as f32 + 0.5), rect.top() + 12.0),
                egui::Align2::CENTER_TOP,
                *nome,
                Papel::RotuloCampo.font_id(),
                cores.texto_fraco,
            );
        }
        p.hline(
            rect.left()..=rect.right(),
            rect.top() + cab,
            egui::Stroke::new(1.0_f32, cores.borda),
        );

        for semana in 0..6 {
            for dow_i in 0..7 {
                let dia = inicio.mais_dias(semana * 7 + dow_i);
                let x = rect.left() + col_w * dow_i as f32;
                let y = rect.top() + cab + row_h * semana as f32;
                let cel = Rect::from_min_size(egui::pos2(x, y), egui::vec2(col_w, row_h));

                let do_mes = dia.mes() == mes_atual;
                let e_hoje = dia == hoje;

                // Fundo da célula: leve realce em hoje, esmaecimento fora do mês.
                if e_hoje {
                    p.rect_filled(cel, 0.0, cores.rubro_ativo.gamma_multiply(0.55));
                } else if !do_mes {
                    p.rect_filled(cel, 0.0, cores.superficie_2.gamma_multiply(0.35));
                }

                // Número do dia (clicável → abre a visão diária).
                let centro_num = egui::pos2(cel.left() + 16.0, cel.top() + 16.0);
                if e_hoje {
                    p.circle_filled(centro_num, 11.0, cores.rubro);
                }
                p.text(
                    centro_num,
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
                let num_rect = Rect::from_center_size(centro_num, egui::vec2(24.0, 22.0));
                let num_resp = ui.interact(
                    num_rect,
                    ui.id().with(("mes-num", semana, dow_i)),
                    Sense::click(),
                );
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

                let pill_h = 19.0_f32;
                let topo = cel.top() + 32.0;
                let cabem = (((row_h - 36.0) / (pill_h + 3.0)).floor() as usize).max(1);
                let mostrar_n = if do_dia.len() > cabem {
                    cabem - 1
                } else {
                    do_dia.len()
                };

                for (k, b) in do_dia.iter().take(mostrar_n).enumerate() {
                    let py = topo + k as f32 * (pill_h + 3.0);
                    let pill = Rect::from_min_size(
                        egui::pos2(cel.left() + 6.0, py),
                        egui::vec2(col_w - 12.0, pill_h),
                    );
                    let (fill, acc) = super::agenda_calendario::cor_tag_pub(b.tag, &cores);
                    p.rect_filled(pill, Raio::CAMPO, fill);
                    p.circle_filled(egui::pos2(pill.left() + 9.0, pill.center().y), 3.0, acc);
                    let hora = b.inicio.hora(Fuso::BRASILIA);
                    let g = ui.painter().layout(
                        format!("{}  {}", hora.formatar(), b.titulo),
                        Papel::RotuloCampo.font_id(),
                        cores.texto,
                        (pill.width() - 24.0).max(1.0),
                    );
                    p.galley(
                        egui::pos2(pill.left() + 17.0, pill.center().y - g.size().y / 2.0),
                        g,
                        cores.texto,
                    );

                    let r = ui.interact(pill, ui.id().with(("mes-pill", b.id)), Sense::click());
                    if r.hovered() {
                        p.rect_stroke(pill, Raio::CAMPO, egui::Stroke::new(1.0_f32, acc));
                        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    }
                    if r.clicked() {
                        acao = AcaoAgenda::Bloco(b.id.to_owned());
                    }
                }

                if do_dia.len() > mostrar_n {
                    let py = topo + mostrar_n as f32 * (pill_h + 3.0);
                    let etq = Rect::from_min_size(
                        egui::pos2(cel.left() + 6.0, py),
                        egui::vec2(col_w - 12.0, pill_h),
                    );
                    let r = ui.interact(
                        etq,
                        ui.id().with(("mes-mais", semana, dow_i)),
                        Sense::click(),
                    );
                    let cor = if r.hovered() {
                        cores.texto
                    } else {
                        cores.texto_medio
                    };
                    p.text(
                        egui::pos2(etq.left() + 11.0, etq.center().y),
                        egui::Align2::LEFT_CENTER,
                        format!("+{} mais", do_dia.len() - mostrar_n),
                        Papel::RotuloCampo.font_id(),
                        cor,
                    );
                    if r.hovered() {
                        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    }
                    if r.clicked() {
                        acao = AcaoAgenda::AbrirDia(dia);
                    }
                }

                // Clique no vazio da célula → novo compromisso às 9h.
                let resto = Rect::from_min_max(egui::pos2(cel.left(), topo), cel.max);
                let vazio = ui.interact(
                    resto,
                    ui.id().with(("mes-cel", semana, dow_i)),
                    Sense::click(),
                );
                if vazio.clicked() && do_dia.len() <= mostrar_n {
                    if let Ok(hora) = Hora::de_hms(9, 0, 0) {
                        acao = AcaoAgenda::Vazio { data: dia, hora };
                    }
                }
            }
        }

        // ── Grade: apenas as linhas internas, finas ───────────────────────
        for c in 1..7 {
            let x = rect.left() + col_w * c as f32;
            p.vline(
                x,
                (rect.top() + cab)..=(rect.bottom() - 1.0),
                egui::Stroke::new(1.0_f32, linha),
            );
        }
        for r in 1..6 {
            let y = rect.top() + cab + row_h * r as f32;
            p.hline(
                rect.left()..=rect.right(),
                y,
                egui::Stroke::new(1.0_f32, linha),
            );
        }

        acao
    }
}
