//! A aba **Contador** do Financeiro (o Portal do Contador, `mod_financeiro::contador`): os
//! apontamentos entre o contador e a empresa e o DE/PARA do plano de contas — o código de cada
//! conta no sistema do escritório, que a exportação dos lançamentos do pacote do mês usa.

use std::collections::HashMap;

use cardeal_cliente::{Motor, Sessao};
use cardeal_kernel::{Competencia, Data, Fuso, Id};
use cardeal_ui::atoms::{Botao, Etiqueta, Rotulo, Tom};
use cardeal_ui::molecules::Campo;
use cardeal_ui::organisms::{notificar, Notificacao, Painel};
use cardeal_ui::tokens::Espaco;
use eframe::egui;
use mod_financeiro::contador::{
    Apontamento, Apontamentos, DeParaDoContador, DefinirDePara, EstadoApontamento, LinhaDePara,
    RegistrarApontamento, ResolverApontamento, ResponderApontamento,
};

/// O estado da aba.
#[derive(Default)]
pub struct EstadoContador {
    apontamentos: Vec<Apontamento>,
    todos: bool,
    /// O apontamento novo.
    texto: String,
    /// A resposta digitada de cada apontamento.
    respostas: HashMap<Id, String>,
    de_para: Vec<LinhaDePara>,
    /// O código digitado de cada conta.
    codigos: HashMap<Id, String>,
    so_sem_codigo: bool,
    /// Quantos esperam a resposta da empresa (o aviso da Visão geral).
    aguardando: usize,
}

impl EstadoContador {
    /// Recarrega os apontamentos e o DE/PARA.
    pub fn carregar(&mut self, motor: &Motor, sessao: &Sessao) {
        self.apontamentos = motor
            .consultar(
                sessao,
                "financeiro.apontamentos.v1",
                &Apontamentos {
                    so_pendentes: !self.todos,
                },
            )
            .unwrap_or_default();
        self.de_para = motor
            .consultar(
                sessao,
                "financeiro.de_para_do_contador.v1",
                &DeParaDoContador,
            )
            .unwrap_or_default();
        self.codigos = self
            .de_para
            .iter()
            .map(|l| (l.conta, l.externo.clone().unwrap_or_default()))
            .collect();
        self.conhecer(motor, sessao);
    }

    /// Só a contagem do aviso da Visão geral.
    pub fn conhecer(&mut self, motor: &Motor, sessao: &Sessao) {
        self.aguardando = esperando_resposta(motor, sessao);
    }

    /// Para os testes: quantos esperam a empresa.
    #[cfg(test)]
    pub const fn aguardando(&self) -> usize {
        self.aguardando
    }
}

/// O aviso da Visão geral: o contador pediu e a empresa ainda não respondeu.
pub(super) fn aviso_na_visao(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut super::EstadoTelaFinanceiro,
) {
    let n = estado.contador.aguardando;
    if n == 0 {
        return;
    }
    let mut abrir = false;
    Painel::novo().realce(Tom::Atencao).mostrar(ui, |ui| {
        ui.horizontal(|ui| {
            ui.add(Rotulo::interface(format!(
                "O contador deixou {n} apontamento(s) esperando a resposta da empresa"
            )));
            ui.add_space(Espaco::E12);
            abrir = ui.add(Botao::secundario("Responder").pequeno()).clicked();
        });
    });
    ui.add_space(Espaco::E12);
    if abrir {
        estado.aba = super::Aba::Contador;
        estado.carregar(motor, sessao);
    }
}

/// Quantos apontamentos esperam a resposta da empresa (o aviso da Visão geral).
pub fn esperando_resposta(motor: &Motor, sessao: &Sessao) -> usize {
    if !sessao.concede("financeiro.apontamento.responder") {
        return 0;
    }
    motor
        .consultar(
            sessao,
            "financeiro.apontamentos.v1",
            &Apontamentos { so_pendentes: true },
        )
        .map(|v| {
            v.iter()
                .filter(|a| a.estado == EstadoApontamento::Aberto)
                .count()
        })
        .unwrap_or(0)
}

fn etiqueta(e: EstadoApontamento) -> Etiqueta {
    match e {
        EstadoApontamento::Aberto => Etiqueta::atencao("esperando a empresa"),
        EstadoApontamento::Respondido => Etiqueta::info("respondido"),
        EstadoApontamento::Resolvido => Etiqueta::positiva("resolvido"),
    }
}

/// O que a pessoa pediu num apontamento.
enum Pedido {
    Responder(Id, String),
    Resolver(Id, bool),
}

/// Desenha a aba.
pub fn painel(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, e: &mut EstadoContador) {
    let responde = sessao.concede("financeiro.apontamento.responder");
    let aponta = sessao.concede("financeiro.contador.apontar");
    ui.add(Rotulo::titulo_secao("Apontamentos do contador"));
    ui.add(
        Rotulo::apoio(
            "O contador pede o que falta (uma nota, a classificação de um pagamento); a empresa \
             responde aqui; o contador resolve quando conferiu.",
        )
        .quebravel(),
    );
    ui.add_space(Espaco::E8);
    if aponta {
        ui.horizontal(|ui| {
            ui.allocate_ui(egui::vec2(520.0, 40.0), |ui| {
                ui.add(
                    Campo::novo("Novo apontamento", &mut e.texto)
                        .marcador("ex.: falta a nota da compra de 12/10"),
                );
            });
            if ui.add(Botao::primario("Registrar").pequeno()).clicked() {
                let mes = Data::hoje(Fuso::BRASILIA).competencia();
                match motor.executar(
                    sessao,
                    "financeiro.registrar_apontamento.v1",
                    &RegistrarApontamento {
                        competencia: mes,
                        texto: e.texto.clone(),
                    },
                ) {
                    Ok(_) => {
                        e.texto.clear();
                        e.carregar(motor, sessao);
                    }
                    Err(erro) => notificar(ui.ctx(), Notificacao::erro(erro.mensagem)),
                }
            }
        });
        ui.add_space(Espaco::E8);
    }
    let rot = if e.todos {
        "Só os pendentes"
    } else {
        "Mostrar também os resolvidos"
    };
    if ui.add(Botao::fantasma(rot).pequeno()).clicked() {
        e.todos = !e.todos;
        e.carregar(motor, sessao);
    }
    if e.apontamentos.is_empty() {
        ui.add(Rotulo::apoio("Nenhum apontamento pendente."));
    }
    let mut pedido = None;
    for a in &e.apontamentos {
        let tom = match a.estado {
            EstadoApontamento::Aberto => Tom::Atencao,
            EstadoApontamento::Respondido => Tom::Info,
            EstadoApontamento::Resolvido => Tom::Neutro,
        };
        Painel::novo().realce(tom).compacto().mostrar(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(etiqueta(a.estado));
                ui.add(Rotulo::campo(format!(
                    "{} · {} · {}",
                    competencia_curta(a.competencia),
                    a.autor,
                    a.criado_em.formatar(Fuso::BRASILIA)
                )));
            });
            ui.add(Rotulo::interface(a.texto.clone()).quebravel());
            if let Some(r) = &a.resposta {
                ui.add(
                    Rotulo::apoio(format!(
                        "Resposta{}: {r}",
                        a.respondido_por
                            .as_deref()
                            .map(|n| format!(" de {n}"))
                            .unwrap_or_default()
                    ))
                    .quebravel(),
                );
            }
            if a.estado != EstadoApontamento::Resolvido && responde {
                let texto = e.respostas.entry(a.id).or_default();
                ui.horizontal(|ui| {
                    ui.allocate_ui(egui::vec2(460.0, 40.0), |ui| {
                        ui.add(Campo::novo("Resposta", texto));
                    });
                    if ui.add(Botao::secundario("Responder").pequeno()).clicked() {
                        pedido = Some(Pedido::Responder(a.id, texto.clone()));
                    }
                });
            }
            if aponta {
                let (rot, resolver) = if a.estado == EstadoApontamento::Resolvido {
                    ("Reabrir", false)
                } else {
                    ("Resolver", true)
                };
                if ui.add(Botao::fantasma(rot).pequeno()).clicked() {
                    pedido = Some(Pedido::Resolver(a.id, resolver));
                }
            }
        });
        ui.add_space(Espaco::E4);
    }
    if let Some(p) = pedido {
        let r = match p {
            Pedido::Responder(id, resposta) => motor.executar(
                sessao,
                "financeiro.responder_apontamento.v1",
                &ResponderApontamento {
                    apontamento: id,
                    resposta,
                },
            ),
            Pedido::Resolver(id, resolvido) => motor.executar(
                sessao,
                "financeiro.resolver_apontamento.v1",
                &ResolverApontamento {
                    apontamento: id,
                    resolvido,
                },
            ),
        };
        match r {
            Ok(()) => e.carregar(motor, sessao),
            Err(erro) => notificar(ui.ctx(), Notificacao::erro(erro.mensagem)),
        }
    }

    ui.add_space(Espaco::E16);
    secao_de_para(ui, motor, sessao, e);
}

fn competencia_curta(c: Competencia) -> String {
    format!("{:02}/{}", c.mes(), c.ano())
}

fn secao_de_para(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, e: &mut EstadoContador) {
    let edita = sessao.concede("financeiro.contador.de_para");
    let preenchidas = e.de_para.iter().filter(|l| l.externo.is_some()).count();
    ui.horizontal(|ui| {
        ui.add(Rotulo::titulo_secao("DE/PARA do plano de contas"));
        ui.add(Etiqueta::neutra(format!(
            "{preenchidas} de {} contas",
            e.de_para.len()
        )));
    });
    ui.add(
        Rotulo::apoio(
            "O código de cada conta no sistema do contador. O pacote do mês leva os lançamentos \
             num CSV com estes códigos (a conta sem código sai com o do Cardeal).",
        )
        .quebravel(),
    );
    ui.add_space(Espaco::E8);
    let rot = if e.so_sem_codigo {
        "Mostrar todas"
    } else {
        "Só as sem código"
    };
    if ui.add(Botao::fantasma(rot).pequeno()).clicked() {
        e.so_sem_codigo = !e.so_sem_codigo;
    }
    for l in &e.de_para {
        if e.so_sem_codigo && l.externo.is_some() {
            continue;
        }
        ui.horizontal(|ui| {
            ui.allocate_ui(egui::vec2(110.0, 36.0), |ui| {
                ui.add(Rotulo::codigo(l.codigo.clone()));
            });
            ui.allocate_ui(egui::vec2(320.0, 36.0), |ui| {
                ui.add(Rotulo::interface(l.nome.clone()));
            });
            let cod = e.codigos.entry(l.conta).or_default();
            ui.allocate_ui(egui::vec2(160.0, 36.0), |ui| {
                ui.add(
                    Campo::novo("", cod)
                        .somente_leitura(!edita)
                        .marcador("código"),
                );
            });
        });
    }
    if edita {
        ui.add_space(Espaco::E8);
        let mudadas: Vec<(Id, Option<String>)> = e
            .de_para
            .iter()
            .filter_map(|l| {
                let novo = e.codigos.get(&l.conta).map(|s| s.trim().to_owned())?;
                let antes = l.externo.clone().unwrap_or_default();
                (novo != antes).then(|| (l.conta, (!novo.is_empty()).then_some(novo)))
            })
            .collect();
        if ui
            .add(
                Botao::primario(format!("Salvar o DE/PARA ({} alterações)", mudadas.len()))
                    .habilitado(!mudadas.is_empty()),
            )
            .clicked()
        {
            match motor.executar(
                sessao,
                "financeiro.definir_de_para.v1",
                &DefinirDePara { codigos: mudadas },
            ) {
                Ok(n) => {
                    notificar(
                        ui.ctx(),
                        Notificacao::sucesso(format!("{n} conta(s) gravada(s)")),
                    );
                    e.carregar(motor, sessao);
                }
                Err(erro) => notificar(ui.ctx(), Notificacao::erro(erro.mensagem)),
            }
        }
    }
}
