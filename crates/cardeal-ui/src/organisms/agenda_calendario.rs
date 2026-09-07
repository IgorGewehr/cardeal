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

        let gutter = 60.0_f32;
        let altura_hora = 56.0_f32;
        let cab = 70.0_f32;
        let n_horas = (self.hora_ate - self.hora_de).max(1);
        let largura_total = ui.available_width().max(1.0);
        let largura_col = ((largura_total - gutter) / dias.len() as f32).max(64.0);
        let altura_total = cab + altura_hora * n_horas as f32 + 8.0;

        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(largura_total, altura_total), Sense::hover());
        let p = ui.painter_at(rect);
        let mut acao = AcaoAgenda::Nenhuma;

        let grade_esq = rect.left() + gutter;
        let corpo_topo = rect.top() + cab;
        let corpo_base = rect.bottom() - 4.0;
        let linha_hora = mistura(cores.borda, cores.superficie, 0.35);
        let linha_meia = mistura(cores.borda, cores.superficie, 0.7);

        // ── Moldura do cartão ─────────────────────────────────────────────
        p.rect(
            rect,
            Raio::CARTAO,
            cores.superficie,
            egui::Stroke::new(1.0_f32, cores.borda),
        );

        // ── Faixas de coluna: hoje e fim de semana ────────────────────────
        for (i, d) in dias.iter().enumerate() {
            let x = grade_esq + largura_col * i as f32;
            let col = Rect::from_min_max(
                egui::pos2(x, corpo_topo),
                egui::pos2(x + largura_col, corpo_base),
            );
            if *d == hoje {
                p.rect_filled(col, 0.0, cores.rubro_ativo.gamma_multiply(0.5));
            } else if dias.len() > 1 && matches!(d.dia_da_semana() as i32, 0 | 6) {
                p.rect_filled(col, 0.0, cores.superficie_2.gamma_multiply(0.4));
            }
        }

        // ── Cabeçalho dos dias ────────────────────────────────────────────
        for (i, d) in dias.iter().enumerate() {
            let cx = grade_esq + largura_col * (i as f32 + 0.5);
            let e_hoje = *d == hoje;

            // "Hoje": um realce arredondado sob a célula inteira do cabeçalho, agrupando o
            // dia da semana e o número — em vez de um círculo solto e apertado.
            if e_hoje {
                let larg = largura_col.min(64.0) - 8.0;
                let realce = Rect::from_min_max(
                    egui::pos2(cx - larg / 2.0, rect.top() + 7.0),
                    egui::pos2(cx + larg / 2.0, corpo_topo - 6.0),
                );
                p.rect_filled(realce, Raio::ITEM, cores.rubro_ativo);
            }

            p.text(
                egui::pos2(cx, rect.top() + 13.0),
                egui::Align2::CENTER_TOP,
                d.dia_da_semana().abreviacao().to_uppercase(),
                Papel::RotuloCampo.font_id(),
                if e_hoje { cores.rubro } else { cores.texto_fraco },
            );
            let centro_num = egui::pos2(cx, rect.top() + 46.0);
            if e_hoje {
                p.circle_filled(centro_num, 12.5, cores.rubro);
            }
            p.text(
                centro_num,
                egui::Align2::CENTER_CENTER,
                d.dia().to_string(),
                Papel::TituloSecao.font_id(),
                if e_hoje { Rubro::CONTRASTE } else { cores.texto_forte },
            );
        }
        p.hline(
            rect.left()..=rect.right(),
            corpo_topo,
            egui::Stroke::new(1.0_f32, cores.borda),
        );

        // ── Linhas de hora + meia-hora ────────────────────────────────────
        let y_de_hora = |h: f32| corpo_topo + (h - self.hora_de as f32) * altura_hora;
        for h in self.hora_de..self.hora_ate {
            let y = y_de_hora(h as f32);
            p.hline(
                grade_esq..=(rect.right() - 1.0),
                y,
                egui::Stroke::new(1.0_f32, linha_hora),
            );
            p.hline(
                grade_esq..=(rect.right() - 1.0),
                y + altura_hora / 2.0,
                egui::Stroke::new(1.0_f32, linha_meia),
            );
            p.text(
                egui::pos2(grade_esq - 10.0, y),
                egui::Align2::RIGHT_CENTER,
                format!("{h:02}:00"),
                Papel::RotuloCampo.font_id(),
                cores.texto_fraco,
            );
        }

        // ── Divisórias verticais de coluna ────────────────────────────────
        for i in 0..=dias.len() {
            let x = grade_esq + largura_col * i as f32;
            p.vline(
                x,
                corpo_topo..=corpo_base,
                egui::Stroke::new(1.0_f32, linha_hora),
            );
        }

        // ── Área clicável de cada célula vazia (30 min) ───────────────────
        for (i, d) in dias.iter().enumerate() {
            for meia in 0..(n_horas * 2) {
                let x = grade_esq + largura_col * i as f32;
                let y = corpo_topo + meia as f32 * (altura_hora / 2.0);
                let cel = Rect::from_min_size(
                    egui::pos2(x, y),
                    egui::vec2(largura_col, altura_hora / 2.0),
                );
                let id = ui.id().with(("agenda-cel", i, meia));
                let r = ui.interact(cel, id, Sense::click());
                if r.hovered() {
                    p.rect_filled(
                        cel.shrink(2.0),
                        Raio::CAMPO,
                        cores.rubro_ativo.gamma_multiply(0.75),
                    );
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
                if r.clicked() {
                    let minutos = self.hora_de * 60 + meia * 30;
                    if let Ok(hora) = Hora::de_hms(minutos / 60, minutos % 60, 0) {
                        acao = AcaoAgenda::Vazio { data: *d, hora };
                    }
                }
            }
        }

        // ── Blocos ────────────────────────────────────────────────────────
        for (i, d) in dias.iter().enumerate() {
            let x0 = grade_esq + largura_col * i as f32;
            let do_dia = layout_do_dia(self.blocos, *d, self.hora_de, self.hora_ate);
            for (b, faixa) in do_dia {
                let ini = b.inicio.hora(Fuso::BRASILIA);
                let fimh = b.fim.hora(Fuso::BRASILIA);
                let f_ini =
                    (ini.hora() as f32 + ini.minuto() as f32 / 60.0).max(self.hora_de as f32);
                let f_fim =
                    (fimh.hora() as f32 + fimh.minuto() as f32 / 60.0).min(self.hora_ate as f32);
                if f_fim <= f_ini {
                    continue;
                }
                let pad = 3.0_f32;
                let bx = x0 + pad + faixa.0 * (largura_col - pad * 2.0);
                let bw = (faixa.1 * (largura_col - pad * 2.0) - 2.0).max(28.0);
                let by = y_de_hora(f_ini) + 1.0;
                let bh = ((f_fim - f_ini) * altura_hora - 2.0).max(20.0);
                let bloco = Rect::from_min_size(egui::pos2(bx, by), egui::vec2(bw, bh));

                let (fill, acc) = cor_tag(b.tag, &cores);
                let esmaecido = matches!(b.tag, TagAgenda::Inativo);
                p.rect_filled(bloco, Raio::CAMPO, fill);
                let barra = Rect::from_min_size(
                    bloco.min + egui::vec2(0.0, 3.0),
                    egui::vec2(3.0, (bloco.height() - 6.0).max(1.0)),
                );
                p.rect_filled(barra, Raio::PILULA, acc);
                if b.conflito {
                    p.rect_stroke(bloco, Raio::CAMPO, egui::Stroke::new(1.5_f32, cores.atencao));
                }

                let cor_titulo = if esmaecido { cores.texto_fraco } else { cores.texto_forte };
                let texto = ui.painter().layout(
                    b.titulo.to_owned(),
                    Papel::Interface.font_id(),
                    cor_titulo,
                    (bw - 16.0).max(1.0),
                );
                p.galley(bloco.min + egui::vec2(9.0, 5.0), texto, cor_titulo);
                if bh > 36.0 {
                    p.text(
                        bloco.min + egui::vec2(9.0, bh - 17.0),
                        egui::Align2::LEFT_TOP,
                        format!("{}–{}", ini.formatar(), fimh.formatar()),
                        Papel::RotuloCampo.font_id(),
                        if esmaecido { cores.texto_fraco } else { acc },
                    );
                }

                let id = ui.id().with(("agenda-bloco", b.id));
                let r = ui.interact(bloco, id, Sense::click());
                if r.hovered() {
                    p.rect_stroke(bloco, Raio::CAMPO, egui::Stroke::new(1.5_f32, acc));
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
                if r.clicked() {
                    acao = AcaoAgenda::Bloco(b.id.to_owned());
                }
            }
        }

        // ── Linha do "agora" ──────────────────────────────────────────────
        if let Some(col) = dias.iter().position(|d| *d == hoje) {
            let h = agora.hora(Fuso::BRASILIA);
            let frac = h.hora() as f32 + h.minuto() as f32 / 60.0;
            if frac >= self.hora_de as f32 && frac <= self.hora_ate as f32 {
                let y = y_de_hora(frac);
                let x0 = grade_esq + largura_col * col as f32;
                p.hline(
                    x0..=(x0 + largura_col),
                    y,
                    egui::Stroke::new(2.0_f32, Rubro::R500),
                );
                p.circle_filled(egui::pos2(x0, y), 4.0, Rubro::R500);
                p.text(
                    egui::pos2(grade_esq - 10.0, y),
                    egui::Align2::RIGHT_CENTER,
                    h.formatar(),
                    Papel::RotuloCampo.font_id(),
                    Rubro::R600,
                );
            }
        }

        acao
    }
}

/// Interpola duas cores em espaço linear (`t` = 0 devolve `a`, `t` = 1 devolve `b`).
#[must_use]
pub(crate) fn mistura(a: Color32, b: Color32, t: f32) -> Color32 {
    let a = egui::Rgba::from(a);
    let b = egui::Rgba::from(b);
    Color32::from(a * (1.0 - t) + b * t)
}

fn cor_tag(tag: TagAgenda, c: &crate::tokens::Cores) -> (Color32, Color32) {
    match tag {
        TagAgenda::Interno => (mistura(c.superficie_2, c.borda, 0.35), c.texto_medio),
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
