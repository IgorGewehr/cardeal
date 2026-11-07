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
    let aberto_do_usuario = estado
        .apontamentos
        .iter()
        .find(|a| a.tecnico == sessao.usuario() && a.esta_aberto())
        .cloned();
    let total = match &aberto_do_usuario {
        Some(ap) => estado.tempo_total_ordem + ap.duracao_segundos(Instante::agora()),
        None => estado.tempo_total_ordem,
    };

    let mut clicou = false;
    Painel::novo()
        .plano()
        .compacto()
        .titulo("Tempo de reparo")
        .mostrar_com_acoes(
            ui,
            |ui| {
                if aberto_do_usuario.is_some() {
                    ui.add(Etiqueta::positiva("rodando").com_ponto());
                }
            },
            |ui| {
                Dado::novo("Total registrado", formatar_duracao(total)).mostrar(ui);
                match &aberto_do_usuario {
                    Some(ap) => {
                        // O total inclui o cronômetro rodando; sem pedir quadro novo, o egui
                        // só redesenha quando o mouse mexe e o tempo parece congelado.
                        ui.ctx()
                            .request_repaint_after(std::time::Duration::from_secs(1));
                        ui.add(Rotulo::apoio(format!(
                            "Seu cronômetro desde {}",
                            ap.inicio.formatar(cardeal_kernel::Fuso::BRASILIA)
                        )));
                        ui.add_space(Espaco::E8);
                        clicou = ui
                            .add(Botao::secundario("Parar cronômetro").preenche_largura())
                            .clicked();
                    }
                    None => {
                        clicou = ui
                            .add(Botao::secundario("Iniciar cronômetro").preenche_largura())
                            .clicked();
                    }
                }
            },
        );
    ui.add_space(Espaco::E12);
    if !clicou {
        return;
    }
    let r = match &aberto_do_usuario {
        Some(ap) => motor
            .executar(
                sessao,
                "os.encerrar_apontamento.v1",
                &EncerrarApontamento { apontamento: ap.id },
            )
            .map(|_| "Apontamento encerrado"),
        None => motor
            .executar(
                sessao,
                "os.iniciar_apontamento.v1",
                &IniciarApontamento {
                    ordem_servico,
                    tecnico: sessao.usuario(),
                },
            )
            .map(|_| "Apontamento iniciado"),
    };
    match r {
        Ok(msg) => {
            estado.carregar_apontamentos(motor, sessao, ordem_servico);
            notificar(ui.ctx(), Notificacao::sucesso(msg));
        }
        Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
    }
}
