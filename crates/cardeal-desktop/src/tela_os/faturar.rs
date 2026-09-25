//! Faturar a OS: à vista (já recebido) ou a prazo, pelo `EstadoPagamento` compartilhado.

use super::*;

/// Faturar uma OS — disponível desde a abertura (`Dlg::faturar`, botão no rodapé de
/// `dialogo_detalhe`). Quando há cobrança, pergunta a condição pelo
/// [`crate::pagamento::EstadoPagamento`]: à vista (meio + conta, já baixado) ou a prazo
/// (parcelas em aberto no financeiro).
pub(super) fn dialogo_faturar(
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
    let enter =
        ctx.input(|i| i.key_pressed(egui::Key::Enter)) && !ctx.memory(|m| m.any_popup_open());
    let titulo = format!("Faturar OS #{} · {}", os.numero, equipamento_label(&os));

    let fechar = Dialogo::nova(titulo).largura(520.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            ui.horizontal(|ui| {
                ui.add(Rotulo::campo("Total a faturar"));
                ui.add(ValorDinheiro::novo(os.valor_total));
            });
            ui.add_space(Espaco::E12);
            if !os.valor_total.e_positivo() {
                ui.add(
                    Rotulo::interface(
                        "Serviço sem cobrança (garantia/cortesia) — faturar só fecha o \
                         ciclo, sem gerar título.",
                    )
                    .quebravel(),
                );
                return;
            }
            if let Dlg::Faturar(pagamento) = &mut estado.dlg {
                pagamento.mostrar(ui, motor, sessao, "faturar-os", true);
            }
        },
        |ui, estado| {
            if ui.add(Botao::primario("Faturar")).clicked() || enter {
                faturar_os(ui.ctx(), motor, sessao, estado, &os);
            }
            if ui.add(Botao::secundario("Cancelar")).clicked() {
                estado.dlg = Dlg::Detalhe;
            }
        },
    );
    if fechar {
        estado.dlg = Dlg::Detalhe;
    }
}

/// Executa `FaturarOrdemServico` com a condição escolhida em `Dlg::Faturar`.
pub(super) fn faturar_os(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
    os: &OrdemServico,
) {
    let hoje = Data::hoje(Fuso::BRASILIA);
    let (parcelas, primeiro_vencimento, intervalo_dias, pago_no_ato) =
        match (&estado.dlg, os.valor_total.e_positivo()) {
            (_, false) => (1, hoje, 0, None),
            (Dlg::Faturar(p), true) if p.condicao.a_prazo => match p.prazo_validado() {
                Ok((n, venc, intervalo)) => (n, venc, intervalo, None),
                Err(msg) => {
                    notificar(ctx, Notificacao::aviso(msg));
                    return;
                }
            },
            (Dlg::Faturar(p), true) => match p.meio_validado() {
                Ok(meio_pagamento) => (
                    1,
                    hoje,
                    0,
                    Some(PagamentoNoAto {
                        meio_pagamento,
                        conta_destino: p.conta_destino(),
                    }),
                ),
                Err(msg) => {
                    notificar(ctx, Notificacao::aviso(msg));
                    return;
                }
            },
            _ => return,
        };

    match motor.executar(
        sessao,
        "os.faturar_ordem_servico.v1",
        &FaturarOrdemServico {
            ordem_servico: os.id,
            parcelas,
            primeiro_vencimento,
            intervalo_dias,
            pago_no_ato,
        },
    ) {
        Ok(f) => {
            let f: OrdemServicoFaturada = f;
            estado.recarregar_lista(motor, sessao);
            estado.abrir_detalhe(motor, sessao, os.id);
            estado.dlg = Dlg::Detalhe;
            let msg = if f.titulo_pago {
                "OS faturada e recebida".to_owned()
            } else if f.titulo.is_some() {
                format!("OS faturada — {parcelas} parcela(s) a receber no Financeiro")
            } else {
                "OS faturada (sem cobrança)".to_owned()
            };
            notificar(ctx, Notificacao::sucesso(msg));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}
