//! Apontamento de tempo do técnico na OS (cronômetro).

use super::*;

/// Formata segundos como `Hh MMmin` — o suficiente para a UI mostrar tempo acumulado sem
/// exigir uma dependência de formatação de duração no kernel só para isto.
pub(super) fn formatar_duracao(segundos: i64) -> String {
    let segundos = segundos.max(0);
    let horas = segundos / 3600;
    let minutos = (segundos % 3600) / 60;
    if horas > 0 {
        format!("{horas}h {minutos:02}min")
    } else {
        format!("{minutos}min")
    }
}

/// A seção "Apontamento de tempo" do detalhe da OS: tempo total acumulado (encerrado) +
/// cronômetro do técnico logado, se houver um apontamento aberto dele nesta ordem — botão
/// simples "Iniciar"/"Encerrar apontamento", sem exigir uma tela própria
/// (`docs/modulos/os.md`: apontamento de tempo real, diferente da mão de obra orçada).
pub(super) fn secao_apontamento(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaOs,
    ordem_servico: Id,
) {
    ui.add(Rotulo::titulo_secao("Apontamento de tempo"));
    ui.add_space(Espaco::E8);

    let aberto_do_usuario = estado
        .apontamentos
        .iter()
        .find(|a| a.tecnico == sessao.usuario() && a.esta_aberto())
        .cloned();

    ui.horizontal(|ui| {
        ui.add(Rotulo::campo("Tempo total registrado"));
        let total = match &aberto_do_usuario {
            Some(ap) => estado.tempo_total_ordem + ap.duracao_segundos(Instante::agora()),
            None => estado.tempo_total_ordem,
        };
        ui.add(Rotulo::interface(formatar_duracao(total)));
    });
    ui.add_space(Espaco::E8);

    match aberto_do_usuario {
        Some(ap) => {
            // O total mostrado inclui o cronômetro rodando; sem pedir quadro novo, o egui só
            // redesenha quando o mouse mexe e o tempo parece congelado.
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs(1));
            ui.add(Rotulo::interface(format!(
                "Cronômetro rodando desde {}",
                ap.inicio.formatar(cardeal_kernel::Fuso::BRASILIA)
            )));
            if ui.add(Botao::secundario("Encerrar apontamento")).clicked() {
                let r = motor.executar(
                    sessao,
                    "os.encerrar_apontamento.v1",
                    &EncerrarApontamento { apontamento: ap.id },
                );
                match r {
                    Ok(_duracao) => {
                        estado.carregar_apontamentos(motor, sessao, ordem_servico);
                        notificar(ui.ctx(), Notificacao::sucesso("Apontamento encerrado"));
                    }
                    Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
                }
            }
        }
        None => {
            if ui.add(Botao::primario("Iniciar apontamento")).clicked() {
                let r = motor.executar(
                    sessao,
                    "os.iniciar_apontamento.v1",
                    &IniciarApontamento {
                        ordem_servico,
                        tecnico: sessao.usuario(),
                    },
                );
                match r {
                    Ok(_) => {
                        estado.carregar_apontamentos(motor, sessao, ordem_servico);
                        notificar(ui.ctx(), Notificacao::sucesso("Apontamento iniciado"));
                    }
                    Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
                }
            }
        }
    }
}
