//! A aba Contas bancárias: o caixa e os bancos com o saldo de cada um, o **saldo inicial** (o
//! primeiro dia de qualquer cliente), a **transferência** entre contas (depositar o caixa no
//! banco), a lista das transferências com o cancelamento, e **desativar** a conta que não se usa.

use mod_financeiro::{
    CancelarTransferencia, DefinirSaldoInicial, DesativarContaDisponivel, ItemTransferencia,
    SaldoInicialDefinido, TransferenciaFeita, TransferirEntreContas,
};

use super::*;

/// Os campos do diálogo de transferência.
#[derive(Debug, Clone, Default)]
pub(super) struct FormTransferencia {
    pub(super) origem: Option<Id>,
    pub(super) destino: Option<Id>,
    pub(super) valor: String,
    pub(super) data: String,
    pub(super) observacao: String,
}

fn hoje_texto() -> String {
    Data::hoje(Fuso::BRASILIA).to_string()
}

fn valor_de(texto: &str) -> Option<Dinheiro> {
    Dinheiro::de_str(texto.trim()).ok()
}

pub(super) fn painel_bancos(ui: &mut egui::Ui, sessao: &Sessao, estado: &mut EstadoTelaFinanceiro) {
    if estado.contas_disp.is_empty() {
        if EstadoVazio::novo(Icone::Dinheiro, "Nenhuma conta de caixa ou banco ainda.")
            .acao("+ Nova conta bancária")
            .mostrar(ui)
        {
            estado.dlg = Dlg::NovaContaBancaria {
                nome: String::new(),
                saldo: String::new(),
            };
        }
        return;
    }
    let total: Dinheiro = estado
        .contas_disp
        .iter()
        .map(|c| c.saldo)
        .fold(Dinheiro::ZERO, |a, b| a + b);
    let editar = sessao.concede("financeiro.banco.editar");
    let transferir = sessao.concede("financeiro.banco.transferir");

    let mut colunas = vec![
        ColunaGrade::nova("Conta"),
        ColunaGrade::nova("Código").largura(90.0),
        ColunaGrade::nova("Tipo").largura(90.0),
        ColunaGrade::nova("Saldo").largura(140.0).numero(),
    ];
    if editar {
        colunas.push(ColunaGrade::nova("").largura(220.0));
    }
    let mut nova = false;
    let mut nova_transferencia = false;
    let mut saldo_inicial: Option<(Id, String)> = None;
    let mut desativar: Option<(Id, String)> = None;
    Painel::novo()
        .titulo(format!(
            "Total disponível · {}",
            total.formatar_com_simbolo()
        ))
        .compacto()
        .mostrar_com_acoes(
            ui,
            |ui| {
                nova = ui
                    .add(Botao::secundario("+ Nova conta bancária").pequeno())
                    .clicked();
                if transferir {
                    nova_transferencia =
                        ui.add(Botao::secundario("Transferir").pequeno()).clicked();
                }
            },
            |ui| {
                Grade::nova(colunas).mostrar(ui, estado.contas_disp.len(), |i, row| {
                    let c = &estado.contas_disp[i];
                    row.col(|ui| {
                        ui.add(Rotulo::interface(c.nome.clone()));
                    });
                    row.col(|ui| {
                        ui.add(Rotulo::campo(c.codigo.clone()));
                    });
                    row.col(|ui| {
                        ui.add(Rotulo::campo(if c.e_caixa { "Caixa" } else { "Banco" }));
                    });
                    row.col(|ui| {
                        ui.add(ValorDinheiro::novo(c.saldo));
                    });
                    if editar {
                        row.col(|ui| {
                            ui.horizontal(|ui| {
                                if ui.add(Botao::fantasma("Saldo inicial").pequeno()).clicked() {
                                    saldo_inicial = Some((c.conta, c.nome.clone()));
                                }
                                if ui.add(Botao::fantasma("Desativar").pequeno()).clicked() {
                                    desativar = Some((c.conta, c.nome.clone()));
                                }
                            });
                        });
                    }
                });
            },
        );
    if nova {
        estado.dlg = Dlg::NovaContaBancaria {
            nome: String::new(),
            saldo: String::new(),
        };
    }
    if nova_transferencia {
        estado.dlg = Dlg::Transferir(FormTransferencia {
            data: hoje_texto(),
            ..FormTransferencia::default()
        });
    }
    if let Some((conta, nome)) = saldo_inicial {
        estado.dlg = Dlg::SaldoInicial {
            conta,
            nome,
            valor: String::new(),
            data: hoje_texto(),
        };
    }
    if let Some((conta, nome)) = desativar {
        estado.dlg = Dlg::DesativarConta { conta, nome };
    }

    if !estado.transferencias.is_empty() {
        ui.add_space(Espaco::E16);
        lista_transferencias(ui, transferir, estado);
    }
}

fn lista_transferencias(ui: &mut egui::Ui, pode_cancelar: bool, estado: &mut EstadoTelaFinanceiro) {
    let mut cancelar: Option<(Id, String)> = None;
    Painel::novo()
        .titulo("Transferências")
        .compacto()
        .mostrar(ui, |ui| {
            let colunas = vec![
                ColunaGrade::nova("Data").largura(100.0),
                ColunaGrade::nova("De"),
                ColunaGrade::nova("Para"),
                ColunaGrade::nova("Valor").largura(130.0).numero(),
                ColunaGrade::nova("").largura(120.0),
            ];
            Grade::nova(colunas)
                .id_salt("financeiro-transferencias")
                .mostrar(ui, estado.transferencias.len(), |i, row| {
                    let t = &estado.transferencias[i];
                    row.col(|ui| {
                        ui.add(Rotulo::campo(t.data.to_string()));
                    });
                    row.col(|ui| {
                        ui.add(Rotulo::interface(t.origem.clone()));
                    });
                    row.col(|ui| {
                        ui.add(Rotulo::interface(t.destino.clone()));
                    });
                    row.col(|ui| {
                        ui.add(ValorDinheiro::novo(t.valor));
                    });
                    row.col(|ui| {
                        if t.cancelada_em.is_some() {
                            ui.add(Etiqueta::neutra("cancelada"));
                        } else if pode_cancelar
                            && ui.add(Botao::fantasma("Cancelar").pequeno()).clicked()
                        {
                            cancelar = Some((
                                t.id,
                                format!(
                                    "{} de {} para {} em {}",
                                    t.valor.formatar_com_simbolo(),
                                    t.origem,
                                    t.destino,
                                    t.data.formatar()
                                ),
                            ));
                        }
                    });
                });
        });
    if let Some((id, descricao)) = cancelar {
        estado.dlg = Dlg::CancelarTransferencia {
            id,
            descricao,
            motivo: String::new(),
        };
    }
}

/// Recarrega as contas e as transferências depois de uma mudança, avisa e fecha o diálogo.
fn concluir(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
    mensagem: String,
) {
    estado.dlg = Dlg::Fechado;
    estado.carregar_fluxo(motor, sessao);
    notificar(ctx, Notificacao::sucesso(mensagem));
}

pub(super) fn dialogo_conta_bancaria(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) {
    let fechar = Dialogo::nova("Nova conta bancária").largura(460.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            let Dlg::NovaContaBancaria { nome, saldo } = &mut estado.dlg else {
                return;
            };
            ui.add(Rotulo::apoio(
                "Entra no plano como conta analítica do grupo 1.1 (Disponível).",
            ));
            ui.add_space(Espaco::E8);
            ui.add(Campo::novo("Nome", nome).marcador("Nubank — cc 12345-6"));
            ui.add_space(Espaco::E8);
            ui.add(Campo::novo("Saldo hoje (opcional)", saldo).marcador("0,00"));
        },
        |ui, estado| {
            if ui.add(Botao::primario("Criar")).clicked() {
                criar_conta(ui.ctx(), motor, sessao, estado);
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

pub(super) fn criar_conta(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) {
    let Dlg::NovaContaBancaria { nome, saldo } = &estado.dlg else {
        return;
    };
    let nome = nome.trim().to_owned();
    if nome.is_empty() {
        notificar(ctx, Notificacao::aviso("Informe o nome da conta."));
        return;
    }
    let saldo = match saldo.trim() {
        "" => None,
        t => match valor_de(t) {
            Some(v) => Some(v),
            None => {
                notificar(ctx, Notificacao::aviso("Saldo: um valor como 1.250,00."));
                return;
            }
        },
    };
    let criada: Result<ContaBancariaCriada, _> = motor.executar(
        sessao,
        "financeiro.criar_conta_bancaria.v1",
        &CriarContaBancaria { nome },
    );
    let criada = match criada {
        Ok(c) => c,
        Err(e) => {
            notificar(ctx, Notificacao::erro(e.mensagem));
            return;
        }
    };
    if let Some(saldo) = saldo.filter(|s| !s.e_zero()) {
        let r: Result<SaldoInicialDefinido, _> = motor.executar(
            sessao,
            "financeiro.definir_saldo_inicial.v1",
            &DefinirSaldoInicial {
                conta: criada.conta,
                saldo,
                data: Data::hoje(Fuso::BRASILIA),
            },
        );
        if let Err(e) = r {
            notificar(
                ctx,
                Notificacao::erro("A conta foi criada, mas o saldo inicial não")
                    .detalhe(e.mensagem),
            );
        }
    }
    concluir(
        ctx,
        motor,
        sessao,
        estado,
        "Conta bancária criada".to_owned(),
    );
}

pub(super) fn dialogo_saldo_inicial(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) {
    let Dlg::SaldoInicial { nome, .. } = &estado.dlg else {
        return;
    };
    let titulo = format!("Saldo inicial · {nome}");
    let mut confirmar = false;
    let fechar = Dialogo::nova(titulo).largura(460.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            let Dlg::SaldoInicial { valor, data, .. } = &mut estado.dlg else {
                return;
            };
            ui.add(
                Rotulo::apoio(
                    "Quanto havia nesta conta no dia em que você começou a usar o Cardeal. \
                     Redefinir substitui o anterior; negativo para banco no cheque especial.",
                )
                .quebravel(),
            );
            ui.add_space(Espaco::E8);
            ui.columns(2, |c| {
                c[0].add(Campo::novo("Saldo", valor).marcador("0,00"));
                c[1].add(Campo::novo("Em", data).mascara(Mascara::Data));
            });
        },
        |ui, estado| {
            confirmar = ui.add(Botao::primario("Salvar")).clicked();
            if ui.add(Botao::secundario("Cancelar")).clicked() {
                estado.dlg = Dlg::Fechado;
            }
        },
    );
    if confirmar {
        let Dlg::SaldoInicial {
            conta, valor, data, ..
        } = &estado.dlg
        else {
            return;
        };
        let (Some(saldo), Ok(data)) = (valor_de(valor), data.parse::<Data>()) else {
            notificar(
                ctx,
                Notificacao::aviso("Informe o saldo (ex.: 1.250,00) e a data."),
            );
            return;
        };
        let r: Result<SaldoInicialDefinido, _> = motor.executar(
            sessao,
            "financeiro.definir_saldo_inicial.v1",
            &DefinirSaldoInicial {
                conta: *conta,
                saldo,
                data,
            },
        );
        match r {
            Ok(s) => concluir(
                ctx,
                motor,
                sessao,
                estado,
                format!(
                    "Saldo atual da conta: {}",
                    s.saldo_atual.formatar_com_simbolo()
                ),
            ),
            Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
        }
        return;
    }
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

pub(super) fn dialogo_transferir(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) {
    let opcoes: Vec<(Id, String)> = estado
        .contas_disp
        .iter()
        .map(|c| {
            (
                c.conta,
                format!("{} · {}", c.nome, c.saldo.formatar_com_simbolo()),
            )
        })
        .collect();
    let mut confirmar = false;
    let fechar = Dialogo::nova("Transferir entre contas")
        .largura(520.0)
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                let Dlg::Transferir(f) = &mut estado.dlg else {
                    return;
                };
                ui.columns(2, |c| {
                    SeletorOpcao::novo("De", &mut f.origem)
                        .opcoes(opcoes.iter().cloned())
                        .placeholder("Escolha a conta")
                        .mostrar(&mut c[0]);
                    SeletorOpcao::novo("Para", &mut f.destino)
                        .opcoes(opcoes.iter().cloned())
                        .placeholder("Escolha a conta")
                        .mostrar(&mut c[1]);
                });
                ui.add_space(Espaco::E8);
                ui.columns(2, |c| {
                    c[0].add(Campo::novo("Valor", &mut f.valor).marcador("0,00"));
                    c[1].add(Campo::novo("Data", &mut f.data).mascara(Mascara::Data));
                });
                ui.add_space(Espaco::E8);
                ui.add(
                    Campo::novo("Observação (opcional)", &mut f.observacao)
                        .marcador("Depósito do caixa"),
                );
            },
            |ui, _| {
                confirmar = ui.add(Botao::primario("Transferir")).clicked();
            },
        );
    if confirmar {
        let Dlg::Transferir(f) = &estado.dlg else {
            return;
        };
        let (Some(origem), Some(destino)) = (f.origem, f.destino) else {
            notificar(
                ctx,
                Notificacao::aviso("Escolha a conta de origem e a de destino."),
            );
            return;
        };
        let (Some(valor), Ok(data)) = (valor_de(&f.valor), f.data.parse::<Data>()) else {
            notificar(ctx, Notificacao::aviso("Informe o valor e a data."));
            return;
        };
        let r: Result<TransferenciaFeita, _> = motor.executar(
            sessao,
            "financeiro.transferir_entre_contas.v1",
            &TransferirEntreContas {
                origem,
                destino,
                valor,
                data,
                observacao: Some(f.observacao.clone()),
            },
        );
        match r {
            Ok(_) => concluir(
                ctx,
                motor,
                sessao,
                estado,
                format!("Transferência de {} feita", valor.formatar_com_simbolo()),
            ),
            Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
        }
        return;
    }
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

pub(super) fn dialogo_cancelar_transferencia(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) {
    let Dlg::CancelarTransferencia { descricao, .. } = &estado.dlg else {
        return;
    };
    let descricao = descricao.clone();
    let mut confirmar = false;
    let fechar = Dialogo::nova("Cancelar transferência")
        .descricao(descricao)
        .largura(480.0)
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                let Dlg::CancelarTransferencia { motivo, .. } = &mut estado.dlg else {
                    return;
                };
                ui.add(
                    Rotulo::apoio(
                        "O dinheiro volta para a conta de origem (os dois lados são estornados).",
                    )
                    .quebravel(),
                );
                ui.add_space(Espaco::E8);
                ui.add(Campo::novo("Motivo", motivo).marcador("Depósito não foi feito"));
            },
            |ui, _| {
                confirmar = ui
                    .add(Botao::destrutivo("Cancelar transferência"))
                    .clicked();
            },
        );
    if confirmar {
        let Dlg::CancelarTransferencia { id, motivo, .. } = &estado.dlg else {
            return;
        };
        let r: Result<(), _> = motor.executar(
            sessao,
            "financeiro.cancelar_transferencia.v1",
            &CancelarTransferencia {
                transferencia: *id,
                motivo: motivo.clone(),
            },
        );
        match r {
            Ok(()) => concluir(
                ctx,
                motor,
                sessao,
                estado,
                "Transferência cancelada".to_owned(),
            ),
            Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
        }
        return;
    }
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

pub(super) fn confirmar_desativacao(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) {
    let Dlg::DesativarConta { conta, nome } = &estado.dlg else {
        return;
    };
    let (conta, nome) = (*conta, nome.clone());
    let resposta = cardeal_ui::organisms::dialogo_confirmacao(
        ctx,
        "Desativar conta",
        &format!(
            "{nome} deixa de aparecer nas listas e não recebe mais lançamentos. O histórico \
             continua. Só se desativa conta sem saldo."
        ),
        "Desativar",
    );
    if resposta.confirmado {
        let r: Result<(), _> = motor.executar(
            sessao,
            "financeiro.desativar_conta.v1",
            &DesativarContaDisponivel { conta },
        );
        match r {
            Ok(()) => concluir(ctx, motor, sessao, estado, format!("{nome} desativada")),
            Err(e) => {
                notificar(ctx, Notificacao::erro(e.mensagem));
                estado.dlg = Dlg::Fechado;
            }
        }
    } else if resposta.fechar {
        estado.dlg = Dlg::Fechado;
    }
}

/// As transferências, para a lista da aba.
pub(super) fn carregar_transferencias(motor: &Motor, sessao: &Sessao) -> Vec<ItemTransferencia> {
    if !sessao.concede("financeiro.banco.ver") {
        return Vec::new();
    }
    motor
        .consultar(
            sessao,
            "financeiro.transferencias.v1",
            &mod_financeiro::Transferencias,
        )
        .unwrap_or_default()
}
