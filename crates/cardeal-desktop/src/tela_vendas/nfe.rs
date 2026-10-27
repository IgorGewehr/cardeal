//! Vendas › pedido faturado › **NF-e**: emitir a nota do pedido, a situação dela e, rejeitada, o
//! que corrigir (no cadastro do cliente ou do produto) e reenviar. A transmissão é a mesma fila da
//! NFC-e, em segundo plano.

use cardeal_cliente::{Motor, Sessao};
use cardeal_kernel::Id;
use cardeal_ui::atoms::{Botao, Rotulo};
use cardeal_ui::organisms::{notificar, Notificacao};
use cardeal_ui::tokens::Espaco;
use eframe::egui;
use mod_fiscal::{
    DocumentoDoPedido, DocumentoResumo, EmitirNfeDoPedido, EstadoDocumento, ReenviarDocumento,
};

/// A NF-e do pedido aberto (relida quando a fila roda).
#[derive(Default)]
pub(super) struct NfeDoPedido {
    pedido: Option<Id>,
    geracao: u64,
    doc: Option<DocumentoResumo>,
}

impl NfeDoPedido {
    fn atualizar(&mut self, motor: &Motor, sessao: &Sessao, pedido: Id) {
        let geracao = crate::fiscal_fundo::geracao();
        if self.pedido == Some(pedido) && self.geracao == geracao {
            return;
        }
        self.pedido = Some(pedido);
        self.geracao = geracao;
        self.doc = motor
            .consultar(
                sessao,
                "fiscal.documento_do_pedido.v1",
                &DocumentoDoPedido { pedido },
            )
            .ok()
            .flatten();
    }

    /// Guarda o documento que voltou de um comando.
    fn guardar(&mut self, ctx: &egui::Context, motor: &Motor, sessao: &Sessao, d: DocumentoResumo) {
        if d.estado == EstadoDocumento::Pendente {
            crate::fiscal_fundo::disparar(ctx, motor, sessao);
            notificar(
                ctx,
                Notificacao::sucesso("NF-e na fila — vai para a SEFAZ agora"),
            );
        } else {
            notificar(
                ctx,
                Notificacao::erro("A NF-e não saiu")
                    .detalhe(d.motivo.clone().unwrap_or_else(|| d.estado.rotulo().into())),
            );
        }
        self.doc = Some(d);
    }
}

/// A seção da NF-e no pedido faturado.
pub(super) fn secao(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    nfe: &mut NfeDoPedido,
    pedido: Id,
) {
    if !sessao.concede("fiscal.ver") {
        return;
    }
    nfe.atualizar(motor, sessao, pedido);
    let pode = sessao.concede("fiscal.emitir");
    ui.add_space(Espaco::E12);
    let Some(d) = nfe.doc.clone() else {
        if pode && ui.add(Botao::primario("Emitir NF-e")).clicked() {
            let r: Result<DocumentoResumo, _> = motor.executar(
                sessao,
                "fiscal.emitir_nfe_do_pedido.v1",
                &EmitirNfeDoPedido { pedido },
            );
            match r {
                Ok(d) => nfe.guardar(ui.ctx(), motor, sessao, d),
                Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
            }
        }
        return;
    };
    ui.horizontal(|ui| {
        ui.add(Rotulo::campo(format!("NF-e {}-{}", d.serie, d.numero)));
        ui.add(crate::tela_notas::etiqueta(&d).com_ponto());
        if d.estado == EstadoDocumento::Rejeitada
            && pode
            && d.pedido.is_none()
            && ui.add(Botao::secundario("Reenviar").pequeno()).clicked()
        {
            let r: Result<DocumentoResumo, _> = motor.executar(
                sessao,
                "fiscal.reenviar_documento.v1",
                &ReenviarDocumento { documento: d.id },
            );
            match r {
                Ok(novo) => nfe.guardar(ui.ctx(), motor, sessao, novo),
                Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
            }
        }
    });
    if d.estado != EstadoDocumento::Autorizada {
        if let Some(m) = &d.motivo {
            ui.add(Rotulo::apoio(m.clone()).quebravel());
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::testes_comum::motor_de_teste;

    #[test]
    fn sem_nota_mostra_o_botao_e_desenha() {
        let t = motor_de_teste();
        let mut nfe = NfeDoPedido::default();
        let ctx = egui::Context::default();
        cardeal_ui::tokens::instalar_fontes(&ctx);
        cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
        let pedido = Id::novo();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                secao(ui, &t.motor, &t.sessao, &mut nfe, pedido);
            });
        });
        assert_eq!(nfe.pedido, Some(pedido));
        assert!(nfe.doc.is_none());
        // Emitir num pedido que não existe: o erro do motor, sem nota.
        let r: Result<DocumentoResumo, _> = t.motor.executar(
            &t.sessao,
            "fiscal.emitir_nfe_do_pedido.v1",
            &EmitirNfeDoPedido { pedido },
        );
        assert!(r.is_err());
    }
}
