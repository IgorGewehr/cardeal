//! A aba "Plano de contas": a árvore (até 5 níveis), contas novas, os modelos por segmento e as
//! categorias que lançam em cada conta de receita/despesa — o que faz a DRE separar aluguel de
//! energia de pró-labore.

use std::collections::HashSet;

use super::*;
use cardeal_ledger::ModeloDePlano;
use cardeal_ui::organisms::{ArvoreHierarquia, NoArvore};
use mod_financeiro::{
    AplicarModeloDePlano, CategoriasPorConta, CriarContaDoPlano, ModeloAplicado, NoPlano,
    PlanoDeContas, VincularCategoriaAConta, NIVEL_MAXIMO_DO_PLANO,
};

/// O estado da aba.
#[derive(Default)]
pub(super) struct EstadoPlano {
    pub(super) nos: Vec<NoPlano>,
    recolhidos: HashSet<Id>,
    selecionada: Option<Id>,
    /// Pares (conta, categoria).
    pub(super) vinculos: Vec<(Id, Id)>,
    nome_novo: String,
    agrupadora: bool,
    categoria_a_vincular: Option<Id>,
    modelo: Option<ModeloDePlano>,
}

fn natureza_rotulo(n: &str) -> &'static str {
    match n {
        "Ativo" => "Ativo",
        "Passivo" => "Passivo",
        "PatrimonioLiquido" => "Patrimônio líquido",
        "Receita" => "Receita",
        _ => "Despesa",
    }
}

impl EstadoPlano {
    pub(super) fn carregar(&mut self, motor: &Motor, sessao: &Sessao) {
        if let Ok(n) = motor.consultar(sessao, "financeiro.plano_de_contas.v1", &PlanoDeContas) {
            self.nos = n;
        }
        if let Ok(v) = motor.consultar(
            sessao,
            "financeiro.categorias_por_conta.v1",
            &CategoriasPorConta,
        ) {
            self.vinculos = v;
        }
        if self
            .selecionada
            .is_some_and(|s| !self.nos.iter().any(|n| n.id == s))
        {
            self.selecionada = None;
        }
    }

    fn selecionado(&self) -> Option<&NoPlano> {
        self.selecionada
            .and_then(|s| self.nos.iter().find(|n| n.id == s))
    }

    fn recebe_categoria(n: &NoPlano) -> bool {
        !n.sintetica && matches!(n.natureza.as_str(), "Receita" | "Despesa")
    }
}

pub(super) fn painel_plano(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) {
    modelos(ui, motor, sessao, estado);
    ui.add_space(Espaco::E16);
    ui.columns(2, |c| {
        arvore(&mut c[0], estado);
        detalhe(&mut c[1], motor, sessao, estado);
    });
}

fn modelos(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoTelaFinanceiro) {
    SecaoExpansivel::nova("Modelos de plano por segmento").mostrar(ui, |ui| {
        ui.add(Rotulo::apoio(
            "O plano padrão já vem pronto. Um modelo acrescenta as contas do seu tipo de negócio \
             (aluguel, energia, pró-labore, serviços de bancada…) sem mexer no que existe, e liga \
             às contas as categorias de mesmo nome. Pode aplicar mais de um.",
        ));
        ui.add_space(Espaco::E8);
        let itens: Vec<(ModeloDePlano, &str)> = ModeloDePlano::TODOS
            .iter()
            .map(|m| (*m, m.nome()))
            .collect();
        if let Some(m) = Abas::nova(&itens)
            .selecionada(estado.plano.modelo.unwrap_or(ModeloDePlano::Comercio))
            .id_salt("plano-modelo")
            .mostrar(ui)
        {
            estado.plano.modelo = Some(m);
        }
        let modelo = estado.plano.modelo.unwrap_or(ModeloDePlano::Comercio);
        ui.add_space(Espaco::E4);
        ui.add(Rotulo::apoio(modelo.descricao()));
        ui.add_space(Espaco::E8);
        if ui
            .add(Botao::secundario(format!(
                "Aplicar modelo: {}",
                modelo.nome()
            )))
            .clicked()
        {
            match motor.executar(
                sessao,
                "financeiro.aplicar_modelo_de_plano.v1",
                &AplicarModeloDePlano { modelo },
            ) {
                Ok(r) => {
                    let r: ModeloAplicado = r;
                    estado.plano.carregar(motor, sessao);
                    estado.carregar_categorias(motor, sessao);
                    let texto = if r.criadas.is_empty() {
                        "Nada a acrescentar: o plano já tem todas as contas deste modelo".to_owned()
                    } else {
                        format!(
                            "{} conta(s) acrescentada(s); {} categoria(s) ligada(s) a contas",
                            r.criadas.len(),
                            r.categorias_vinculadas
                        )
                    };
                    notificar(ui.ctx(), Notificacao::sucesso(texto));
                }
                Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
            }
        }
    });
}

fn arvore(ui: &mut egui::Ui, estado: &mut EstadoTelaFinanceiro) {
    let p = &mut estado.plano;
    let nos: Vec<NoArvore> = p
        .nos
        .iter()
        .map(|n| NoArvore {
            id: n.id,
            nivel: n.nivel,
            rotulo: format!("{} · {}", n.codigo, n.nome),
            apoio: None,
            tem_filhos: p.nos.iter().any(|f| f.pai == Some(n.id)),
        })
        .collect();
    if nos.is_empty() {
        EstadoVazio::novo(Icone::Dinheiro, "Plano de contas não carregado.").mostrar(ui);
        return;
    }
    let copia = p.nos.clone();
    let resposta = ArvoreHierarquia::nova(&nos, &mut p.recolhidos)
        .selecionado(p.selecionada)
        .mostrar(ui, |ui, no| {
            let Some(n) = copia.iter().find(|n| n.id == no.id) else {
                return;
            };
            if !n.ativa {
                ui.add(Etiqueta::neutra("inativa"));
            }
            ui.add(if n.sintetica {
                Etiqueta::neutra("agrupadora")
            } else {
                Etiqueta::info("recebe lançamento")
            });
        });
    if let Some(id) = resposta.clicado {
        p.selecionada = Some(id);
        p.categoria_a_vincular = None;
        p.nome_novo.clear();
    }
}

fn detalhe(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoTelaFinanceiro) {
    let Some(no) = estado.plano.selecionado().cloned() else {
        Painel::novo().plano().mostrar(ui, |ui| {
            ui.add(Rotulo::apoio(
                "Escolha uma conta na árvore para ver os detalhes, criar contas dentro dela ou \
                 ligar categorias.",
            ));
        });
        return;
    };
    Painel::novo().plano().mostrar(ui, |ui| {
        ui.add(Rotulo::sobrelinha(natureza_rotulo(&no.natureza)));
        ui.add(Rotulo::titulo_secao(format!("{} · {}", no.codigo, no.nome)));
        ui.add(Rotulo::apoio(format!(
            "Nível {} · {}{}",
            no.nivel,
            if no.sintetica {
                "agrupadora (recebe contas filhas)"
            } else {
                "recebe lançamento"
            },
            no.papel
                .as_deref()
                .map_or_else(String::new, |p| format!(" · papel {p}"))
        )));
        ui.add_space(Espaco::E12);
        if no.sintetica && no.nivel < NIVEL_MAXIMO_DO_PLANO {
            nova_conta(ui, motor, sessao, estado, &no);
        } else if no.sintetica {
            ui.add(Rotulo::apoio(format!(
                "O plano tem no máximo {NIVEL_MAXIMO_DO_PLANO} níveis: não dá para criar abaixo desta conta."
            )));
        }
        if EstadoPlano::recebe_categoria(&no) {
            categorias_da_conta(ui, motor, sessao, estado, &no);
        }
    });
}

fn nova_conta(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
    no: &NoPlano,
) {
    ui.add(Rotulo::titulo_secao(format!(
        "Nova conta dentro de {}",
        no.nome
    )));
    ui.add_space(Espaco::E8);
    ui.add(Campo::novo("Nome da conta", &mut estado.plano.nome_novo));
    ui.add_space(Espaco::E8);
    if let Some(a) = Abas::nova(&[(false, "Recebe lançamentos"), (true, "Agrupadora")])
        .selecionada(estado.plano.agrupadora)
        .id_salt("plano-tipo")
        .mostrar(ui)
    {
        estado.plano.agrupadora = a;
    }
    ui.add_space(Espaco::E8);
    if ui.add(Botao::primario("Criar conta")).clicked() {
        match motor.executar(
            sessao,
            "financeiro.criar_conta_do_plano.v1",
            &CriarContaDoPlano {
                pai: no.id,
                nome: estado.plano.nome_novo.clone(),
                agrupadora: estado.plano.agrupadora,
            },
        ) {
            Ok(c) => {
                let c: mod_financeiro::ContaDoPlanoCriada = c;
                estado.plano.nome_novo.clear();
                estado.plano.recolhidos.remove(&no.id);
                estado.plano.carregar(motor, sessao);
                estado.plano.selecionada = Some(c.conta);
                notificar(
                    ui.ctx(),
                    Notificacao::sucesso(format!("Conta {} criada", c.codigo)),
                );
            }
            Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
        }
    }
}

fn categorias_da_conta(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
    no: &NoPlano,
) {
    ui.add_space(Espaco::E12);
    ui.add(Rotulo::titulo_secao("Categorias que lançam nesta conta"));
    ui.add(Rotulo::apoio(
        "Um título lançado com a categoria vinculada vai para esta conta; sem vínculo, vai para a \
         conta genérica de despesas ou receitas.",
    ));
    ui.add_space(Espaco::E8);
    let aqui: Vec<Id> = estado
        .plano
        .vinculos
        .iter()
        .filter(|(c, _)| *c == no.id)
        .map(|(_, cat)| *cat)
        .collect();
    let mut desvincular: Option<Id> = None;
    for cat in &aqui {
        ui.horizontal(|ui| {
            ui.add(Rotulo::interface(estado.nome_categoria(Some(*cat))));
            if ui.add(Botao::fantasma("Desvincular").pequeno()).clicked() {
                desvincular = Some(*cat);
            }
        });
    }
    if aqui.is_empty() {
        ui.add(Rotulo::apoio("Nenhuma categoria vinculada."));
    }
    let especie_ok = |c: &CategoriaFinanceira| match c.especie {
        Some(EspecieTitulo::Pagar) => no.natureza == "Despesa",
        Some(EspecieTitulo::Receber) => no.natureza == "Receita",
        None => true,
    };
    let candidatas: Vec<(Id, String)> = estado
        .categorias
        .iter()
        .filter(|c| especie_ok(c) && !aqui.contains(&c.id))
        .map(|c| (c.id, c.nome.clone()))
        .collect();
    ui.add_space(Espaco::E8);
    if !candidatas.is_empty() {
        SeletorOpcao::novo("Vincular categoria", &mut estado.plano.categoria_a_vincular)
            .opcoes(candidatas)
            .placeholder("Escolha uma categoria")
            .mostrar(ui);
        ui.add_space(Espaco::E4);
        if ui
            .add_enabled(
                estado.plano.categoria_a_vincular.is_some(),
                Botao::secundario("Vincular"),
            )
            .clicked()
        {
            if let Some(cat) = estado.plano.categoria_a_vincular {
                vincular(ui.ctx(), motor, sessao, estado, cat, Some(no.id));
                estado.plano.categoria_a_vincular = None;
            }
        }
    }
    if let Some(cat) = desvincular {
        vincular(ui.ctx(), motor, sessao, estado, cat, None);
    }
}

fn vincular(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
    categoria: Id,
    conta: Option<Id>,
) {
    match motor.executar(
        sessao,
        "financeiro.vincular_categoria_a_conta.v1",
        &VincularCategoriaAConta { categoria, conta },
    ) {
        Ok(()) => {
            estado.plano.carregar(motor, sessao);
            notificar(
                ctx,
                Notificacao::sucesso(if conta.is_some() {
                    "Categoria vinculada à conta"
                } else {
                    "Vínculo desfeito"
                }),
            );
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}
