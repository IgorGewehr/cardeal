//! Tela de Agenda — calendário de compromissos. `docs/modulos/agenda.md`.
//!
//! **Backend ainda não existe** (`mod-agenda` é stub). Esta tela roda sobre um modelo em
//! memória (`Compromisso` local) para validar a UI/UX. Quando `mod-agenda` chegar, os pontos
//! marcados `// BACKEND` viram `motor.consultar` / `motor.executar` e o modelo local sai —
//! os tipos aqui espelham §3/§4 do doc de propósito.

use cardeal_kernel::{Data, Fuso, Hora, Instante};
use cardeal_ui::atoms::{Botao, Rotulo};
use cardeal_ui::molecules::{Campo, Mascara, SeletorOpcao};
use cardeal_ui::organisms::{
    AcaoAgenda, AgendaCalendario, BlocoAgenda, Dialogo, LayoutTela, ModoCalendario, TagAgenda,
};
use cardeal_ui::tokens::{Espaco, TemaUi};
use eframe::egui;

const FUSO: Fuso = Fuso::BRASILIA;

/// O estado de um compromisso (`docs/modulos/agenda.md` §3).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Estado {
    Agendado,
    Confirmado,
    EmAndamento,
    Concluido,
    Cancelado,
    NaoCompareceu,
}

impl Estado {
    const fn rotulo(self) -> &'static str {
        match self {
            Self::Agendado => "Agendado",
            Self::Confirmado => "Confirmado",
            Self::EmAndamento => "Em andamento",
            Self::Concluido => "Concluído",
            Self::Cancelado => "Cancelado",
            Self::NaoCompareceu => "Não compareceu",
        }
    }
}

#[derive(Clone)]
struct Compromisso {
    id: String,
    titulo: String,
    cliente: String,
    recurso: String,
    inicio: Instante,
    fim: Instante,
    estado: Estado,
}

impl Compromisso {
    fn tag(&self, conflito: bool) -> TagAgenda {
        if conflito {
            return TagAgenda::EmAndamento;
        }
        match self.estado {
            Estado::Cancelado | Estado::NaoCompareceu => TagAgenda::Inativo,
            Estado::EmAndamento => TagAgenda::EmAndamento,
            Estado::Confirmado | Estado::Concluido => TagAgenda::Confirmado,
            Estado::Agendado if !self.cliente.trim().is_empty() => TagAgenda::Cliente,
            Estado::Agendado => TagAgenda::Interno,
        }
    }
}

#[derive(Default)]
enum Dlg {
    #[default]
    Fechado,
    Novo(FormComp),
    Ver(String),
}

#[derive(Default, Clone)]
struct FormComp {
    id: Option<String>,
    titulo: String,
    cliente: String,
    recurso: Option<usize>,
    data: String,
    hora_ini: String,
    hora_fim: String,
}

/// Estado local da tela.
pub struct EstadoTelaAgenda {
    compromissos: Vec<Compromisso>,
    recursos: Vec<String>,
    base: Data,
    modo: ModoCalendario,
    dlg: Dlg,
    seq: u32,
    erro: Option<String>,
}

impl Default for EstadoTelaAgenda {
    fn default() -> Self {
        let hoje = Data::hoje(FUSO);
        let em = |h: u32, m: u32| {
            Instante::de_data_hora(hoje, Hora::de_hms(h, m, 0).unwrap_or_default(), FUSO)
        };
        Self {
            recursos: vec!["Sala 1".into(), "Sala 2".into(), "Técnico João".into()],
            compromissos: vec![
                Compromisso {
                    id: "seed-1".into(),
                    titulo: "Reunião de alinhamento".into(),
                    cliente: String::new(),
                    recurso: "Sala 1".into(),
                    inicio: em(9, 0),
                    fim: em(10, 0),
                    estado: Estado::Confirmado,
                },
                Compromisso {
                    id: "seed-2".into(),
                    titulo: "Visita — Supermercado Sul".into(),
                    cliente: "Supermercado Sul".into(),
                    recurso: "Técnico João".into(),
                    inicio: em(14, 30),
                    fim: em(16, 0),
                    estado: Estado::Agendado,
                },
            ],
            base: hoje,
            modo: ModoCalendario::Semana,
            dlg: Dlg::Fechado,
            seq: 0,
            erro: None,
        }
    }
}

impl EstadoTelaAgenda {
    /// `carregar` — hoje um no-op; vira `motor.consultar("agenda.proximos_compromissos.v1")`.
    pub fn carregar(
        &mut self,
        _motor: &cardeal_cliente::MotorLocal,
        _sessao: &cardeal_cliente::SessaoLocal,
    ) {
        // BACKEND: buscar compromissos do período visível.
    }

    fn em_conflito(&self, c: &Compromisso) -> bool {
        self.compromissos.iter().any(|o| {
            o.id != c.id
                && o.recurso == c.recurso
                && !c.recurso.trim().is_empty()
                && !matches!(o.estado, Estado::Cancelado | Estado::NaoCompareceu)
                && o.inicio.em_micros() < c.fim.em_micros()
                && o.fim.em_micros() > c.inicio.em_micros()
        })
    }
}

/// Desenha a tela inteira.
pub fn mostrar(
    ui: &mut egui::Ui,
    _motor: &cardeal_cliente::MotorLocal,
    _sessao: &cardeal_cliente::SessaoLocal,
    estado: &mut EstadoTelaAgenda,
) {
    LayoutTela::nova("Agenda").mostrar(
        ui,
        estado,
        |ui, estado| {
            if ui
                .add(Botao::primario("+ Novo compromisso").atalho("Ctrl+N"))
                .clicked()
            {
                estado.dlg = Dlg::Novo(form_em(estado, Data::hoje(FUSO), 9));
            }
        },
        |ui, estado| {
            barra_controles(ui, estado);
            ui.add_space(Espaco::E12);
            if let Some(e) = &estado.erro {
                ui.add(
                    Rotulo::interface(e.clone())
                        .quebravel()
                        .cor(ui.cores().negativo),
                );
                ui.add_space(Espaco::E8);
            }
            resumo_hoje(ui, estado);
            ui.add_space(Espaco::E12);

            // Monta os blocos (empresta de `compromissos`).
            let conflitos: Vec<bool> = estado
                .compromissos
                .iter()
                .map(|c| estado.em_conflito(c))
                .collect();
            let blocos: Vec<BlocoAgenda> = estado
                .compromissos
                .iter()
                .zip(&conflitos)
                .map(|(c, &cf)| BlocoAgenda {
                    id: &c.id,
                    titulo: &c.titulo,
                    subtitulo: if c.cliente.is_empty() {
                        None
                    } else {
                        Some(&c.cliente)
                    },
                    inicio: c.inicio,
                    fim: c.fim,
                    tag: c.tag(cf),
                    conflito: cf,
                })
                .collect();

            let acao = egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    AgendaCalendario::nova(estado.base, estado.modo, &blocos)
                        .horas(7, 21)
                        .mostrar(ui)
                })
                .inner;

            match acao {
                AcaoAgenda::Nenhuma => {}
                AcaoAgenda::Bloco(id) => estado.dlg = Dlg::Ver(id),
                AcaoAgenda::Vazio { data, hora } => {
                    let mut f = form_em(estado, data, hora.hora());
                    f.hora_ini = hora.formatar();
                    let fim =
                        Hora::de_hms((hora.hora() + 1).min(23), hora.minuto(), 0).unwrap_or(hora);
                    f.hora_fim = fim.formatar();
                    estado.dlg = Dlg::Novo(f);
                }
            }
        },
    );

    match &estado.dlg {
        Dlg::Fechado => {}
        Dlg::Novo(_) => dialogo_novo(ui.ctx(), estado),
        Dlg::Ver(_) => dialogo_ver(ui.ctx(), estado),
    }
}

fn form_em(estado: &EstadoTelaAgenda, data: Data, hora: u32) -> FormComp {
    let ini = Hora::de_hms(hora, 0, 0).unwrap_or_default();
    let fim = Hora::de_hms((hora + 1).min(23), 0, 0).unwrap_or_default();
    FormComp {
        id: None,
        recurso: (!estado.recursos.is_empty()).then_some(0),
        data: data.to_string(),
        hora_ini: ini.formatar(),
        hora_fim: fim.formatar(),
        ..FormComp::default()
    }
}

fn barra_controles(ui: &mut egui::Ui, estado: &mut EstadoTelaAgenda) {
    ui.horizontal(|ui| {
        for (m, r) in [
            (ModoCalendario::Dia, "Dia"),
            (ModoCalendario::Semana, "Semana"),
        ] {
            let b = if estado.modo == m {
                Botao::primario(r)
            } else {
                Botao::fantasma(r)
            };
            if ui.add(b).clicked() {
                estado.modo = m;
            }
        }
        ui.add_space(Espaco::E16);
        if ui.add(Botao::secundario("‹")).clicked() {
            estado.base = estado.base.mais_dias(passo(estado.modo, -1));
        }
        if ui.add(Botao::secundario("Hoje")).clicked() {
            estado.base = Data::hoje(FUSO);
        }
        if ui.add(Botao::secundario("›")).clicked() {
            estado.base = estado.base.mais_dias(passo(estado.modo, 1));
        }
        ui.add_space(Espaco::E12);
        ui.add(Rotulo::titulo_secao(titulo_periodo(estado)));
    });
}

const fn passo(modo: ModoCalendario, dir: i32) -> i32 {
    match modo {
        ModoCalendario::Dia => dir,
        ModoCalendario::Semana => dir * 7,
    }
}

fn titulo_periodo(estado: &EstadoTelaAgenda) -> String {
    match estado.modo {
        ModoCalendario::Dia => estado.base.formatar(),
        ModoCalendario::Semana => {
            let dow = estado.base.dia_da_semana() as i32;
            let seg = estado.base.mais_dias(-((dow + 6) % 7));
            let dom = seg.mais_dias(6);
            format!("{} – {}", seg.formatar_curta(), dom.formatar_curta())
        }
    }
}

fn resumo_hoje(ui: &mut egui::Ui, estado: &EstadoTelaAgenda) {
    let hoje = Data::hoje(FUSO);
    let agora = Instante::agora();
    let mut hoje_comp: Vec<&Compromisso> = estado
        .compromissos
        .iter()
        .filter(|c| c.inicio.data(FUSO) == hoje && !matches!(c.estado, Estado::Cancelado))
        .collect();
    hoje_comp.sort_by_key(|c| c.inicio.em_micros());
    let proximo = hoje_comp
        .iter()
        .find(|c| c.fim.em_micros() > agora.em_micros());

    let texto = match (hoje_comp.len(), proximo) {
        (0, _) => "Nada agendado para hoje.".to_owned(),
        (n, Some(p)) => format!(
            "{n} compromisso(s) hoje · próximo {} — {}",
            p.inicio.hora(FUSO).formatar(),
            p.titulo
        ),
        (n, None) => format!("{n} compromisso(s) hoje · todos já passaram"),
    };
    ui.add(Rotulo::interface(texto).cor(ui.cores().texto_medio));
}

fn dialogo_novo(ctx: &egui::Context, estado: &mut EstadoTelaAgenda) {
    let recursos = estado.recursos.clone();
    let fechar = Dialogo::nova(if matches!(&estado.dlg, Dlg::Novo(f) if f.id.is_some()) {
        "Editar compromisso"
    } else {
        "Novo compromisso"
    })
    .largura(560.0)
    .mostrar(
        ctx,
        estado,
        |ui, estado| {
            let Dlg::Novo(f) = &mut estado.dlg else {
                return;
            };
            ui.add(Campo::novo("Título", &mut f.titulo).marcador("ex.: Reunião, Visita técnica"));
            ui.add_space(Espaco::E12);
            ui.add(Campo::novo("Cliente (opcional)", &mut f.cliente));
            ui.add_space(Espaco::E12);
            SeletorOpcao::novo("Recurso", &mut f.recurso)
                .opcoes(recursos.iter().cloned().enumerate())
                .placeholder("Sala / técnico / equipamento")
                .mostrar(ui);
            ui.add_space(Espaco::E12);
            ui.add(Campo::novo("Data", &mut f.data).mascara(Mascara::Data));
            ui.add_space(Espaco::E12);
            ui.columns(2, |c| {
                c[0].add(Campo::novo("Início", &mut f.hora_ini).marcador("14:30"));
                c[1].add(Campo::novo("Fim", &mut f.hora_fim).marcador("15:30"));
            });
        },
        |ui, estado| {
            if ui.add(Botao::primario("Salvar")).clicked() {
                salvar(estado);
            }
            if ui.add(Botao::secundario("Cancelar")).clicked() {
                estado.dlg = Dlg::Fechado;
            }
        },
    );
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

fn salvar(estado: &mut EstadoTelaAgenda) {
    let Dlg::Novo(f) = &estado.dlg else { return };
    let f = f.clone();
    if f.titulo.trim().is_empty() {
        estado.erro = Some("Dê um título ao compromisso.".to_owned());
        return;
    }
    let (Ok(data), Some(hi), Some(hf)) = (
        f.data.parse::<Data>(),
        parse_hora(&f.hora_ini),
        parse_hora(&f.hora_fim),
    ) else {
        estado.erro = Some("Data (dd/mm/aaaa) ou horas (HH:MM) inválidas.".to_owned());
        return;
    };
    let recurso_txt = f
        .recurso
        .and_then(|i| estado.recursos.get(i).cloned())
        .unwrap_or_default();
    let inicio = Instante::de_data_hora(data, hi, FUSO);
    let fim = Instante::de_data_hora(data, hf, FUSO);
    if fim.em_micros() <= inicio.em_micros() {
        estado.erro = Some("O fim tem de ser depois do início.".to_owned());
        return;
    }

    // BACKEND: motor.executar("agenda.criar_compromisso.v1", ...) — verifica ConflitoDeAgenda.
    if let Some(id) = &f.id {
        if let Some(c) = estado.compromissos.iter_mut().find(|c| &c.id == id) {
            c.titulo = f.titulo.clone();
            c.cliente = f.cliente.clone();
            c.recurso = recurso_txt.clone();
            c.inicio = inicio;
            c.fim = fim;
        }
    } else {
        estado.seq += 1;
        estado.compromissos.push(Compromisso {
            id: format!("c{}", estado.seq),
            titulo: f.titulo.clone(),
            cliente: f.cliente.clone(),
            recurso: recurso_txt.clone(),
            inicio,
            fim,
            estado: Estado::Agendado,
        });
    }
    estado.erro = None;
    estado.dlg = Dlg::Fechado;
}

fn parse_hora(s: &str) -> Option<Hora> {
    let s = s.trim().replace('h', ":");
    let (h, m) = s.split_once(':').unwrap_or((s.as_str(), "0"));
    Hora::de_hms(h.trim().parse().ok()?, m.trim().parse().unwrap_or(0), 0).ok()
}

fn dialogo_ver(ctx: &egui::Context, estado: &mut EstadoTelaAgenda) {
    let Dlg::Ver(id) = &estado.dlg else { return };
    let Some(c) = estado.compromissos.iter().find(|c| &c.id == id).cloned() else {
        estado.dlg = Dlg::Fechado;
        return;
    };

    let fechar = Dialogo::nova(c.titulo.clone()).largura(520.0).mostrar(
        ctx,
        estado,
        |ui, _estado| {
            let dia = c.inicio.data(FUSO);
            ui.columns(2, |col| {
                kv(
                    &mut col[0],
                    "Quando",
                    &format!(
                        "{} · {}–{}",
                        dia.formatar_curta(),
                        c.inicio.hora(FUSO).formatar(),
                        c.fim.hora(FUSO).formatar()
                    ),
                );
                kv(&mut col[1], "Estado", c.estado.rotulo());
            });
            ui.columns(2, |col| {
                kv(&mut col[0], "Cliente", &c.cliente);
                kv(&mut col[1], "Recurso", &c.recurso);
            });
            ui.add_space(Espaco::E12);
            ui.separator();
            ui.add_space(Espaco::E12);
            acoes_estado(ui, _estado, &c);
        },
        |ui, estado| {
            if ui.add(Botao::secundario("Editar")).clicked() {
                let dia = c.inicio.data(FUSO);
                estado.dlg = Dlg::Novo(FormComp {
                    id: Some(c.id.clone()),
                    titulo: c.titulo.clone(),
                    cliente: c.cliente.clone(),
                    recurso: estado.recursos.iter().position(|r| *r == c.recurso),
                    data: dia.to_string(),
                    hora_ini: c.inicio.hora(FUSO).formatar(),
                    hora_fim: c.fim.hora(FUSO).formatar(),
                });
            }
            if ui.add(Botao::destrutivo("Excluir")).clicked() {
                estado.compromissos.retain(|x| x.id != c.id);
                estado.dlg = Dlg::Fechado;
            }
            if ui.add(Botao::fantasma("Fechar")).clicked() {
                estado.dlg = Dlg::Fechado;
            }
        },
    );
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

fn acoes_estado(ui: &mut egui::Ui, estado: &mut EstadoTelaAgenda, c: &Compromisso) {
    let mut mudar = |novo: Estado| {
        if let Some(x) = estado.compromissos.iter_mut().find(|x| x.id == c.id) {
            x.estado = novo;
        }
    };
    ui.horizontal(|ui| match c.estado {
        Estado::Agendado => {
            if ui.add(Botao::primario("Confirmar")).clicked() {
                mudar(Estado::Confirmado);
            }
            if ui.add(Botao::destrutivo("Cancelar")).clicked() {
                mudar(Estado::Cancelado);
            }
        }
        Estado::Confirmado => {
            if ui.add(Botao::primario("Iniciar")).clicked() {
                mudar(Estado::EmAndamento);
            }
            if ui.add(Botao::secundario("Não compareceu")).clicked() {
                mudar(Estado::NaoCompareceu);
            }
            if ui.add(Botao::destrutivo("Cancelar")).clicked() {
                mudar(Estado::Cancelado);
            }
        }
        Estado::EmAndamento => {
            if ui.add(Botao::primario("Concluir")).clicked() {
                mudar(Estado::Concluido);
            }
        }
        Estado::Concluido | Estado::Cancelado | Estado::NaoCompareceu => {
            ui.add(Rotulo::campo("Sem ações neste estado."));
        }
    });
}

fn kv(ui: &mut egui::Ui, chave: &str, valor: &str) {
    ui.add(Rotulo::campo(chave));
    ui.add(Rotulo::interface(if valor.trim().is_empty() {
        "—"
    } else {
        valor
    }));
    ui.add_space(Espaco::E8);
}
