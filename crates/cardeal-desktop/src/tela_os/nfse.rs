//! Ordens de serviço › OS faturada › **NFS-e**: emitir a nota da mão de obra, a situação dela
//! na fila e, rejeitada, o que corrigir (e emitir de novo, no mesmo número); autorizada, o número,
//! a chave e o cancelamento (motivo e justificativa), que vai pela fila.

use cardeal_cliente::{Motor, Sessao};
use cardeal_fiscal::MotivoCancelamentoNfse;
use cardeal_kernel::Id;
use cardeal_ui::atoms::{Botao, Etiqueta, Rotulo};
use cardeal_ui::molecules::{dado, Campo, SeletorOpcao};
use cardeal_ui::organisms::{notificar, Notificacao};
use cardeal_ui::tokens::Espaco;
use eframe::egui;
use mod_fiscal::nfse::{
    CancelarNfse, EmitirNfseDaOs, EstadoNfse, NfseDaOs, NfseResumo, JUSTIFICATIVA_MINIMA,
};

use crate::fiscal_fundo;

const MOTIVOS: [MotivoCancelamentoNfse; 4] = [
    MotivoCancelamentoNfse::ErroNaEmissao,
    MotivoCancelamentoNfse::ServicoNaoPrestado,
    MotivoCancelamentoNfse::Duplicidade,
    MotivoCancelamentoNfse::ErroDeProcessamento,
];

/// A NFS-e da OS aberta (relida quando a fila roda) e o formulário do cancelamento.
#[derive(Default)]
pub(super) struct NfseDaOsAberta {
    ordem: Option<Id>,
    geracao: u64,
    nfse: Option<NfseResumo>,
    cancelando: Option<(Option<usize>, String)>,
}

impl NfseDaOsAberta {
    fn atualizar(&mut self, motor: &Motor, sessao: &Sessao, ordem: Id) {
        let geracao = fiscal_fundo::geracao();
        if self.ordem == Some(ordem) && self.geracao == geracao {
            return;
        }
        self.ordem = Some(ordem);
        self.geracao = geracao;
        self.nfse = motor
            .consultar(sessao, "fiscal.nfse_da_os.v1", &NfseDaOs { ordem })
            .ok()
            .flatten();
    }
}

/// A seção da NFS-e na OS faturada.
pub(super) fn secao(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut NfseDaOsAberta,
    ordem: Id,
) {
    if !sessao.concede("fiscal.ver") {
        return;
    }
    estado.atualizar(motor, sessao, ordem);
    ui.add(Rotulo::titulo_secao("NFS-e"));
    ui.add_space(Espaco::E8);
    let mut emitir = false;
    let mut abrir_cancelamento = false;
    match estado.nfse.clone() {
        None => {
            ui.add(
                Rotulo::apoio(
                    "A NFS-e da mão de obra desta OS ainda não foi emitida (as peças ficam de \
                     fora: são mercadoria).",
                )
                .quebravel(),
            );
            ui.add_space(Espaco::E8);
            emitir = sessao.concede("fiscal.emitir")
                && ui.add(Botao::primario("Emitir NFS-e")).clicked();
        }
        Some(n) => {
            let texto = format!("DPS {}-{}: {}", n.serie, n.numero, n.estado.rotulo());
            ui.add(
                match n.estado {
                    EstadoNfse::Autorizada => Etiqueta::positiva(texto),
                    EstadoNfse::Rejeitada => Etiqueta::negativa(texto),
                    EstadoNfse::Cancelada => Etiqueta::neutra(texto),
                    _ => Etiqueta::atencao(texto),
                }
                .com_ponto(),
            );
            ui.add_space(Espaco::E8);
            if let Some(numero) = n.numero_nfse {
                dado(ui, "Número da NFS-e", &numero.to_string());
            }
            if let Some(chave) = &n.chave {
                dado(ui, "Chave de acesso", chave);
            }
            if let Some(m) = &n.motivo {
                ui.add(Rotulo::apoio(m.clone()).quebravel());
            }
            if n.cancelamento_pedido {
                ui.add(Etiqueta::atencao("Cancelamento na fila").com_ponto());
            }
            ui.add_space(Espaco::E8);
            ui.horizontal(|ui| {
                if matches!(n.estado, EstadoNfse::Rejeitada | EstadoNfse::Cancelada)
                    && sessao.concede("fiscal.emitir")
                    && ui.add(Botao::secundario("Emitir de novo")).clicked()
                {
                    emitir = true;
                }
                if n.estado == EstadoNfse::Autorizada
                    && !n.cancelamento_pedido
                    && estado.cancelando.is_none()
                    && sessao.concede("fiscal.cancelar")
                    && ui.add(Botao::fantasma("Cancelar NFS-e")).clicked()
                {
                    abrir_cancelamento = true;
                }
            });
        }
    }
    if abrir_cancelamento {
        estado.cancelando = Some((None, String::new()));
    }
    let mut confirmar = None;
    let mut desistir = false;
    if let (Some((motivo, justificativa)), Some(n)) = (estado.cancelando.as_mut(), &estado.nfse) {
        ui.add_space(Espaco::E8);
        let opcoes: Vec<(usize, String)> = MOTIVOS
            .iter()
            .enumerate()
            .map(|(i, m)| (i, m.rotulo().to_owned()))
            .collect();
        SeletorOpcao::novo("Motivo", motivo)
            .opcoes(opcoes)
            .mostrar(ui);
        ui.add(Campo::novo("Justificativa (ao menos 15 caracteres)", justificativa).multilinha(2));
        let pode = motivo.is_some() && justificativa.trim().chars().count() >= JUSTIFICATIVA_MINIMA;
        ui.horizontal(|ui| {
            if ui
                .add(Botao::destrutivo("Confirmar cancelamento").habilitado(pode))
                .clicked()
            {
                confirmar = motivo.map(|i| CancelarNfse {
                    nfse: n.id,
                    motivo: MOTIVOS[i],
                    justificativa: justificativa.trim().to_owned(),
                });
            }
            desistir = ui.add(Botao::fantasma("Desistir")).clicked();
        });
    }
    if desistir {
        estado.cancelando = None;
    }
    if let Some(cmd) = confirmar {
        match motor.executar(sessao, "fiscal.cancelar_nfse.v1", &cmd) {
            Ok(n) => {
                estado.nfse = Some(n);
                estado.cancelando = None;
                fiscal_fundo::disparar(ui.ctx(), motor, sessao);
                notificar(
                    ui.ctx(),
                    Notificacao::sucesso("Cancelamento da NFS-e na fila"),
                );
            }
            Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
        }
    }
    if emitir {
        match motor.executar(
            sessao,
            "fiscal.emitir_nfse_da_os.v1",
            &EmitirNfseDaOs { ordem },
        ) {
            Ok(n) if n.estado == EstadoNfse::Rejeitada => {
                notificar(
                    ui.ctx(),
                    Notificacao::aviso(
                        n.motivo
                            .clone()
                            .unwrap_or_else(|| "A NFS-e não saiu".into()),
                    ),
                );
                estado.nfse = Some(n);
            }
            Ok(n) => {
                fiscal_fundo::disparar(ui.ctx(), motor, sessao);
                notificar(
                    ui.ctx(),
                    Notificacao::sucesso("NFS-e na fila — vai ao município agora"),
                );
                estado.nfse = Some(n);
            }
            Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
        }
    }
}
