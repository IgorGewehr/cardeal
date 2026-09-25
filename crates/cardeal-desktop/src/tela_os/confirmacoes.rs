//! As confirmações da tela de OS: nenhuma transição de estado (cancelar, aprovar, reprovar,
//! concluir, desfaturar, reabrir) dispara no primeiro clique.

use super::*;

/// Desenha a confirmação pendente (se houver) e executa a ação quando confirmada.
pub(super) fn confirmacoes(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
) {
    if let Some((ordem_servico, rotulo)) = estado.confirmar_exclusao.clone() {
        let resposta = cardeal_ui::organisms::dialogo_confirmacao(
            ctx,
            "Excluir OS",
            &format!(
                "Tem certeza que quer excluir a OS \"{rotulo}\"? Ela vai pra Cancelada — \
                 peças já aplicadas voltam pro estoque automaticamente, quando possível.",
            ),
            "Excluir",
        );
        if resposta.confirmado {
            match motor.executar(
                sessao,
                "os.cancelar_ordem_servico.v1",
                &CancelarOrdemServico { ordem_servico },
            ) {
                Ok(r) => {
                    let r: OrdemServicoCancelada = r;
                    // Peças aplicadas voltam ao estoque no cancelamento.
                    estado.recarregar_lista(motor, sessao);
                    estado.carregar_produtos(motor, sessao);
                    let msg = if r.pecas_pendentes_de_estorno_manual > 0 {
                        format!(
                            "OS excluída — {} peça(s) precisam de correção manual de estoque",
                            r.pecas_pendentes_de_estorno_manual
                        )
                    } else {
                        "OS excluída".to_owned()
                    };
                    notificar(ctx, Notificacao::sucesso(msg));
                }
                Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
            }
        }
        if resposta.fechar {
            estado.confirmar_exclusao = None;
        }
    }

    if let Some(acao) = estado.confirmar_acao {
        let (titulo, mensagem, rotulo_acao) = match acao {
            AcaoPendente::AprovarOrcamento(_) => (
                "Aprovar orçamento",
                "Confirma a aprovação deste orçamento pelo cliente? A OS libera para \
                 execução."
                    .to_owned(),
                "Aprovar",
            ),
            AcaoPendente::ReprovarOrcamento(_) => (
                "Reprovar orçamento",
                "Confirma a reprovação? A OS encerra sem execução — esta ação não tem volta."
                    .to_owned(),
                "Reprovar",
            ),
            AcaoPendente::ConcluirExecucao(_) => (
                "Concluir execução",
                "Confirma que o reparo terminou? A OS fica pronta para faturar.".to_owned(),
                "Concluir",
            ),
            AcaoPendente::Desfaturar(_) => (
                "Desfaturar OS",
                "Confirma desfaturar esta OS? O título gerado no financeiro é cancelado \
                 (qualquer recebimento já dado é estornado) e a OS volta a aceitar edição de \
                 valores e serviços."
                    .to_owned(),
                "Desfaturar",
            ),
            AcaoPendente::Reabrir(_) => (
                "Reabrir OS",
                "Confirma reabrir esta OS cancelada? Ela volta para \"Aberta\" e aceita edição \
                 normalmente."
                    .to_owned(),
                "Reabrir",
            ),
        };
        let resposta =
            cardeal_ui::organisms::dialogo_confirmacao(ctx, titulo, &mensagem, rotulo_acao);
        if resposta.confirmado {
            match acao {
                AcaoPendente::AprovarOrcamento(ordem_servico) => aplicar_e_recarregar(
                    ctx,
                    motor,
                    sessao,
                    estado,
                    ordem_servico,
                    "os.aprovar_orcamento.v1",
                    &AprovarOrcamentoOs {
                        ordem_servico,
                        identificacao_aprovador: estado.aprovador.clone(),
                    },
                    "Orçamento aprovado",
                ),
                AcaoPendente::ReprovarOrcamento(ordem_servico) => aplicar_e_recarregar(
                    ctx,
                    motor,
                    sessao,
                    estado,
                    ordem_servico,
                    "os.reprovar_orcamento.v1",
                    &ReprovarOrcamentoOs { ordem_servico },
                    "Orçamento reprovado",
                ),
                AcaoPendente::ConcluirExecucao(ordem_servico) => aplicar_e_recarregar(
                    ctx,
                    motor,
                    sessao,
                    estado,
                    ordem_servico,
                    "os.concluir_execucao.v1",
                    &ConcluirExecucao { ordem_servico },
                    "Execução concluída",
                ),
                AcaoPendente::Desfaturar(ordem_servico) => aplicar_e_recarregar(
                    ctx,
                    motor,
                    sessao,
                    estado,
                    ordem_servico,
                    "os.desfaturar_ordem_servico.v1",
                    &DesfaturarOrdemServico { ordem_servico },
                    "OS desfaturada — título removido do financeiro",
                ),
                AcaoPendente::Reabrir(ordem_servico) => aplicar_e_recarregar(
                    ctx,
                    motor,
                    sessao,
                    estado,
                    ordem_servico,
                    "os.reabrir_ordem_servico.v1",
                    &ReabrirOrdemServico { ordem_servico },
                    "OS reaberta",
                ),
            }
        }
        if resposta.fechar {
            estado.confirmar_acao = None;
        }
    }
}
