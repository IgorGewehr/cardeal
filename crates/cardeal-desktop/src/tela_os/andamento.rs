//! O andamento da OS: a linha do tempo (o que já aconteceu, com quem e quando, e o que falta)
//! e o **próximo passo** — a única ação principal da gaveta, no rodapé. Antes cada estado
//! tinha seu botão primário perdido no meio do corpo, junto de "Faturar" e "Comprovante"
//! também primários: o técnico precisava procurar o que fazer.

use super::*;
use cardeal_ui::molecules::{LinhaDoTempo, PassoLinhaDoTempo, SituacaoPasso};

/// O que empurra a OS para o estado seguinte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ProximoPasso {
    EnviarParaAprovacao,
    Aprovar,
    IniciarExecucao,
    ConcluirExecucao,
    Faturar,
}

impl ProximoPasso {
    pub(super) fn rotulo(&self) -> String {
        match self {
            Self::EnviarParaAprovacao => "Enviar orçamento".to_owned(),
            Self::Aprovar => "Aprovar orçamento".to_owned(),
            Self::IniciarExecucao => "Iniciar execução".to_owned(),
            Self::ConcluirExecucao => "Concluir execução".to_owned(),
            Self::Faturar => "Faturar".to_owned(),
        }
    }
}

/// O passo natural a partir do estado atual, se houver um. Aberta sem orçamento não tem:
/// falta o técnico diagnosticar e orçar (ou faturar direto, que continua no rodapé).
pub(super) fn proximo_passo(detalhe: &DetalheOrdem) -> Option<ProximoPasso> {
    let os = &detalhe.ordem;
    match os.estado {
        EstadoOs::Aberta | EstadoOs::EmDiagnostico => {
            (!os.valor_total.e_zero()).then_some(ProximoPasso::EnviarParaAprovacao)
        }
        EstadoOs::AguardandoAprovacao => Some(ProximoPasso::Aprovar),
        EstadoOs::Aprovada => Some(ProximoPasso::IniciarExecucao),
        // As peças que faltam saem do estoque na própria conclusão.
        EstadoOs::EmExecucao => Some(ProximoPasso::ConcluirExecucao),
        EstadoOs::Concluida => Some(ProximoPasso::Faturar),
        EstadoOs::Faturada | EstadoOs::Cancelada | EstadoOs::Reprovada => None,
    }
}

/// Executa o passo (ou abre a confirmação/diálogo que ele exige).
pub(super) fn executar_proximo(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
    os: Id,
    passo: ProximoPasso,
) {
    match passo {
        ProximoPasso::EnviarParaAprovacao => aplicar_e_recarregar(
            ctx,
            motor,
            sessao,
            estado,
            os,
            "os.enviar_para_aprovacao.v1",
            &EnviarParaAprovacao { ordem_servico: os },
            "Orçamento enviado para aprovação",
        ),
        ProximoPasso::Aprovar => {
            if estado.aprovador.trim().is_empty() {
                notificar(
                    ctx,
                    Notificacao::aviso("Informe quem aprovou (nome e documento)."),
                );
            } else {
                estado.confirmar_acao = Some(AcaoPendente::AprovarOrcamento(os));
            }
        }
        ProximoPasso::IniciarExecucao => aplicar_e_recarregar(
            ctx,
            motor,
            sessao,
            estado,
            os,
            "os.iniciar_execucao.v1",
            &IniciarExecucao { ordem_servico: os },
            "Execução iniciada",
        ),
        ProximoPasso::ConcluirExecucao => {
            estado.confirmar_acao = Some(AcaoPendente::ConcluirExecucao(os));
        }
        ProximoPasso::Faturar => estado.dlg = Dlg::faturar(),
    }
}

/// Nome curto de um estado na linha do tempo.
const fn titulo_do_estado(e: EstadoOs) -> &'static str {
    match e {
        EstadoOs::Aberta => "Aberta",
        EstadoOs::EmDiagnostico => "Em diagnóstico",
        EstadoOs::AguardandoAprovacao => "Orçamento enviado",
        EstadoOs::Aprovada => "Orçamento aprovado",
        EstadoOs::Reprovada => "Orçamento reprovado",
        EstadoOs::EmExecucao => "Em execução",
        EstadoOs::Concluida => "Pronta para retirada",
        EstadoOs::Faturada => "Faturada",
        EstadoOs::Cancelada => "Cancelada",
    }
}

/// O caminho comum de uma OS — o que a linha do tempo mostra como "ainda falta".
const CAMINHO: [EstadoOs; 6] = [
    EstadoOs::Aberta,
    EstadoOs::AguardandoAprovacao,
    EstadoOs::Aprovada,
    EstadoOs::EmExecucao,
    EstadoOs::Concluida,
    EstadoOs::Faturada,
];

/// Monta os passos: o histórico gravado (feito; o último é o atual) e, se a OS ainda está
/// ativa, o resto do caminho comum como pendente. OS de antes do histórico começam pela data
/// de abertura.
pub(super) fn passos_da_linha_do_tempo(
    detalhe: &DetalheOrdem,
    usuarios: &[UsuarioResumo],
) -> Vec<PassoLinhaDoTempo> {
    let os = &detalhe.ordem;
    let nome = |id: Id| {
        usuarios
            .iter()
            .find(|u| u.id == id)
            .map(|u| u.nome.split_whitespace().next().unwrap_or("").to_owned())
            .unwrap_or_default()
    };
    let mut passos: Vec<(EstadoOs, String)> = detalhe
        .historico
        .iter()
        .map(|p| {
            let quem = nome(p.usuario);
            let quando = p.momento.formatar(Fuso::BRASILIA);
            let detalhe = if quem.is_empty() {
                quando
            } else {
                format!("{quando} · {quem}")
            };
            (p.estado, detalhe)
        })
        .collect();
    if passos.first().is_none_or(|(e, _)| *e != EstadoOs::Aberta) {
        passos.insert(0, (EstadoOs::Aberta, os.data_abertura.formatar()));
    }
    // O estado atual sempre aparece por último, mesmo que o histórico não o tenha (OS antiga).
    if passos.last().is_none_or(|(e, _)| *e != os.estado) {
        passos.push((os.estado, String::new()));
    }
    let ultimo = passos.len() - 1;
    let mut saida: Vec<PassoLinhaDoTempo> = passos
        .into_iter()
        .enumerate()
        .map(|(i, (e, detalhe))| PassoLinhaDoTempo {
            titulo: titulo_do_estado(e).to_owned(),
            detalhe,
            situacao: if i == ultimo && pode_faturar(os.estado) {
                SituacaoPasso::Atual
            } else {
                SituacaoPasso::Feito
            },
        })
        .collect();
    if pode_faturar(os.estado) {
        let desde = CAMINHO
            .iter()
            .position(|e| *e == os.estado)
            .map_or(1, |i| i + 1);
        for e in &CAMINHO[desde.min(CAMINHO.len())..] {
            saida.push(PassoLinhaDoTempo {
                titulo: titulo_do_estado(*e).to_owned(),
                detalhe: String::new(),
                situacao: SituacaoPasso::Pendente,
            });
        }
    }
    saida
}

/// A seção "Andamento" do corpo da gaveta.
pub(super) fn secao_andamento(ui: &mut egui::Ui, estado: &EstadoTelaOs, detalhe: &DetalheOrdem) {
    let passos = passos_da_linha_do_tempo(detalhe, &estado.usuarios);
    SecaoExpansivel::nova("Andamento")
        .aberta_por_padrao(true)
        .mostrar(ui, |ui| {
            ui.add(LinhaDoTempo::nova(&passos));
        });
    ui.add_space(Espaco::E12);
}
