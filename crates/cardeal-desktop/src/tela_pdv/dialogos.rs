//! Diálogos que falam com o motor: cadastrar/abrir caixa, desconto no item, cancelar cupom.

use super::*;

// ── diálogos ─────────────────────────────────────────────────────────────────

pub(super) fn dialogos(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaPdv,
) {
    match &estado.dlg {
        Dlg::Fechado => {}
        Dlg::CadastrarCaixa { .. } => dialogo_cadastrar(ctx, motor, sessao, estado),
        Dlg::AbrirCaixa { .. } => dialogo_abrir(ctx, motor, sessao, estado),
        Dlg::Pagamento(_) => dialogo_pagamento(ctx, estado),
        Dlg::Desconto { .. } => dialogo_desconto(ctx, motor, sessao, estado),
        Dlg::CancelarCupom { .. } => dialogo_cancelar_cupom(ctx, motor, sessao, estado),
        Dlg::FecharCaixa { .. } => dialogo_fechar_caixa(ctx, motor, sessao, estado),
        Dlg::Sangria { .. } => dialogo_sangria(ctx, motor, sessao, estado),
    }
}

pub(super) fn enter_pressionado(ui: &egui::Ui) -> bool {
    ui.input(|i| i.key_pressed(egui::Key::Enter))
}

pub(super) fn dialogo_cadastrar(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaPdv,
) {
    let contas: Vec<(Id, String)> = estado
        .contas_caixa
        .iter()
        .map(|c| (c.conta, format!("{} · {}", c.codigo, c.nome)))
        .collect();

    let fechar = Dialogo::nova("Cadastrar caixa")
        .descricao("Um caixa é o ponto de venda e a conta do Razão onde o dinheiro entra.")
        .pequeno()
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                let Dlg::CadastrarCaixa { nome, conta } = &mut estado.dlg else {
                    return;
                };
                ui.add(Campo::novo("Nome", nome).marcador("Caixa 1"));
                ui.add_space(Espaco::E12);
                SeletorOpcao::novo("Conta do Razão", conta)
                    .opcoes(contas.clone())
                    .placeholder("Conta de caixa (1.1.01)")
                    .mostrar(ui);
            },
            |ui, estado| {
                if ui.add(Botao::primario("Cadastrar")).clicked() {
                    if let Dlg::CadastrarCaixa { nome, conta } = &estado.dlg {
                        let (nome, conta) = (nome.clone(), *conta);
                        let Some(conta_razao) = conta else {
                            notificar(ctx, Notificacao::aviso("Escolha a conta do Razão."));
                            return;
                        };
                        match motor.executar(
                            sessao,
                            "financeiro.cadastrar_caixa.v1",
                            &CadastrarCaixa {
                                nome,
                                local_operacao: None,
                                conta_razao,
                                permite_negativo: false,
                            },
                        ) {
                            Ok(CaixaCadastrado { caixa }) => {
                                estado.caixa_sel = Some(caixa);
                                estado.dlg = Dlg::Fechado;
                                estado.carregar(motor, sessao);
                                notificar(ctx, Notificacao::sucesso("Caixa cadastrado"));
                            }
                            Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
                        }
                    }
                }
                if ui
                    .add(Botao::secundario("Cancelar").atalho("Esc"))
                    .clicked()
                {
                    estado.dlg = Dlg::Fechado;
                }
            },
        );
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

pub(super) fn dialogo_abrir(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaPdv,
) {
    let Some(caixa) = estado.caixa_sel else {
        estado.dlg = Dlg::Fechado;
        return;
    };
    // Abrir é a mesma ação por `Enter` no campo ou pelo botão.
    let abrir_caixa = |estado: &mut EstadoTelaPdv| {
        let Dlg::AbrirCaixa { valor } = &estado.dlg else {
            return;
        };
        let Ok(valor_abertura) = valor.parse::<Dinheiro>() else {
            notificar(ctx, Notificacao::aviso("Valor inválido (0,00)."));
            return;
        };
        match motor.executar(
            sessao,
            "financeiro.abrir_caixa.v1",
            &AbrirCaixa {
                caixa,
                valor_abertura,
            },
        ) {
            Ok(CaixaFoiAberto { .. }) => {
                estado.dlg = Dlg::Fechado;
                estado.foco_entrada = true;
                estado.carregar(motor, sessao);
                notificar(ctx, Notificacao::sucesso("Caixa aberto"));
            }
            Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
        }
    };

    let fechar = Dialogo::nova("Abrir caixa")
        .descricao("Informe o valor de troco que está na gaveta agora.")
        .pequeno()
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                let Dlg::AbrirCaixa { valor } = &mut estado.dlg else {
                    return;
                };
                let resp = ui.add(Campo::novo("Valor de abertura", valor).marcador("0,00"));
                if !resp.has_focus() {
                    resp.request_focus();
                }
                if resp.lost_focus() && enter_pressionado(ui) {
                    abrir_caixa(estado);
                }
            },
            |ui, estado| {
                if ui
                    .add(Botao::primario("Abrir caixa").atalho("Enter"))
                    .clicked()
                {
                    abrir_caixa(estado);
                }
                if ui
                    .add(Botao::secundario("Cancelar").atalho("Esc"))
                    .clicked()
                {
                    estado.dlg = Dlg::Fechado;
                }
            },
        );
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

pub(super) fn dialogo_desconto(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaPdv,
) {
    let Dlg::Desconto { linha, .. } = &estado.dlg else {
        return;
    };
    let linha = *linha;
    let Some(l) = estado.carrinho.get(linha) else {
        estado.dlg = Dlg::Fechado;
        return;
    };
    let descricao = format!(
        "{} · {} × {}",
        l.nome,
        l.quantidade.formatar(0),
        l.preco.formatar_com_simbolo()
    );

    let aplicar_desconto = |estado: &mut EstadoTelaPdv| {
        let (Dlg::Desconto { percentual, .. }, Some(cupom)) = (&estado.dlg, estado.cupom) else {
            return;
        };
        let Ok(pct) = percentual.trim().parse::<Percentual>() else {
            notificar(
                ctx,
                Notificacao::aviso("Desconto inválido (ex.: 10 ou 7,5)."),
            );
            return;
        };
        let Some(item) = estado.carrinho.get(linha).map(|l| l.item) else {
            return;
        };
        match motor.executar(
            sessao,
            "pdv.aplicar_desconto_item.v1",
            &AplicarDescontoItem {
                cupom,
                item,
                desconto_percentual: pct,
            },
        ) {
            Ok(()) => {
                if let Some(l) = estado.carrinho.get_mut(linha) {
                    l.com_desconto(pct);
                }
                estado.recalcular_total();
                estado.dlg = Dlg::Fechado;
                estado.foco_entrada = true;
                notificar(ctx, Notificacao::sucesso("Desconto aplicado"));
            }
            // Ex.: acima do teto do papel — o diálogo continua aberto para corrigir.
            Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
        }
    };

    let fechar = Dialogo::nova("Desconto no item")
        .descricao(descricao)
        .pequeno()
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                let Dlg::Desconto { percentual, .. } = &mut estado.dlg else {
                    return;
                };
                let resp = ui.add(Campo::novo("Desconto (%)", percentual).marcador("10"));
                if !resp.has_focus() {
                    resp.request_focus();
                }
                if resp.lost_focus() && enter_pressionado(ui) {
                    aplicar_desconto(estado);
                }
            },
            |ui, estado| {
                if ui
                    .add(Botao::primario("Aplicar desconto").atalho("Enter"))
                    .clicked()
                {
                    aplicar_desconto(estado);
                }
                if ui
                    .add(Botao::secundario("Cancelar").atalho("Esc"))
                    .clicked()
                {
                    estado.dlg = Dlg::Fechado;
                }
            },
        );
    if fechar {
        estado.dlg = Dlg::Fechado;
        estado.foco_entrada = true;
    }
}

pub(super) fn dialogo_cancelar_cupom(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaPdv,
) {
    let titulo = format!(
        "Cancelar cupom nº {} — {}",
        estado.cupom_numero.unwrap_or(0),
        estado.total.formatar_com_simbolo()
    );
    let confirmar = |estado: &mut EstadoTelaPdv| {
        let Dlg::CancelarCupom { motivo } = &estado.dlg else {
            return;
        };
        let motivo = motivo.trim().to_owned();
        if motivo.is_empty() {
            notificar(ctx, Notificacao::aviso("Informe o motivo do cancelamento."));
            return;
        }
        if cancelar_cupom(ctx, motor, sessao, estado, motivo) {
            estado.dlg = Dlg::Fechado;
        }
    };

    let fechar = Dialogo::nova(titulo)
        .descricao(
            "A venda em andamento é encerrada e o cancelamento fica registrado com o seu usuário.",
        )
        .pequeno()
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                let Dlg::CancelarCupom { motivo } = &mut estado.dlg else {
                    return;
                };
                let resp =
                    ui.add(Campo::novo("Motivo", motivo).marcador("Cliente desistiu da compra"));
                if !resp.has_focus() {
                    resp.request_focus();
                }
                if resp.lost_focus() && enter_pressionado(ui) {
                    confirmar(estado);
                }
            },
            |ui, estado| {
                if ui
                    .add(Botao::destrutivo("Cancelar cupom").atalho("Enter"))
                    .clicked()
                {
                    confirmar(estado);
                }
                if ui.add(Botao::secundario("Voltar").atalho("Esc")).clicked() {
                    estado.dlg = Dlg::Fechado;
                }
            },
        );
    if fechar {
        estado.dlg = Dlg::Fechado;
        estado.foco_entrada = true;
    }
}

/// `F12` com o caixa aberto. A contagem é **cega**: o esperado só aparece depois de fechar
/// (`docs/modulos/financeiro.md`), então o operador não ajusta o que conta ao que o sistema
/// espera. O motivo só é exigido pelo backend se a quebra passar da tolerância — que o
/// operador não tem como saber antes —, então o campo fica sempre à vista.
pub(super) fn dialogo_fechar_caixa(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaPdv,
) {
    let Some(sessao_caixa) = estado.sessao_aberta() else {
        estado.dlg = Dlg::Fechado;
        return;
    };
    let confirmar = |estado: &mut EstadoTelaPdv| {
        let Dlg::FecharCaixa { contado, motivo } = &estado.dlg else {
            return;
        };
        let Ok(valor_contado) = contado.trim().parse::<Dinheiro>() else {
            notificar(
                ctx,
                Notificacao::aviso("Informe o valor contado (ex.: 350,00)."),
            );
            return;
        };
        let motivo = Some(motivo.trim().to_owned()).filter(|m| !m.is_empty());
        match motor.executar(
            sessao,
            "financeiro.fechar_caixa.v1",
            &FecharCaixa {
                sessao: sessao_caixa,
                valor_contado,
                motivo,
            },
        ) {
            Ok(CaixaFoiFechado {
                valor_esperado,
                quebra,
                ..
            }) => {
                estado.dlg = Dlg::Fechado;
                estado.carregar(motor, sessao);
                let detalhe = format!(
                    "Esperado {} · contado {}",
                    valor_esperado.formatar_com_simbolo(),
                    valor_contado.formatar_com_simbolo()
                );
                let aviso = if quebra == Dinheiro::ZERO {
                    Notificacao::sucesso("Caixa fechado — sem diferença")
                } else if quebra.e_negativo() {
                    Notificacao::aviso(format!(
                        "Caixa fechado — faltam {}",
                        quebra.abs().formatar_com_simbolo()
                    ))
                } else {
                    Notificacao::aviso(format!(
                        "Caixa fechado — sobram {}",
                        quebra.formatar_com_simbolo()
                    ))
                };
                notificar(ctx, aviso.detalhe(detalhe));
            }
            // Ex.: quebra acima da tolerância sem motivo — o diálogo continua aberto.
            Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
        }
    };

    let fechar = Dialogo::nova("Fechar caixa")
        .descricao("Conte o dinheiro da gaveta e informe o total. O esperado só aparece depois.")
        .pequeno()
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                let Dlg::FecharCaixa { contado, motivo } = &mut estado.dlg else {
                    return;
                };
                let resp = ui.add(Campo::novo("Valor contado", contado).marcador("0,00"));
                if !resp.has_focus() && motivo.is_empty() {
                    resp.request_focus();
                }
                ui.add_space(Espaco::E12);
                let dica = format!(
                    "Motivo da diferença (obrigatório acima de {})",
                    TOLERANCIA_QUEBRA.formatar_com_simbolo()
                );
                let resp_motivo = ui.add(Campo::novo(dica, motivo));
                if (resp.lost_focus() || resp_motivo.lost_focus()) && enter_pressionado(ui) {
                    confirmar(estado);
                }
            },
            |ui, estado| {
                if ui
                    .add(Botao::primario("Fechar caixa").atalho("Enter"))
                    .clicked()
                {
                    confirmar(estado);
                }
                if ui.add(Botao::secundario("Voltar").atalho("Esc")).clicked() {
                    estado.dlg = Dlg::Fechado;
                }
            },
        );
    if fechar {
        estado.dlg = Dlg::Fechado;
        estado.foco_entrada = true;
    }
}

/// `F9`: retirada de dinheiro da gaveta (`financeiro.registrar_sangria.v1`).
pub(super) fn dialogo_sangria(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaPdv,
) {
    let Some(sessao_caixa) = estado.sessao_aberta() else {
        estado.dlg = Dlg::Fechado;
        return;
    };
    let confirmar = |estado: &mut EstadoTelaPdv| {
        let Dlg::Sangria { valor, motivo } = &estado.dlg else {
            return;
        };
        let valor = match valor.trim().parse::<Dinheiro>() {
            Ok(v) if v > Dinheiro::ZERO => v,
            _ => {
                notificar(
                    ctx,
                    Notificacao::aviso("Informe o valor retirado (ex.: 200,00)."),
                );
                return;
            }
        };
        let motivo = motivo.trim().to_owned();
        if motivo.is_empty() {
            notificar(ctx, Notificacao::aviso("Informe o motivo da sangria."));
            return;
        }
        match motor.executar(
            sessao,
            "financeiro.registrar_sangria.v1",
            &RegistrarSangria {
                sessao: sessao_caixa,
                valor,
                motivo,
            },
        ) {
            Ok(_) => {
                estado.dlg = Dlg::Fechado;
                estado.foco_entrada = true;
                notificar(
                    ctx,
                    Notificacao::sucesso(format!(
                        "Sangria de {} registrada",
                        valor.formatar_com_simbolo()
                    )),
                );
            }
            Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
        }
    };

    let fechar = Dialogo::nova("Sangria")
        .descricao("Retirada de dinheiro da gaveta. Fica registrada com o motivo.")
        .pequeno()
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                let Dlg::Sangria { valor, motivo } = &mut estado.dlg else {
                    return;
                };
                let resp = ui.add(Campo::novo("Valor", valor).marcador("0,00"));
                if !resp.has_focus() && motivo.is_empty() {
                    resp.request_focus();
                }
                ui.add_space(Espaco::E12);
                let resp_motivo =
                    ui.add(Campo::novo("Motivo", motivo).marcador("Depósito no cofre"));
                if (resp.lost_focus() || resp_motivo.lost_focus()) && enter_pressionado(ui) {
                    confirmar(estado);
                }
            },
            |ui, estado| {
                if ui
                    .add(Botao::primario("Registrar sangria").atalho("Enter"))
                    .clicked()
                {
                    confirmar(estado);
                }
                if ui
                    .add(Botao::secundario("Cancelar").atalho("Esc"))
                    .clicked()
                {
                    estado.dlg = Dlg::Fechado;
                }
            },
        );
    if fechar {
        estado.dlg = Dlg::Fechado;
        estado.foco_entrada = true;
    }
}
