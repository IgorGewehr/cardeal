//! Corrigir uma conta no diálogo da parcela: **alterar o vencimento** (qualquer origem) e
//! **cancelar o título** lançado errado (só o lançado no financeiro, sem baixa) —
//! `financeiro.alterar_vencimento.v1` e `financeiro.cancelar_titulo.v1`.

use super::*;

/// As origens que o financeiro cancela (as outras se desfazem pela origem).
const ORIGENS_DO_FINANCEIRO: &[&str] = &["avulso", "financeiro_recorrencia"];

/// Os campos, como o usuário digita.
#[derive(Debug, Clone, Default)]
pub(super) struct FormAjuste {
    pub(super) vencimento: String,
    pub(super) motivo: String,
    /// `true` = a confirmação do novo vencimento está aberta.
    pub(super) confirmar_vencimento: bool,
    /// `true` = a confirmação do cancelamento está aberta.
    pub(super) confirmar_cancelamento: bool,
}

/// De onde veio a conta, para dizer onde desfazê-la.
fn origem_por_extenso(origem: &str) -> &'static str {
    match origem {
        "compras" => "de uma nota de compra (desfaça pela entrada da nota)",
        "vendas" => "de uma venda (desfaça pela devolução da venda)",
        "pdv" => "do PDV (desfaça pelo estorno da venda, F11)",
        "os" | "os_peca" => "de uma ordem de serviço (desfaça pelo desfaturamento da OS)",
        "financeiro" => "de uma renegociação",
        _ => "de outro módulo",
    }
}

/// As seções recolhidas "Alterar vencimento" e "Cancelar esta conta".
pub(super) fn secoes_ajuste(
    ui: &mut egui::Ui,
    sessao: &Sessao,
    form: &mut FormAjuste,
    p: &ItemTituloEmAberto,
) {
    if p.estado.aceita_baixa() && sessao.concede("financeiro.titulo.editar") {
        ui.add_space(Espaco::E16);
        SecaoExpansivel::nova("Alterar vencimento").mostrar(ui, |ui| {
            if form.vencimento.is_empty() {
                form.vencimento = p.vencimento.to_string();
            }
            ui.columns(2, |c| {
                c[0].add(
                    Campo::novo("Novo vencimento", &mut form.vencimento).mascara(Mascara::Data),
                );
            });
            ui.add_space(Espaco::E8);
            if ui.add(Botao::secundario("Alterar vencimento")).clicked() {
                form.confirmar_vencimento = true;
            }
        });
    }
    if !sessao.concede("financeiro.titulo.cancelar")
        || matches!(
            p.estado,
            EstadoParcela::Cancelada | EstadoParcela::Renegociada
        )
    {
        return;
    }
    ui.add_space(Espaco::E16);
    SecaoExpansivel::nova("Cancelar esta conta").mostrar(ui, |ui| {
        if !ORIGENS_DO_FINANCEIRO.contains(&p.origem_modulo.as_str()) {
            ui.add(
                Rotulo::interface(format!(
                    "Esta conta veio {}: cancelá-la só no financeiro deixaria o estoque e a \
                     contabilidade desencontrados.",
                    origem_por_extenso(&p.origem_modulo)
                ))
                .quebravel()
                .cor(ui.cores().texto_medio),
            );
            return;
        }
        if p.estado != EstadoParcela::Aberta || p.valor_baixado.e_positivo() {
            ui.add(
                Rotulo::interface(
                    "Esta conta já tem baixa. Estorne as baixas no histórico abaixo antes de \
                     cancelar — um recebimento ou pagamento de verdade não se desfaz sozinho.",
                )
                .quebravel()
                .cor(ui.cores().texto_medio),
            );
            return;
        }
        ui.add(
            Rotulo::interface(
                "Cancela a conta inteira (todas as parcelas): o lançamento é estornado e nada é \
                 apagado — fica no histórico como cancelada.",
            )
            .quebravel(),
        );
        ui.add_space(Espaco::E8);
        ui.add(Campo::novo("Motivo", &mut form.motivo).marcador("Lançada em duplicidade"));
        ui.add_space(Espaco::E8);
        if ui.add(Botao::destrutivo("Cancelar conta")).clicked() {
            form.confirmar_cancelamento = true;
        }
    });
}

/// As confirmações e a execução — chamada depois do diálogo da parcela, para desenhar por cima.
pub(super) fn confirmar_ajustes(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
    p: &ItemTituloEmAberto,
) {
    let Dlg::Baixar { ajuste, .. } = &mut estado.dlg else {
        return;
    };
    if ajuste.confirmar_vencimento {
        let Ok(vencimento) = ajuste.vencimento.parse::<Data>() else {
            notificar(
                ctx,
                Notificacao::aviso("Informe o vencimento (dd/mm/aaaa)."),
            );
            ajuste.confirmar_vencimento = false;
            return;
        };
        let resposta = cardeal_ui::organisms::dialogo_confirmacao(
            ctx,
            "Alterar vencimento",
            &format!(
                "A parcela {} passa a vencer em {} (era {}).",
                p.numero,
                vencimento.formatar(),
                p.vencimento.formatar()
            ),
            "Alterar",
        );
        if resposta.confirmado {
            let r: Result<(), _> = motor.executar(
                sessao,
                "financeiro.alterar_vencimento.v1",
                &mod_financeiro::AlterarVencimento {
                    parcela: p.parcela,
                    vencimento,
                },
            );
            match r {
                Ok(()) => {
                    estado.dlg = Dlg::Fechado;
                    estado.carregar(motor, sessao);
                    notificar(
                        ctx,
                        Notificacao::sucesso(format!(
                            "Vencimento alterado para {}",
                            vencimento.formatar()
                        )),
                    );
                    return;
                }
                Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
            }
        }
        if resposta.fechar || resposta.confirmado {
            if let Dlg::Baixar { ajuste, .. } = &mut estado.dlg {
                ajuste.confirmar_vencimento = false;
            }
        }
        return;
    }
    if !ajuste.confirmar_cancelamento {
        return;
    }
    let motivo = ajuste.motivo.trim().to_owned();
    if motivo.chars().count() < 10 {
        notificar(
            ctx,
            Notificacao::aviso("Explique o motivo do cancelamento (ao menos 10 caracteres)."),
        );
        ajuste.confirmar_cancelamento = false;
        return;
    }
    let resposta = cardeal_ui::organisms::dialogo_confirmacao(
        ctx,
        "Cancelar conta",
        "A conta inteira (todas as parcelas) é cancelada e o lançamento estornado. Não dá para \
         desfazer: se precisar, lance de novo.",
        "Cancelar conta",
    );
    if resposta.confirmado {
        let r: Result<mod_financeiro::TituloFoiCancelado, _> = motor.executar(
            sessao,
            "financeiro.cancelar_titulo.v1",
            &mod_financeiro::CancelarTitulo {
                titulo: p.titulo,
                motivo,
            },
        );
        match r {
            Ok(c) => {
                estado.dlg = Dlg::Fechado;
                estado.carregar(motor, sessao);
                notificar(
                    ctx,
                    Notificacao::sucesso(format!(
                        "Conta de {} cancelada ({} parcela(s))",
                        c.valor.formatar_com_simbolo(),
                        c.parcelas
                    )),
                );
                return;
            }
            Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
        }
    }
    if resposta.fechar || resposta.confirmado {
        if let Dlg::Baixar { ajuste, .. } = &mut estado.dlg {
            ajuste.confirmar_cancelamento = false;
        }
    }
}
