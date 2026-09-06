//! Camada 3 (organisms) — `AgendaCalendario`: a grade de calendário (dia ou semana) com os
//! compromissos posicionados por horário. `docs/modulos/agenda.md` §6.
//!
//! Genérico sobre o domínio: recebe [`BlocoAgenda`] (id + título + início/fim + cor) e
//! devolve o que foi tocado — um bloco existente ou um espaço vazio (para criar). O cálculo
//! de sobreposição divide a largura da coluna entre blocos concorrentes.

#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_lossless, clippy::many_single_char_names)]

use cardeal_kernel::{Data, Fuso, Hora, Instante};
use egui::{Color32, CursorIcon, Rect, Sense, Ui};

use crate::tokens::{Papel, Raio, Rubro, TemaUi};

/// A cor semântica de um bloco (mapeada para um token do tema).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TagAgenda {
    /// Compromisso interno.
    #[default]
    Interno,
    /// Compromisso com cliente.
    Cliente,
    /// Confirmado.
    Confirmado,
    /// Em andamento agora.
    EmAndamento,
    /// Cancelado / não compareceu — esmaecido.
    Inativo,
}

/// Um compromisso a desenhar na grade.
pub struct BlocoAgenda<'a> {
    /// Id estável, devolvido no clique.
    pub id: &'a str,
    /// Título curto.
    pub titulo: &'a str,
    /// Subtítulo opcional (cliente, recurso).
    pub subtitulo: Option<&'a str>,
    /// Início.
    pub inicio: Instante,
    /// Fim.
    pub fim: Instante,
    /// Cor.
    pub tag: TagAgenda,
    /// Verdadeiro se está em conflito de recurso com outro — ganha contorno de atenção.
    pub conflito: bool,
}

/// O modo da grade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModoCalendario {
    /// Um dia, uma coluna.
    Dia,
    /// Sete dias a partir da segunda-feira da semana de `base`.
    Semana,
    /// Grade mensal — renderizada por [`AgendaMes`](super::AgendaMes).
    Mes,
}

/// O que o usuário tocou na grade.
pub enum AcaoAgenda {
    /// Nada.
    Nenhuma,
    /// Clicou num compromisso.
    Bloco(String),
    /// Clicou num espaço livre — proposta de novo compromisso.
    Vazio {
        /// O dia.
        data: Data,
        /// A hora (arredondada para 30 min).
        hora: Hora,
    },
    /// Clicou no número de um dia na visão mensal — abrir a visão diária dele.
    AbrirDia(Data),
}

/// A cor `(preenchimento, barra)` de um bloco — compartilhada com [`AgendaMes`](super::AgendaMes).
#[must_use]
pub(crate) fn cor_tag_pub(tag: TagAgenda, c: &crate::tokens::Cores) -> (Color32, Color32) {
    cor_tag(tag, c)
}

/// A grade de calendário.
pub struct AgendaCalendario<'a> {
    base: Data,
    modo: ModoCalendario,
    blocos: &'a [BlocoAgenda<'a>],
    hora_de: u32,
    hora_ate: u32,
}

impl<'a> AgendaCalendario<'a> {
    /// Uma grade centrada em `base`, mostrando das 7h às 21h por padrão.
    #[must_use]
    pub const fn nova(base: Data, modo: ModoCalendario, blocos: &'a [BlocoAgenda<'a>]) -> Self {
        Self {
            base,
            modo,
            blocos,
            hora_de: 7,
            hora_ate: 21,
        }
    }

    /// Ajusta a faixa de horas visível.
    #[must_use]
    pub const fn horas(mut self, de: u32, ate: u32) -> Self {
        self.hora_de = de;
        self.hora_ate = ate;
        self
    }

    fn dias(&self) -> Vec<Data> {
        match self.modo {
            ModoCalendario::Dia => vec![self.base],
            ModoCalendario::Semana | ModoCalendario::Mes => {
                // Segunda = 0 … Domingo = 6 (dia_da_semana varia; normalizamos).
                let dow = self.base.dia_da_semana() as i32; // Domingo = 0
                let atras = (dow + 6) % 7;
                let segunda = self.base.mais_dias(-atras);
                (0..7).map(|d| segunda.mais_dias(d)).collect()
            }
        }
    }

    /// Desenha a grade e devolve a interação.
    #[allow(clippy::too_many_lines)]
    pub fn mostrar(self, ui: &mut Ui) -> AcaoAgenda {
        let cores = ui.cores();
        let dias = self.dias();
        let hoje = Data::hoje(Fuso::BRASILIA);
        let agora = Instante::agora();

        let gutter = 54.0_f32;
        let altura_hora = 52.0_f32;
        let cab = 44.0_f32;
        let n_horas = (self.hora_ate - self.hora_de).max(1);
        let largura_total = ui.available_width();
        let largura_col = ((largura_total - gutter) / dias.len() as f32).max(60.0);
        let altura_total = cab + altura_hora * n_horas as f32;

        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(largura_total, altura_total), Sense::hover());
        let p = ui.painter_at(rect);
        let mut acao = AcaoAgenda::Nenhuma;

        // ── Cabeçalho dos dias ─────────────────────────────────────────────
        for (i, d) in dias.iter().enumerate() {
            let x = rect.left() + gutter + largura_col * i as f32;
            let cel = Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(largura_col, cab));
            let e_hoje = *d == hoje;
            if e_hoje {
                p.rect_filled(cel.shrink(2.0), Raio::CAMPO, cores.rubro_ativo);
            }
            let nome = d.dia_da_semana().abreviacao();
            p.text(
                cel.center_top() + egui::vec2(0.0, 8.0),
                egui::Align2::CENTER_TOP,
                format!("{nome} {}", d.dia()),
                Papel::RotuloCampo.font_id(),
                if e_hoje { Rubro::R600 } else { cores.texto_medio },
            );
        }

        // ── Linhas de hora ─────────────────────────────────────────────────
        let y_de_hora = |h: f32| rect.top() + cab + (h - self.hora_de as f32) * altura_hora;
        for h in self.hora_de..=self.hora_ate {
            let y = y_de_hora(h as f32);
            p.hline(
                rect.left()..=rect.right(),
                y,
                egui::Stroke::new(1.0_f32, cores.borda),
            );
            p.text(
                egui::pos2(rect.left() + gutter - 8.0, y),
                egui::Align2::RIGHT_CENTER,
                format!("{h:02}:00"),
                Papel::RotuloCampo.font_id(),
                cores.texto_fraco,
            );
        }
        // Divisórias verticais de coluna.
        for i in 0..=dias.len() {
            let x = rect.left() + gutter + largura_col * i as f32;
            p.vline(
                x,
                (rect.top() + cab)..=rect.bottom(),
                egui::Stroke::new(1.0_f32, cores.borda),
            );
        }

        // ── Área clicável de cada célula vazia (30 min) ────────────────────
        for (i, d) in dias.iter().enumerate() {
            for meia in 0..(n_horas * 2) {
                let x = rect.left() + gutter + largura_col * i as f32;
                let y = rect.top() + cab + meia as f32 * (altura_hora / 2.0);
                let cel = Rect::from_min_size(
                    egui::pos2(x, y),
                    egui::vec2(largura_col, altura_hora / 2.0),
                );
                let id = ui.id().with(("agenda-cel", i, meia));
                let r = ui.interact(cel, id, Sense::click());
                if r.hovered() {
                    ui.painter_at(rect).rect_filled(
                        cel.shrink(1.0),
                        0.0,
                        cores.superficie_hover.gamma_multiply(0.6),
                    );
                    ui.ctx().set_cursor_icon(CursorIcon::Cell);
                }
                if r.clicked() {
                    let minutos = self.hora_de * 60 + meia * 30;
                    if let Ok(hora) = Hora::de_hms(minutos / 60, minutos % 60, 0) {
                        acao = AcaoAgenda::Vazio { data: *d, hora };
                    }
                }
            }
        }

        // ── Linha do "agora" ──────────────────────────────────────────────
        if let Some(col) = dias.iter().position(|d| *d == hoje) {
            let h = agora.hora(Fuso::BRASILIA);
            let frac = h.hora() as f32 + h.minuto() as f32 / 60.0;
            if frac >= self.hora_de as f32 && frac <= self.hora_ate as f32 {
                let y = y_de_hora(frac);
                let x0 = rect.left() + gutter + largura_col * col as f32;
                p.hline(
                    x0..=(x0 + largura_col),
                    y,
                    egui::Stroke::new(2.0_f32, Rubro::R500),
                );
                p.circle_filled(egui::pos2(x0, y), 3.5, Rubro::R500);
            }
        }

        // ── Blocos ────────────────────────────────────────────────────────
        for (i, d) in dias.iter().enumerate() {
            let x0 = rect.left() + gutter + largura_col * i as f32;
            let do_dia = layout_do_dia(self.blocos, *d, self.hora_de, self.hora_ate);
            for (b, faixa) in do_dia {
                let ini = b.inicio.hora(Fuso::BRASILIA);
                let fimh = b.fim.hora(Fuso::BRASILIA);
                let f_ini = (ini.hora() as f32 + ini.minuto() as f32 / 60.0)
                    .max(self.hora_de as f32);
                let f_fim = (fimh.hora() as f32 + fimh.minuto() as f32 / 60.0)
                    .min(self.hora_ate as f32);
                if f_fim <= f_ini {
                    continue;
                }
                let bx = x0 + 2.0 + faixa.0 * (largura_col - 4.0);
                let bw = (faixa.1 * (largura_col - 4.0) - 2.0).max(24.0);
                let by = y_de_hora(f_ini) + 1.0;
                let bh = ((f_fim - f_ini) * altura_hora - 2.0).max(18.0);
                let bloco = Rect::from_min_size(egui::pos2(bx, by), egui::vec2(bw, bh));

                let (fill, borda_cor) = cor_tag(b.tag, &cores);
                p.rect_filled(bloco, Raio::CAMPO, fill);
                let barra = Rect::from_min_size(bloco.min, egui::vec2(3.0, bloco.height()));
                p.rect_filled(barra, 0.0, borda_cor);
                if b.conflito {
                    p.rect_stroke(bloco, Raio::CAMPO, egui::Stroke::new(1.5_f32, cores.atencao));
                }

                let texto = ui.painter().layout(
                    b.titulo.to_owned(),
                    Papel::Interface.font_id(),
                    cores.texto_forte,
                    bw - 12.0,
                );
                p.galley(bloco.min + egui::vec2(8.0, 4.0), texto, cores.texto_forte);
                if bh > 34.0 {
                    p.text(
                        bloco.min + egui::vec2(8.0, bh - 16.0),
                        egui::Align2::LEFT_TOP,
                        format!("{}–{}", ini.formatar(), fimh.formatar()),
                        Papel::RotuloCampo.font_id(),
                        cores.texto_medio,
                    );
                }

                let id = ui.id().with(("agenda-bloco", b.id));
                let r = ui.interact(bloco, id, Sense::click());
                if r.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
                if r.clicked() {
                    acao = AcaoAgenda::Bloco(b.id.to_owned());
                }
            }
        }        acao
    }
}

fn cor_tag(tag: TagAgenda, c: &crate::tokens::Cores) -> (Color32, Color32) {
    match tag {
        TagAgenda::Interno => (c.superficie_2, c.texto_fraco),
        TagAgenda::Cliente => (c.info_suave, c.info),
        TagAgenda::Confirmado => (c.positivo_suave, c.positivo),
        TagAgenda::EmAndamento => (c.rubro_ativo, c.rubro),
        TagAgenda::Inativo => (c.superficie_2.gamma_multiply(0.6), c.texto_fraco),
    }
}

type Faixa = (f32, f32); // (offset, largura) em fração da coluna

/// Distribui os blocos de um dia em faixas horizontais para não se sobreporem visualmente.
fn layout_do_dia<'a>(
    blocos: &'a [BlocoAgenda<'a>],
    dia: Data,
    hora_de: u32,
    hora_ate: u32,
) -> Vec<(&'a BlocoAgenda<'a>, Faixa)> {
    let mut do_dia: Vec<&BlocoAgenda> = blocos
        .iter()
        .filter(|b| {
            b.inicio.data(Fuso::BRASILIA) == dia
                && {
                    let h = b.inicio.hora(Fuso::BRASILIA).hora();
                    h < hora_ate
                }
                && b.fim.hora(Fuso::BRASILIA).hora() >= hora_de
        })
        .collect();
    do_dia.sort_by_key(|b| b.inicio.em_micros());

    // Agrupa em "clusters" que se tocam; dentro do cluster, colunas gulosas.
    let mut resultado = Vec::new();
    let mut i = 0;
    while i < do_dia.len() {
        let mut fim_cluster = do_dia[i].fim.em_micros();
        let inicio_cluster = i;
        let mut j = i + 1;
        while j < do_dia.len() && do_dia[j].inicio.em_micros() < fim_cluster {
            fim_cluster = fim_cluster.max(do_dia[j].fim.em_micros());
            j += 1;
        }
        let cluster = &do_dia[inicio_cluster..j];
        let mut colunas: Vec<i64> = Vec::new(); // fim de cada coluna
        let mut col_de: Vec<usize> = Vec::with_capacity(cluster.len());
        for b in cluster {
            let livre = colunas.iter().position(|&f| f <= b.inicio.em_micros());
            let c = livre.unwrap_or_else(|| {
                colunas.push(0);
                colunas.len() - 1
            });
            colunas[c] = b.fim.em_micros();
            col_de.push(c);
        }
        let n = colunas.len().max(1) as f32;
        for (k, b) in cluster.iter().enumerate() {
            let c = col_de[k] as f32;
            resultado.push((*b, (c / n, 1.0 / n)));
        }
        i = j;
    }
    resultado
}
