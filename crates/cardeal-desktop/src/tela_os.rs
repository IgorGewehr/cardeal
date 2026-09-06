//! Tela de Ordens de Serviço — lista + dialog (criar / ver / agir). `docs/modulos/os.md`.
//!
//! Padrão de UI do projeto: a tela abre na lista inteira; "Nova OS" e o clique numa linha
//! abrem o mesmo `Dialogo`. OS é workflow (máquina de estados), então o dialog de detalhe
//! mostra as infos + a ação certa pro estado atual, em vez de um "Editar" genérico.

use cardeal_cliente::{MotorLocal, SessaoLocal};
use cardeal_kernel::{Dinheiro, Id};
use cardeal_modkit::Icone;
use cardeal_ui::atoms::{Botao, Rotulo, ValorDinheiro};
use cardeal_ui::molecules::{Campo, EstadoVazio};
use cardeal_ui::organisms::{ColunaGrade, Dialogo, Grade, LayoutTela};
use cardeal_ui::tokens::{Espaco, TemaUi};
use eframe::egui;
use mod_clientes::{CriarPessoa, Papel as PapelCliente, PessoaCadastrada, TipoDocumento, TipoPessoa};
use mod_os::{
    AbrirOrdemServico, AprovarOrcamentoOs, BuscarDetalheOrdem, ConcluirExecucao, DetalheOrdem,
    EnviarParaAprovacao, EstadoOs, FaturarOrdemServico, IniciarExecucao, ItemOrcamentoNovo,
    MontarOrcamentoOs, OrdemServico, OrdemServicoAberta, OrdensEmAberto, RegistrarLaudo,
    ReprovarOrcamentoOs,
};

/// Qual dialog está aberto.
#[derive(Default)]
enum Dlg {
    #[default]
    Fechado,
    Nova {
        nome: String,
        cpf: String,
        equipamento: String,
    },
    Detalhe,
}

/// Estado local da tela — sobrevive entre quadros, não entre reinícios.
#[derive(Default)]
pub struct EstadoTelaOs {
    ordens: Vec<OrdemServico>,
    detalhe: Option<DetalheOrdem>,
    erro: Option<String>,
    dlg: Dlg,

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

    fn abrir_detalhe(&mut self, motor: &MotorLocal, sessao: &SessaoLocal, id: Id) {
        match motor.consultar(
            sessao,
            "os.buscar_detalhe_ordem.v1",
            &BuscarDetalheOrdem { ordem_servico: id },
        ) {
            Ok(detalhe) => {
                self.detalhe = detalhe;
                self.dlg = Dlg::Detalhe;
                self.limpar_campos();
            }
            Err(e) => self.erro = Some(e.mensagem),
        }
    }

    fn limpar_campos(&mut self) {
        self.laudo_problema.clear();
        self.laudo_diagnostico.clear();
        self.mao_de_obra_descricao.clear();
        self.mao_de_obra_valor.clear();
        self.aprovador.clear();
    }
}

/// Desenha a tela inteira.
pub fn mostrar(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
) {
    LayoutTela::nova("Ordens de Serviço").mostrar(
        ui,
        estado,
        |ui, estado| {
            if ui.add(Botao::secundario("Recarregar").atalho("F5")).clicked() {
                estado.carregar(motor, sessao);
            }
            if ui.add(Botao::primario("+ Nova OS").atalho("Ctrl+N")).clicked() {
                estado.dlg = Dlg::Nova {
                    nome: String::new(),
                    cpf: String::new(),
                    equipamento: String::new(),
                };
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
            lista(ui, motor, sessao, estado);
        },
    );

    match estado.dlg {
        Dlg::Fechado => {}
        Dlg::Nova { .. } => dialogo_nova(ui.ctx(), motor, sessao, estado),
        Dlg::Detalhe => dialogo_detalhe(ui.ctx(), motor, sessao, estado),
    }
}

fn lista(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
) {
    if estado.ordens.is_empty() {
        if EstadoVazio::novo(Icone::Ferramenta, "Nenhuma ordem em aberto.")
            .acao("Abrir a primeira OS")
            .mostrar(ui)
        {
            estado.dlg = Dlg::Nova {
                nome: String::new(),
                cpf: String::new(),
                equipamento: String::new(),
            };
        }
        return;
    }

    let colunas = vec![
        ColunaGrade::nova("Nº").largura(56.0),
        ColunaGrade::nova("Equipamento"),
        ColunaGrade::nova("Estado").largura(160.0),
        ColunaGrade::nova("Total").largura(120.0),
    ];
    let clicada = Grade::nova(colunas).selecionavel(None).mostrar(
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
            row.col(|ui| {
                ui.add(ValorDinheiro::novo(os.valor_total));
            });
        },
    );
    if let Some(i) = clicada {
        let id = estado.ordens[i].id;
        estado.abrir_detalhe(motor, sessao, id);
    }
}

fn dialogo_nova(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
) {
    let fechar = Dialogo::nova("Nova ordem de serviço").largura(640.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            let Dlg::Nova {
                nome,
                cpf,
                equipamento,
            } = &mut estado.dlg
            else {
                return;
            };
            ui.columns(2, |c| {
                c[0].add(Campo::novo("Nome do cliente", nome));
                c[1].add(Campo::novo("CPF do cliente", cpf).marcador("000.000.000-00"));
            });
            ui.add_space(Espaco::E12);
            ui.add(Campo::novo("Equipamento", equipamento).marcador("ex.: Furadeira Bosch GSB 13"));
        },
        |ui, estado| {
            if ui.add(Botao::primario("Abrir OS")).clicked() {
                abrir_os(motor, sessao, estado);
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

fn abrir_os(motor: &MotorLocal, sessao: &SessaoLocal, estado: &mut EstadoTelaOs) {
    let Dlg::Nova {
        nome,
        cpf,
        equipamento,
    } = &estado.dlg
    else {
        return;
    };
    let (nome, cpf, equipamento) = (nome.clone(), cpf.clone(), equipamento.clone());

    let cliente = motor.executar(
        sessao,
        "clientes.criar_pessoa.v1",
        &CriarPessoa {
            tipo: TipoPessoa::Fisica,
            nome,
            nome_fantasia: None,
            papel_inicial: PapelCliente::Cliente,
            documento_tipo: TipoDocumento::Cpf,
            documento_numero: cpf,
        },
    );
    let cliente: PessoaCadastrada = match cliente {
        Ok(c) => c,
        Err(e) => {
            estado.erro = Some(e.mensagem);
            return;
        }
    };
    match motor.executar(
        sessao,
        "os.abrir_ordem_servico.v1",
        &AbrirOrdemServico {
            cliente: cliente.pessoa,
            equipamento,
            tecnico_responsavel: sessao.usuario(),
            garantia_dias: 90,
        },
    ) {
        Ok(aberta) => {
            let aberta: OrdemServicoAberta = aberta;
            estado.erro = None;
            estado.carregar(motor, sessao);
            estado.abrir_detalhe(motor, sessao, aberta.ordem_servico);
        }
        Err(e) => estado.erro = Some(e.mensagem),
    }
}

fn dialogo_detalhe(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
) {
    let Some(detalhe) = estado.detalhe.clone() else {
        estado.dlg = Dlg::Fechado;
        return;
    };
    let os = detalhe.ordem.clone();
    let titulo = format!("OS #{} · {}", os.numero, os.equipamento);

    let fechar = Dialogo::nova(titulo).mostrar(
        ctx,
        estado,
        |ui, estado| corpo_detalhe(ui, motor, sessao, estado, &detalhe),
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

fn corpo_detalhe(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
    detalhe: &DetalheOrdem,
) {
    let os = &detalhe.ordem;
    ui.horizontal(|ui| {
        ui.add(Rotulo::campo("Estado"));
        ui.add(Rotulo::interface(os.estado.rotulo()));
    });
    ui.add_space(Espaco::E16);

    // ── Laudo ──────────────────────────────────────────────────────────────
    ui.add(Rotulo::titulo_secao("Laudo técnico"));
    ui.add_space(Espaco::E8);
    if let Some(laudo) = &detalhe.laudo {
        ui.add(Rotulo::interface(format!("Problema: {}", laudo.descricao_problema)).quebravel());
        if let Some(dg) = &laudo.diagnostico {
            ui.add(Rotulo::interface(format!("Diagnóstico: {dg}")).quebravel());
        }
    } else if matches!(os.estado, EstadoOs::Aberta) {
        ui.columns(2, |c| {
            c[0].add(Campo::novo("Problema relatado", &mut estado.laudo_problema));
            c[1].add(Campo::novo("Diagnóstico (opcional)", &mut estado.laudo_diagnostico));
        });
        ui.add_space(Espaco::E8);
        if ui.add(Botao::primario("Registrar laudo")).clicked() {
            let diagnostico = (!estado.laudo_diagnostico.trim().is_empty())
                .then(|| estado.laudo_diagnostico.clone());
            aplicar_e_recarregar(
                motor,
                sessao,
                estado,
                os.id,
                "os.registrar_laudo.v1",
                &RegistrarLaudo {
                    ordem_servico: os.id,
                    descricao_problema: estado.laudo_problema.clone(),
                    diagnostico,
                    tecnico: sessao.usuario(),
                },
            );
        }
    } else {
        ui.add(Rotulo::interface("—"));
    }
    ui.add_space(Espaco::E16);

    // ── Orçamento ──────────────────────────────────────────────────────────
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
            ui.add(Rotulo::campo(if item.aplicada {
                "(aplicada)"
            } else {
                "(pendente)"
            }));
        });
    }
    ui.add_space(Espaco::E4);
    ui.horizontal(|ui| {
        ui.add(Rotulo::campo("Total"));
        ui.add(ValorDinheiro::novo(os.valor_total));
    });

    if os.estado.aceita_edicao_de_orcamento() {
        ui.add_space(Espaco::E12);
        ui.columns(2, |c| {
            c[0].add(Campo::novo("Serviço", &mut estado.mao_de_obra_descricao));
            c[1].add(Campo::novo("Valor", &mut estado.mao_de_obra_valor).marcador("80,00"));
        });
        ui.add_space(Espaco::E8);
        ui.horizontal(|ui| {
            if ui.add(Botao::secundario("+ Adicionar mão de obra")).clicked() {
                adicionar_mao_de_obra(motor, sessao, estado, os.id);
            }
            if !os.valor_total.e_zero()
                && ui.add(Botao::primario("Enviar para aprovação")).clicked()
            {
                aplicar_e_recarregar(
                    motor,
                    sessao,
                    estado,
                    os.id,
                    "os.enviar_para_aprovacao.v1",
                    &EnviarParaAprovacao { ordem_servico: os.id },
                );
            }
        });
    }

    ui.add_space(Espaco::E16);
    acoes_por_estado(ui, motor, sessao, estado, detalhe);
}

fn adicionar_mao_de_obra(
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
    os: Id,
) {
    match estado.mao_de_obra_valor.parse::<Dinheiro>() {
        Ok(valor) => {
            let r = motor.executar(
                sessao,
                "os.montar_orcamento.v1",
                &MontarOrcamentoOs {
                    ordem_servico: os,
                    item: ItemOrcamentoNovo::MaoDeObra {
                        descricao: estado.mao_de_obra_descricao.clone(),
                        valor,
                        tecnico: sessao.usuario(),
                        horas: None,
                    },
                },
            );
            match r {
                Ok(id) => {
                    let _: Id = id;
                    estado.mao_de_obra_descricao.clear();
                    estado.mao_de_obra_valor.clear();
                    estado.abrir_detalhe(motor, sessao, os);
                    estado.dlg = Dlg::Detalhe;
                }
                Err(e) => estado.erro = Some(e.mensagem),
            }
        }
        Err(_) => estado.erro = Some("Valor inválido — use o formato 80,00".to_owned()),
    }
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
            ui.add(Campo::novo("Quem aprovou (nome e documento)", &mut estado.aprovador));
            ui.add_space(Espaco::E8);
            ui.horizontal(|ui| {
                if ui.add(Botao::primario("Aprovar")).clicked() {
                    aplicar_e_recarregar(
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
                    aplicar_e_recarregar(
                        motor,
                        sessao,
                        estado,
                        os.id,
                        "os.reprovar_orcamento.v1",
                        &ReprovarOrcamentoOs { ordem_servico: os.id },
                    );
                }
            });
        }
        EstadoOs::Aprovada => {
            if ui.add(Botao::primario("Iniciar execução")).clicked() {
                aplicar_e_recarregar(
                    motor,
                    sessao,
                    estado,
                    os.id,
                    "os.iniciar_execucao.v1",
                    &IniciarExecucao { ordem_servico: os.id },
                );
            }
        }
        EstadoOs::EmExecucao => {
            if detalhe.itens_peca.iter().any(|i| !i.aplicada) {
                ui.add(
                    Rotulo::interface("Há peça do orçamento ainda não aplicada no estoque.")
                        .cor(ui.cores().atencao),
                );
            } else if ui.add(Botao::primario("Concluir execução")).clicked() {
                aplicar_e_recarregar(
                    motor,
                    sessao,
                    estado,
                    os.id,
                    "os.concluir_execucao.v1",
                    &ConcluirExecucao { ordem_servico: os.id },
                );
            }
        }
        EstadoOs::Concluida => {
            if ui.add(Botao::primario("Faturar")).clicked() {
                aplicar_e_recarregar(
                    motor,
                    sessao,
                    estado,
                    os.id,
                    "os.faturar_ordem_servico.v1",
                    &FaturarOrdemServico { ordem_servico: os.id },
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

fn aplicar_e_recarregar<C: cardeal_modkit::Comando + serde::Serialize>(
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
    ordem: Id,
    nome: &str,
    comando: &C,
) where
    C::Saida: serde::de::DeserializeOwned,
{
    match motor.executar(sessao, nome, comando) {
        Ok(_) => {
            estado.carregar(motor, sessao);
            estado.abrir_detalhe(motor, sessao, ordem);
            estado.dlg = Dlg::Detalhe;
        }
        Err(e) => estado.erro = Some(e.mensagem),
    }
}
