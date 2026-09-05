//! A tela de Ordens de Serviço — lista + detalhe com a ação certa para o estado atual.
//! `docs/modulos/os.md` §10.
//!
//! **Escopo desta versão**: o orçamento só aceita itens de mão de obra pela tela; peça vinda
//! do estoque (`AplicarPeca`) e busca de cliente existente ficam para quando a tela tiver um
//! buscador de produto/pessoa. Gaps conhecidos, não esquecidos.

use cardeal_cliente::{MotorLocal, SessaoLocal};
use cardeal_kernel::{Dinheiro, Id};
use cardeal_ui::atoms::{Botao, Rotulo, ValorDinheiro};
use cardeal_ui::molecules::{Campo, EstadoVazio};
use cardeal_ui::organisms::{ColunaGrade, Grade, LayoutTela};
use cardeal_ui::tokens::{Espaco, TemaUi};
use cardeal_modkit::Icone;
use eframe::egui;
use mod_clientes::{CriarPessoa, Papel as PapelCliente, PessoaCadastrada, TipoDocumento, TipoPessoa};
use mod_os::{
    AbrirOrdemServico, AprovarOrcamentoOs, BuscarDetalheOrdem, ConcluirExecucao, DetalheOrdem,
    EnviarParaAprovacao, EstadoOs, FaturarOrdemServico, IniciarExecucao, ItemOrcamentoNovo,
    MontarOrcamentoOs, OrdemServico, OrdemServicoAberta, OrdensEmAberto, RegistrarLaudo,
    ReprovarOrcamentoOs,
};

/// O estado local da tela — sobrevive entre quadros, não entre reinícios do app.
#[derive(Default)]
pub struct EstadoTelaOs {
    ordens: Vec<OrdemServico>,
    selecionada: Option<Id>,
    detalhe: Option<DetalheOrdem>,
    erro: Option<String>,

    novo_cliente_nome: String,
    novo_cliente_cpf: String,
    novo_equipamento: String,

    laudo_problema: String,
    laudo_diagnostico: String,

    mao_de_obra_descricao: String,
    mao_de_obra_valor: String,

    aprovador: String,
}

impl EstadoTelaOs {
    /// Recarrega a lista de ordens em aberto.
    pub fn carregar(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        match motor.consultar(sessao, "os.ordens_em_aberto.v1", &OrdensEmAberto) {
            Ok(ordens) => {
                self.ordens = ordens;
                self.erro = None;
            }
            Err(e) => self.erro = Some(e.mensagem),
        }
    }

    fn selecionar(&mut self, motor: &MotorLocal, sessao: &SessaoLocal, id: Id) {
        self.selecionada = Some(id);
        match motor.consultar(
            sessao,
            "os.buscar_detalhe_ordem.v1",
            &BuscarDetalheOrdem { ordem_servico: id },
        ) {
            Ok(detalhe) => self.detalhe = detalhe,
            Err(e) => self.erro = Some(e.mensagem),
        }
    }
}

/// Desenha a tela inteira (lista + detalhe responsivos).
pub fn mostrar(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
) {
    LayoutTela::nova("Ordens de Serviço").mostrar_com_detalhe(
        ui,
        estado,
        |ui, estado| {
            if ui.add(Botao::secundario("Recarregar").atalho("F5")).clicked() {
                estado.carregar(motor, sessao);
            }
        },
        |ui, estado| {
            if let Some(erro) = &estado.erro {
                ui.add(
                    Rotulo::interface(erro.clone())
                        .quebravel()
                        .cor(ui.cores().negativo),
                );
                ui.add_space(Espaco::E12);
            }
            mostrar_nova_os(ui, motor, sessao, estado);
            ui.add_space(Espaco::E24);
            mostrar_lista(ui, motor, sessao, estado);
        },
        |ui, estado| mostrar_detalhe(ui, motor, sessao, estado),
    );
}

fn mostrar_nova_os(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
) {
    ui.add(Rotulo::titulo_secao("Nova OS"));
    ui.add_space(Espaco::E8);
    ui.add(Campo::novo("Nome do cliente", &mut estado.novo_cliente_nome));
    ui.add_space(Espaco::E8);
    ui.add(Campo::novo("CPF do cliente", &mut estado.novo_cliente_cpf));
    ui.add_space(Espaco::E8);
    ui.add(Campo::novo("Equipamento", &mut estado.novo_equipamento));
    ui.add_space(Espaco::E12);
    if ui.add(Botao::primario("Abrir OS")).clicked() {
        abrir_os(motor, sessao, estado);
    }
}

fn abrir_os(motor: &MotorLocal, sessao: &SessaoLocal, estado: &mut EstadoTelaOs) {
    let cliente = motor.executar(
        sessao,
        "clientes.criar_pessoa.v1",
        &CriarPessoa {
            tipo: TipoPessoa::Fisica,
            nome: estado.novo_cliente_nome.clone(),
            nome_fantasia: None,
            papel_inicial: PapelCliente::Cliente,
            documento_tipo: TipoDocumento::Cpf,
            documento_numero: estado.novo_cliente_cpf.clone(),
        },
    );
    let cliente: PessoaCadastrada = match cliente {
        Ok(c) => c,
        Err(e) => {
            estado.erro = Some(e.mensagem);
            return;
        }
    };
    let aberta = motor.executar(
        sessao,
        "os.abrir_ordem_servico.v1",
        &AbrirOrdemServico {
            cliente: cliente.pessoa,
            equipamento: estado.novo_equipamento.clone(),
            tecnico_responsavel: sessao.usuario(),
            garantia_dias: 90,
        },
    );
    match aberta {
        Ok(aberta) => {
            let aberta: OrdemServicoAberta = aberta;
            estado.novo_cliente_nome.clear();
            estado.novo_cliente_cpf.clear();
            estado.novo_equipamento.clear();
            estado.erro = None;
            estado.carregar(motor, sessao);
            estado.selecionar(motor, sessao, aberta.ordem_servico);
        }
        Err(e) => estado.erro = Some(e.mensagem),
    }
}

fn mostrar_lista(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
) {
    ui.add(Rotulo::titulo_secao("Em aberto"));
    ui.add_space(Espaco::E8);
    if estado.ordens.is_empty() {
        EstadoVazio::novo(Icone::Ferramenta, "Nenhuma ordem em aberto.").mostrar(ui);
        return;
    }

    let colunas = vec![
        ColunaGrade::nova("Nº").largura(56.0),
        ColunaGrade::nova("Equipamento"),
        ColunaGrade::nova("Estado").largura(140.0),
    ];
    let sel = estado
        .selecionada
        .and_then(|id| estado.ordens.iter().position(|o| o.id == id));

    let clicada = Grade::nova(colunas).selecionavel(sel).mostrar(
        ui,
        estado.ordens.len(),
        |i, row| {
            let os = &estado.ordens[i];
            row.col(|ui| {
                ui.add(Rotulo::interface(os.numero.to_string()));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(os.equipamento.clone()));
            });
            row.col(|ui| {
                ui.add(Rotulo::campo(os.estado.rotulo()));
            });
        },
    );
    if let Some(i) = clicada {
        let id = estado.ordens[i].id;
        estado.selecionar(motor, sessao, id);
    }
}

fn mostrar_detalhe(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
) {
    let Some(detalhe) = estado.detalhe.clone() else {
        EstadoVazio::novo(Icone::Ferramenta, "Selecione uma ordem à esquerda.").mostrar(ui);
        return;
    };
    let os = &detalhe.ordem;

    ui.add(Rotulo::titulo_secao(format!(
        "OS #{} · {}",
        os.numero, os.equipamento
    )));
    ui.add(Rotulo::campo(os.estado.rotulo()));
    ui.add_space(Espaco::E16);

    // Laudo.
    if let Some(laudo) = &detalhe.laudo {
        ui.add(Rotulo::interface(format!(
            "Problema relatado: {}",
            laudo.descricao_problema
        )).quebravel());
        if let Some(diagnostico) = &laudo.diagnostico {
            ui.add(Rotulo::interface(format!("Diagnóstico: {diagnostico}")).quebravel());
        }
    } else if matches!(os.estado, EstadoOs::Aberta) {
        ui.add(Rotulo::titulo_secao("Laudo técnico"));
        ui.add_space(Espaco::E8);
        ui.add(Campo::novo("Problema relatado", &mut estado.laudo_problema));
        ui.add_space(Espaco::E8);
        ui.add(Campo::novo(
            "Diagnóstico (opcional)",
            &mut estado.laudo_diagnostico,
        ));
        ui.add_space(Espaco::E8);
        if ui.add(Botao::primario("Registrar laudo")).clicked() {
            let diagnostico = (!estado.laudo_diagnostico.trim().is_empty())
                .then(|| estado.laudo_diagnostico.clone());
            let resultado = motor.executar(
                sessao,
                "os.registrar_laudo.v1",
                &RegistrarLaudo {
                    ordem_servico: os.id,
                    descricao_problema: estado.laudo_problema.clone(),
                    diagnostico,
                    tecnico: sessao.usuario(),
                },
            );
            match resultado {
                Ok(_id) => {
                    let _: Id = _id;
                    estado.laudo_problema.clear();
                    estado.laudo_diagnostico.clear();
                    estado.selecionar(motor, sessao, os.id);
                    estado.carregar(motor, sessao);
                }
                Err(e) => estado.erro = Some(e.mensagem),
            }
        }
    }
    ui.add_space(Espaco::E16);

    // Orçamento.
    ui.add(Rotulo::titulo_secao("Orçamento"));
    ui.add_space(Espaco::E8);
    for item in &detalhe.itens_mao_de_obra {
        ui.horizontal(|ui| {
            ui.add(Rotulo::interface(item.descricao.clone()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(ValorDinheiro::novo(item.valor));
            });
        });
    }
    for item in &detalhe.itens_peca {
        ui.horizontal(|ui| {
            ui.add(Rotulo::interface("Peça do estoque"));
            let aplicada = if item.aplicada {
                "(aplicada)"
            } else {
                "(pendente de aplicação)"
            };
            ui.add(Rotulo::campo(aplicada));
        });
    }
    ui.add_space(Espaco::E4);
    ui.horizontal(|ui| {
        ui.add(Rotulo::campo("Total"));
        ui.add(ValorDinheiro::novo(os.valor_total));
    });

    if os.estado.aceita_edicao_de_orcamento() {
        ui.add_space(Espaco::E12);
        ui.add(Campo::novo(
            "Descrição do serviço",
            &mut estado.mao_de_obra_descricao,
        ));
        ui.add_space(Espaco::E8);
        ui.add(
            Campo::novo("Valor", &mut estado.mao_de_obra_valor).marcador("80,00"),
        );
        ui.add_space(Espaco::E8);
        if ui
            .add(Botao::secundario("Adicionar item de mão de obra"))
            .clicked()
        {
            match estado.mao_de_obra_valor.parse::<Dinheiro>() {
                Ok(valor) => {
                    let resultado = motor.executar(
                        sessao,
                        "os.montar_orcamento.v1",
                        &MontarOrcamentoOs {
                            ordem_servico: os.id,
                            item: ItemOrcamentoNovo::MaoDeObra {
                                descricao: estado.mao_de_obra_descricao.clone(),
                                valor,
                                tecnico: sessao.usuario(),
                                horas: None,
                            },
                        },
                    );
                    match resultado {
                        Ok(_id) => {
                            let _: Id = _id;
                            estado.mao_de_obra_descricao.clear();
                            estado.mao_de_obra_valor.clear();
                            estado.selecionar(motor, sessao, os.id);
                        }
                        Err(e) => estado.erro = Some(e.mensagem),
                    }
                }
                Err(_) => {
                    estado.erro = Some("Valor inválido — use o formato 80,00".to_string());
                }
            }
        }

        if !os.valor_total.e_zero()
            && ui.add(Botao::primario("Enviar para aprovação")).clicked()
        {
            aplicar(
                motor,
                sessao,
                estado,
                os.id,
                "os.enviar_para_aprovacao.v1",
                &EnviarParaAprovacao {
                    ordem_servico: os.id,
                },
            );
        }
    }

    ui.add_space(Espaco::E16);
    acoes_por_estado(ui, motor, sessao, estado, &detalhe);
}

fn acoes_por_estado(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
    detalhe: &DetalheOrdem,
) {
    let os = &detalhe.ordem;
    match os.estado {
        EstadoOs::AguardandoAprovacao => {
            ui.add(Rotulo::titulo_secao("Aprovação do cliente"));
            ui.add_space(Espaco::E8);
            ui.add(Campo::novo(
                "Nome (e documento) de quem aprovou",
                &mut estado.aprovador,
            ));
            ui.add_space(Espaco::E8);
            ui.horizontal(|ui| {
                if ui.add(Botao::primario("Aprovar")).clicked() {
                    aplicar(
                        motor,
                        sessao,
                        estado,
                        os.id,
                        "os.aprovar_orcamento.v1",
                        &AprovarOrcamentoOs {
                            ordem_servico: os.id,
                            identificacao_aprovador: estado.aprovador.clone(),
                        },
                    );
                }
                if ui.add(Botao::destrutivo("Reprovar")).clicked() {
                    aplicar(
                        motor,
                        sessao,
                        estado,
                        os.id,
                        "os.reprovar_orcamento.v1",
                        &ReprovarOrcamentoOs {
                            ordem_servico: os.id,
                        },
                    );
                }
            });
        }
        EstadoOs::Aprovada => {
            if ui.add(Botao::primario("Iniciar execução")).clicked() {
                aplicar(
                    motor,
                    sessao,
                    estado,
                    os.id,
                    "os.iniciar_execucao.v1",
                    &IniciarExecucao {
                        ordem_servico: os.id,
                    },
                );
            }
        }
        EstadoOs::EmExecucao => {
            let pendente = detalhe.itens_peca.iter().any(|i| !i.aplicada);
            if pendente {
                ui.add(
                    Rotulo::interface("Há peça do orçamento ainda não aplicada no estoque.")
                        .cor(ui.cores().atencao),
                );
            } else if ui.add(Botao::primario("Concluir execução")).clicked() {
                aplicar(
                    motor,
                    sessao,
                    estado,
                    os.id,
                    "os.concluir_execucao.v1",
                    &ConcluirExecucao {
                        ordem_servico: os.id,
                    },
                );
            }
        }
        EstadoOs::Concluida => {
            if ui.add(Botao::primario("Faturar")).clicked() {
                aplicar(
                    motor,
                    sessao,
                    estado,
                    os.id,
                    "os.faturar_ordem_servico.v1",
                    &FaturarOrdemServico {
                        ordem_servico: os.id,
                    },
                );
            }
        }
        EstadoOs::Faturada
        | EstadoOs::Cancelada
        | EstadoOs::Reprovada
        | EstadoOs::Aberta
        | EstadoOs::EmDiagnostico => {}
    }
}

fn aplicar<C: cardeal_modkit::Comando + serde::Serialize>(
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
    ordem_servico: Id,
    nome: &str,
    comando: &C,
) where
    C::Saida: serde::de::DeserializeOwned,
{
    match motor.executar(sessao, nome, comando) {
        Ok(_saida) => {
            estado.selecionar(motor, sessao, ordem_servico);
            estado.carregar(motor, sessao);
        }
        Err(e) => estado.erro = Some(e.mensagem),
    }
}
