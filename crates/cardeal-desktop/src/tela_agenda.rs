//! Tela de Agenda — calendário de compromissos (dia / semana / mês). `docs/modulos/agenda.md`.
//!
//! Ligada ao `mod-agenda` real: compromissos e recursos vêm do backend, o ciclo de vida usa
//! os comandos da FSM. Cliente é FK real para `clientes_pessoa`. `agenda` não lança dinheiro
//! (§7) — a integração com financeiro é pela correlação `origem_modulo`/`origem_id`, que
//! `os`/`hotelaria` preenchem quando chamam `CriarCompromisso`.

use std::collections::HashMap;

use cardeal_cliente::{MotorLocal, SessaoLocal};
use cardeal_kernel::{Data, Fuso, Hora, Id, Instante};
use cardeal_ui::atoms::{Botao, Rotulo};
use cardeal_ui::molecules::{Campo, Mascara, SeletorOpcao};
use cardeal_ui::organisms::{
    notificar, AcaoAgenda, AgendaCalendario, AgendaMes, BlocoAgenda, Dialogo, LayoutTela,
    ModoCalendario, Notificacao, TagAgenda,
};
use cardeal_ui::tokens::{Espaco, Raio, TemaUi};
use eframe::egui;
use mod_agenda::{
    CancelarCompromisso, Compromisso, CompromissosNoPeriodo, ConcluirCompromisso,
    ConfirmarCompromisso, CriarCompromisso, CriarRecurso, EstadoCompromisso, IniciarCompromisso,
    Recurso, Recursos, TipoCompromisso, TipoRecurso,
};
use mod_clientes::{ItemPessoa, Papel, PessoasPorPapel};

const FUSO: Fuso = Fuso::BRASILIA;

const fn rotulo_estado(e: EstadoCompromisso) -> &'static str {
    match e {
        EstadoCompromisso::Agendado => "Agendado",
        EstadoCompromisso::Confirmado => "Confirmado",
        EstadoCompromisso::EmAndamento => "Em andamento",
        EstadoCompromisso::Concluido => "Concluído",
        EstadoCompromisso::Cancelado => "Cancelado",
        EstadoCompromisso::NaoCompareceu => "Não compareceu",
    }
}

fn tag_de(c: &Compromisso, conflito: bool) -> TagAgenda {
    if conflito {
        return TagAgenda::EmAndamento;
    }
    match c.estado {
        EstadoCompromisso::Cancelado | EstadoCompromisso::NaoCompareceu => TagAgenda::Inativo,
        EstadoCompromisso::EmAndamento => TagAgenda::EmAndamento,
        EstadoCompromisso::Confirmado | EstadoCompromisso::Concluido => TagAgenda::Confirmado,
        EstadoCompromisso::Agendado if c.cliente.is_some() => TagAgenda::Cliente,
        EstadoCompromisso::Agendado => TagAgenda::Interno,
    }
}

#[derive(Default)]
enum Dlg {
    #[default]
    Fechado,
    Novo(FormComp),
    Ver(Id),
    NovoRecurso {
        nome: String,
        tipo: usize,
    },
}

#[derive(Default, Clone)]
struct FormComp {
    cliente: Option<Id>,
    recurso: Option<Id>,
    titulo: String,
    data: String,
    hora_ini: String,
    hora_fim: String,
}

/// Estado local da tela.
#[derive(Default)]
pub struct EstadoTelaAgenda {
    compromissos: Vec<Compromisso>,
    ids: Vec<String>,
    recursos: Vec<Recurso>,
    clientes: Vec<ItemPessoa>,
    nomes: HashMap<Id, String>,
    base: Option<Data>,
    modo: Modo,
    filtro_recurso: Option<Id>,
    dlg: Dlg,
    erro: Option<String>,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum Modo {
    Dia,
    #[default]
    Semana,
    Mes,
}

impl Modo {
    const fn organism(self) -> ModoCalendario {
        match self {
            Self::Dia => ModoCalendario::Dia,
            Self::Semana => ModoCalendario::Semana,
            Self::Mes => ModoCalendario::Mes,
        }
    }
}

impl EstadoTelaAgenda {
    /// Carrega compromissos do período visível, recursos e clientes.
    pub fn carregar(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        let base = self.base.get_or_insert_with(|| Data::hoje(FUSO));
        let (inicio, fim) = periodo(*base, self.modo);

        match motor.consultar(
            sessao,
            "agenda.compromissos_no_periodo.v1",
            &CompromissosNoPeriodo {
                inicio,
                fim,
                recurso: self.filtro_recurso,
            },
        ) {
            Ok(c) => {
                self.ids = c.iter().map(|x| x.id.to_string()).collect();
                self.compromissos = c;
                self.erro = None;
            }
            Err(e) => self.erro = Some(e.mensagem),
        }
        if let Ok(r) = motor.consultar(sessao, "agenda.recursos.v1", &Recursos) {
            self.recursos = r;
        }
        if let Ok(cli) = motor.consultar(
            sessao,
            "clientes.pessoas_por_papel.v1",
            &PessoasPorPapel {
                papel: Papel::Cliente,
                busca: None,
            },
        ) {
            self.nomes = cli.iter().map(|p| (p.pessoa, p.nome.clone())).collect();
            self.clientes = cli;
        }
    }

    fn nome_cliente(&self, id: Option<Id>) -> String {
        id.and_then(|i| self.nomes.get(&i).cloned())
            .unwrap_or_else(|| "—".to_owned())
    }

    fn nome_recurso(&self, comp: &Compromisso) -> String {
        // Sem consulta de recursos-do-compromisso ainda; mostra o vínculo por origem.
        comp.origem_modulo.clone().unwrap_or_default()
    }
}

fn periodo(base: Data, modo: Modo) -> (Instante, Instante) {
    let meia_noite =
        |d: Data| Instante::de_data_hora(d, Hora::de_hms(0, 0, 0).unwrap_or_default(), FUSO);
    match modo {
        Modo::Dia => (meia_noite(base), meia_noite(base.mais_dias(1))),
        Modo::Semana => {
            let seg = base.mais_dias(-((base.dia_da_semana() as i32 + 6) % 7));
            (meia_noite(seg), meia_noite(seg.mais_dias(7)))
        }
        Modo::Mes => {
            let p = base.inicio_do_mes();
            let ini = p.mais_dias(-((p.dia_da_semana() as i32 + 6) % 7));
            (meia_noite(ini), meia_noite(ini.mais_dias(42)))
        }
    }
}

/// Desenha a tela inteira.
pub fn mostrar(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
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
            barra_controles(ui, motor, sessao, estado);
            ui.add_space(Espaco::E12);
            if let Some(e) = &estado.erro {
                ui.add(
                    Rotulo::interface(e.clone())
                        .quebravel()
                        .cor(ui.cores().negativo),
                );
                ui.add_space(Espaco::E8);
            }

            let base = estado.base.unwrap_or_else(|| Data::hoje(FUSO));
            let subtitulos: Vec<String> = estado
                .compromissos
                .iter()
                .map(|c| estado.nome_cliente(c.cliente))
                .collect();
            let blocos: Vec<BlocoAgenda> = estado
                .compromissos
                .iter()
                .enumerate()
                .zip(&subtitulos)
                .map(|((i, c), sub)| BlocoAgenda {
                    id: &estado.ids[i],
                    titulo: &c.titulo,
                    subtitulo: c.cliente.map(|_| sub.as_str()),
                    inicio: c.inicio,
                    fim: c.fim,
                    tag: tag_de(c, false),
                    conflito: false,
                })
                .collect();

            let acao = match estado.modo {
                Modo::Mes => AgendaMes::nova(base, &blocos).mostrar(ui),
                m => AgendaCalendario::nova(base, m.organism(), &blocos)
                    .horas(7, 21)
                    .mostrar(ui),
            };

            match acao {
                AcaoAgenda::Nenhuma => {}
                AcaoAgenda::Bloco(id) => {
                    if let Ok(cid) = id.parse::<Id>() {
                        estado.dlg = Dlg::Ver(cid);
                    }
                }
                AcaoAgenda::AbrirDia(d) => {
                    estado.base = Some(d);
                    estado.modo = Modo::Dia;
                    estado.carregar(motor, sessao);
                }
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
        Dlg::Novo(_) => dialogo_novo(ui.ctx(), motor, sessao, estado),
        Dlg::Ver(_) => dialogo_ver(ui.ctx(), motor, sessao, estado),
        Dlg::NovoRecurso { .. } => dialogo_recurso(ui.ctx(), motor, sessao, estado),
    }
}

fn form_em(estado: &EstadoTelaAgenda, data: Data, hora: u32) -> FormComp {
    let ini = Hora::de_hms(hora, 0, 0).unwrap_or_default();
    let fim = Hora::de_hms((hora + 1).min(23), 0, 0).unwrap_or_default();
    FormComp {
        recurso: estado.recursos.first().map(|r| r.id),
        data: data.to_string(),
        hora_ini: ini.formatar(),
        hora_fim: fim.formatar(),
        ..FormComp::default()
    }
}

fn barra_controles(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaAgenda,
) {
    let base = estado.base.unwrap_or_else(|| Data::hoje(FUSO));
    let cores = ui.cores();
    ui.horizontal(|ui| {
        // Controle segmentado Dia / Semana / Mês.
        egui::Frame::none()
            .fill(cores.superficie_2)
            .rounding(Raio::ITEM)
            .inner_margin(3.0)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (m, r) in [
                    (Modo::Dia, "Dia"),
                    (Modo::Semana, "Semana"),
                    (Modo::Mes, "Mês"),
                ] {
                    let b = if estado.modo == m {
                        Botao::primario(r).pequeno()
                    } else {
                        Botao::fantasma(r).pequeno()
                    };
                    if ui.add(b).clicked() {
                        estado.modo = m;
                        estado.carregar(motor, sessao);
                    }
                }
            });

        ui.add_space(Espaco::E8);
        if ui.add(Botao::fantasma("\u{2039}").pequeno()).clicked() {
            estado.base = Some(recuar(base, estado.modo, -1));
            estado.carregar(motor, sessao);
        }
        if ui.add(Botao::secundario("Hoje").pequeno()).clicked() {
            estado.base = Some(Data::hoje(FUSO));
            estado.carregar(motor, sessao);
        }
        if ui.add(Botao::fantasma("\u{203A}").pequeno()).clicked() {
            estado.base = Some(recuar(base, estado.modo, 1));
            estado.carregar(motor, sessao);
        }
        ui.add_space(Espaco::E12);
        ui.add(Rotulo::titulo_secao(titulo_periodo(base, estado.modo)));

        // Direita, na mesma linha: filtro de recurso + resumo do dia.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let atual = estado
                .filtro_recurso
                .and_then(|id| estado.recursos.iter().find(|r| r.id == id))
                .map_or_else(|| "Todos os recursos".to_owned(), |r| r.nome.clone());
            let mut novo = estado.filtro_recurso;
            egui::ComboBox::from_id_salt("agenda-filtro-recurso")
                .selected_text(atual)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut novo, None, "Todos os recursos");
                    for r in &estado.recursos {
                        ui.selectable_value(&mut novo, Some(r.id), r.nome.clone());
                    }
                });
            if novo != estado.filtro_recurso {
                estado.filtro_recurso = novo;
                estado.carregar(motor, sessao);
            }

            ui.add_space(Espaco::E16);
            let (texto, ponto) = resumo_dia_texto(estado, &cores);
            ui.add(Rotulo::interface(texto).cor(cores.texto_medio));
            let (rct, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
            ui.painter().circle_filled(rct.center(), 4.0, ponto);
        });
    });
}

fn recuar(base: Data, modo: Modo, dir: i32) -> Data {
    match modo {
        Modo::Dia => base.mais_dias(dir),
        Modo::Semana => base.mais_dias(dir * 7),
        Modo::Mes => base.mais_meses(dir),
    }
}

fn titulo_periodo(base: Data, modo: Modo) -> String {
    match modo {
        Modo::Dia => base.formatar(),
        Modo::Mes => base.competencia().formatar_extenso(),
        Modo::Semana => {
            let seg = base.mais_dias(-((base.dia_da_semana() as i32 + 6) % 7));
            format!(
                "{} – {}",
                seg.formatar_curta(),
                seg.mais_dias(6).formatar_curta()
            )
        }
    }
}

/// O texto curto de resumo do dia + a cor do ponto (rubro quando há compromisso em aberto
/// ainda hoje). Fica na barra de controles, não numa faixa separada.
fn resumo_dia_texto(
    estado: &EstadoTelaAgenda,
    cores: &cardeal_ui::tokens::Cores,
) -> (String, egui::Color32) {
    let hoje = Data::hoje(FUSO);
    let agora = Instante::agora();
    let mut hoje_c: Vec<&Compromisso> = estado
        .compromissos
        .iter()
        .filter(|c| {
            c.inicio.data(FUSO) == hoje && !matches!(c.estado, EstadoCompromisso::Cancelado)
        })
        .collect();
    hoje_c.sort_by_key(|c| c.inicio.em_micros());
    let prox = hoje_c
        .iter()
        .find(|c| c.fim.em_micros() > agora.em_micros());

    match (hoje_c.len(), prox) {
        (0, _) => ("Nada hoje".to_owned(), cores.texto_fraco),
        (n, Some(p)) => {
            let t: String = p.titulo.chars().take(22).collect();
            (
                format!("Hoje {n} · {} {}", p.inicio.hora(FUSO).formatar(), t),
                cores.rubro,
            )
        }
        (n, None) => (format!("Hoje {n} · encerrados"), cores.texto_fraco),
    }
}

fn dialogo_novo(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaAgenda,
) {
    let ops_cli: Vec<(Id, String)> = estado
        .clientes
        .iter()
        .map(|c| (c.pessoa, c.nome.clone()))
        .collect();
    let ops_rec: Vec<(Id, String)> = estado
        .recursos
        .iter()
        .map(|r| (r.id, r.nome.clone()))
        .collect();
    let sem_recurso = estado.recursos.is_empty();

    let fechar = Dialogo::nova("Novo compromisso").largura(560.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            let Dlg::Novo(f) = &mut estado.dlg else {
                return;
            };
            ui.add(Campo::novo("Título", &mut f.titulo).marcador("ex.: Reunião, Visita técnica"));
            ui.add_space(Espaco::E12);
            SeletorOpcao::novo("Cliente (opcional)", &mut f.cliente)
                .opcoes(ops_cli.clone())
                .placeholder("Nenhum / interno")
                .mostrar(ui);
            ui.add_space(Espaco::E12);
            if sem_recurso {
                ui.add(Rotulo::campo("Nenhum recurso cadastrado."));
            } else {
                SeletorOpcao::novo("Recurso (opcional)", &mut f.recurso)
                    .opcoes(ops_rec.clone())
                    .placeholder("Sala / técnico / equipamento")
                    .mostrar(ui);
            }
            ui.add_space(Espaco::E12);
            ui.add(Campo::novo("Data", &mut f.data).mascara(Mascara::Data));
            ui.add_space(Espaco::E12);
            ui.columns(2, |c| {
                c[0].add(Campo::novo("Início", &mut f.hora_ini).marcador("14:30"));
                c[1].add(Campo::novo("Fim", &mut f.hora_fim).marcador("15:30"));
            });
        },
        |ui, estado| {
            if ui.add(Botao::secundario("Novo recurso")).clicked() {
                estado.dlg = Dlg::NovoRecurso {
                    nome: String::new(),
                    tipo: 0,
                };
                return;
            }
            if ui.add(Botao::primario("Agendar")).clicked() {
                agendar(ui.ctx(), motor, sessao, estado);
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

fn agendar(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaAgenda,
) {
    let Dlg::Novo(f) = &estado.dlg else { return };
    let f = f.clone();
    if f.titulo.trim().is_empty() {
        notificar(ctx, Notificacao::aviso("Dê um título ao compromisso."));
        return;
    }
    let (Ok(data), Some(hi), Some(hf)) = (
        f.data.parse::<Data>(),
        parse_hora(&f.hora_ini),
        parse_hora(&f.hora_fim),
    ) else {
        notificar(
            ctx,
            Notificacao::aviso("Data (dd/mm/aaaa) ou horas (HH:MM) inválidas."),
        );
        return;
    };
    let inicio = Instante::de_data_hora(data, hi, FUSO);
    let fim = Instante::de_data_hora(data, hf, FUSO);
    if fim.em_micros() <= inicio.em_micros() {
        notificar(
            ctx,
            Notificacao::aviso("O fim tem de ser depois do início."),
        );
        return;
    }

    let cmd = CriarCompromisso {
        titulo: f.titulo.clone(),
        tipo: if f.cliente.is_some() {
            TipoCompromisso::Cliente
        } else {
            TipoCompromisso::Interno
        },
        cliente: f.cliente,
        inicio,
        fim,
        recursos: f.recurso.into_iter().collect(),
        origem_modulo: None,
        origem_id: None,
        // O usuário escolheu o horário deliberadamente na grade — confirma fora do padrão.
        fora_do_horario_padrao: true,
    };
    match motor.executar(sessao, "agenda.criar_compromisso.v1", &cmd) {
        Ok(_) => {
            estado.erro = None;
            estado.dlg = Dlg::Fechado;
            estado.carregar(motor, sessao);
            notificar(ctx, Notificacao::sucesso("Compromisso criado"));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

fn dialogo_recurso(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaAgenda,
) {
    const TIPOS: [(TipoRecurso, &str); 4] = [
        (TipoRecurso::Sala, "Sala"),
        (TipoRecurso::Tecnico, "Técnico"),
        (TipoRecurso::Equipamento, "Equipamento"),
        (TipoRecurso::Pessoa, "Pessoa"),
    ];
    let fechar = Dialogo::nova("Novo recurso").largura(440.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            let Dlg::NovoRecurso { nome, tipo } = &mut estado.dlg else {
                return;
            };
            ui.add(Campo::novo("Nome", nome).marcador("Sala 1, Carlos, Furadeira"));
            ui.add_space(Espaco::E12);
            ui.horizontal_wrapped(|ui| {
                for (i, (_, r)) in TIPOS.iter().enumerate() {
                    let b = if *tipo == i {
                        Botao::primario(*r)
                    } else {
                        Botao::fantasma(*r)
                    };
                    if ui.add(b).clicked() {
                        *tipo = i;
                    }
                }
            });
        },
        |ui, estado| {
            if ui.add(Botao::primario("Criar")).clicked() {
                if let Dlg::NovoRecurso { nome, tipo } = &estado.dlg {
                    let (nome, tipo) = (nome.clone(), TIPOS[*tipo].0);
                    match motor.executar(
                        sessao,
                        "agenda.criar_recurso.v1",
                        &CriarRecurso {
                            nome,
                            tipo,
                            capacidade: None,
                        },
                    ) {
                        Ok(_) => {
                            estado.carregar(motor, sessao);
                            estado.dlg = Dlg::Novo(form_em(estado, Data::hoje(FUSO), 9));
                            notificar(ui.ctx(), Notificacao::sucesso("Recurso criado"));
                        }
                        Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
                    }
                }
            }
            if ui.add(Botao::secundario("Voltar")).clicked() {
                estado.dlg = Dlg::Novo(form_em(estado, Data::hoje(FUSO), 9));
            }
        },
    );
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

fn parse_hora(s: &str) -> Option<Hora> {
    let s = s.trim().replace('h', ":");
    let (h, m) = s.split_once(':').unwrap_or((s.as_str(), "0"));
    Hora::de_hms(h.trim().parse().ok()?, m.trim().parse().unwrap_or(0), 0).ok()
}

fn dialogo_ver(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaAgenda,
) {
    let Dlg::Ver(id) = estado.dlg else { return };
    let Some(c) = estado.compromissos.iter().find(|c| c.id == id).cloned() else {
        estado.dlg = Dlg::Fechado;
        return;
    };
    let cliente = estado.nome_cliente(c.cliente);
    let origem = estado.nome_recurso(&c);

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
                kv(&mut col[1], "Estado", rotulo_estado(c.estado));
            });
            ui.columns(2, |col| {
                kv(&mut col[0], "Cliente", &cliente);
                kv(
                    &mut col[1],
                    "Origem",
                    if origem.is_empty() { "avulso" } else { &origem },
                );
            });
            ui.add_space(Espaco::E12);
            ui.separator();
            ui.add_space(Espaco::E12);
            acoes_estado(ui, motor, sessao, _estado, &c);
        },
        |ui, estado| {
            if ui.add(Botao::secundario("Fechar")).clicked() {
                estado.dlg = Dlg::Fechado;
            }
        },
    );
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

fn acoes_estado(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaAgenda,
    c: &Compromisso,
) {
    let id = c.id;
    ui.horizontal(|ui| match c.estado {
        EstadoCompromisso::Agendado => {
            if ui.add(Botao::primario("Confirmar")).clicked() {
                aplicar(
                    ui.ctx(),
                    motor,
                    sessao,
                    estado,
                    "agenda.confirmar_compromisso.v1",
                    &ConfirmarCompromisso { compromisso: id },
                    "Compromisso confirmado",
                );
            }
            if ui.add(Botao::destrutivo("Cancelar")).clicked() {
                aplicar(
                    ui.ctx(),
                    motor,
                    sessao,
                    estado,
                    "agenda.cancelar_compromisso.v1",
                    &CancelarCompromisso { compromisso: id },
                    "Compromisso cancelado",
                );
            }
        }
        EstadoCompromisso::Confirmado => {
            if ui.add(Botao::primario("Iniciar")).clicked() {
                aplicar(
                    ui.ctx(),
                    motor,
                    sessao,
                    estado,
                    "agenda.iniciar_compromisso.v1",
                    &IniciarCompromisso { compromisso: id },
                    "Compromisso iniciado",
                );
            }
            if ui.add(Botao::destrutivo("Cancelar")).clicked() {
                aplicar(
                    ui.ctx(),
                    motor,
                    sessao,
                    estado,
                    "agenda.cancelar_compromisso.v1",
                    &CancelarCompromisso { compromisso: id },
                    "Compromisso cancelado",
                );
            }
        }
        EstadoCompromisso::EmAndamento => {
            if ui.add(Botao::primario("Concluir")).clicked() {
                aplicar(
                    ui.ctx(),
                    motor,
                    sessao,
                    estado,
                    "agenda.concluir_compromisso.v1",
                    &ConcluirCompromisso { compromisso: id },
                    "Compromisso concluído",
                );
            }
        }
        EstadoCompromisso::Concluido
        | EstadoCompromisso::Cancelado
        | EstadoCompromisso::NaoCompareceu => {
            ui.add(Rotulo::campo("Sem ações neste estado."));
        }
    });
}

fn aplicar<C: cardeal_modkit::Comando + serde::Serialize>(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaAgenda,
    nome: &str,
    comando: &C,
    sucesso: &str,
) where
    C::Saida: serde::de::DeserializeOwned,
{
    match motor.executar(sessao, nome, comando) {
        Ok(_) => {
            estado.dlg = Dlg::Fechado;
            estado.carregar(motor, sessao);
            notificar(ctx, Notificacao::sucesso(sucesso.to_owned()));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
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
