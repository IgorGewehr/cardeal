//! "Estornar entrada" no diálogo de uma nota confirmada: mostra o que vai acontecer (o que sai
//! do estoque, a conta a pagar cancelada, o pagamento) antes de pedir o motivo. A mercadoria
//! que já saiu do local bloqueia; pagamento só se estorna marcando.

use cardeal_cliente::{Motor, Sessao};
use cardeal_kernel::Id;
use cardeal_ui::atoms::{Botao, Caixa, Rotulo, Tom};
use cardeal_ui::molecules::Campo;
use cardeal_ui::organisms::{notificar, Notificacao, Painel};
use cardeal_ui::tokens::Espaco;
use eframe::egui;
use mod_compras::{
    EntradaFoiEstornada, EstornarEntrada, PreviaDoEstornoDaEntrada, PreviaEstornoEntrada,
    MOTIVO_MINIMO_ESTORNO,
};

/// O formulário do estorno, aberto ou não.
#[derive(Default)]
pub(super) struct EstadoEstorno {
    previa: Option<PreviaEstornoEntrada>,
    motivo: String,
    estornar_pagamento: bool,
}

/// O bloco do estorno. Devolve `true` quando estornou (a tela recarrega a nota).
pub(super) fn secao(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoEstorno,
    nota: Id,
) -> bool {
    if !sessao.concede("compras.entrada.estornar") {
        return false;
    }
    let Some(previa) = estado.previa.clone() else {
        ui.add(
            Rotulo::apoio(
                "Entrada dada errado? Estorne para corrigir: a mercadoria sai do estoque, a conta \
                 a pagar é cancelada e a nota volta para a conferência.",
            )
            .quebravel(),
        );
        ui.add_space(Espaco::E8);
        if ui.add(Botao::secundario("Estornar entrada…")).clicked() {
            match motor.consultar(
                sessao,
                "compras.previa_estorno_entrada.v1",
                &PreviaDoEstornoDaEntrada { nota },
            ) {
                Ok(p) => estado.previa = Some(p),
                Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
            }
        }
        return false;
    };

    ui.add(Rotulo::titulo_secao("Estornar entrada"));
    ui.add_space(Espaco::E8);
    for item in &previa.itens {
        ui.add(Rotulo::interface(format!(
            "Sai do estoque: {} — {}",
            item.produto,
            item.entrou.formatar(0)
        )));
    }
    match previa.titulo {
        Some(valor) => ui.add(Rotulo::interface(format!(
            "A conta a pagar de {} é cancelada.",
            valor.formatar_com_simbolo()
        ))),
        None => ui.add(Rotulo::campo("A nota não gerou conta a pagar.")),
    };
    ui.add_space(Espaco::E8);
    if let Some(bloqueio) = previa.bloqueio() {
        Painel::novo().realce(Tom::Atencao).mostrar(ui, |ui| {
            ui.add(Rotulo::interface(bloqueio).quebravel());
        });
        ui.add_space(Espaco::E8);
        if ui.add(Botao::fantasma("Fechar o estorno")).clicked() {
            *estado = EstadoEstorno::default();
        }
        return false;
    }
    let tem_pagamento = previa.pago.e_positivo();
    if tem_pagamento {
        ui.add(Caixa::nova(
            &mut estado.estornar_pagamento,
            format!(
                "Estornar também o pagamento de {} — o dinheiro volta para a conta de onde saiu",
                previa.pago.formatar_com_simbolo()
            ),
        ));
        ui.add_space(Espaco::E8);
    }
    ui.add(
        Campo::novo("Motivo (vai para o histórico)", &mut estado.motivo)
            .marcador("Ex.: produto vinculado errado na conferência"),
    );
    ui.add_space(Espaco::E8);
    let pode = estado.motivo.trim().chars().count() >= MOTIVO_MINIMO_ESTORNO
        && (!tem_pagamento || estado.estornar_pagamento);
    let mut estornou = false;
    ui.horizontal(|ui| {
        if ui
            .add(Botao::destrutivo("Estornar entrada").habilitado(pode))
            .clicked()
        {
            let r: Result<EntradaFoiEstornada, _> = motor.executar(
                sessao,
                "compras.estornar_entrada.v1",
                &EstornarEntrada {
                    nota_entrada: nota,
                    motivo: estado.motivo.trim().to_owned(),
                    estornar_pagamento: estado.estornar_pagamento,
                },
            );
            match r {
                Ok(feito) => {
                    let msg = if feito.pagamento_estornado.e_positivo() {
                        format!(
                            "Entrada estornada, com o pagamento de {} — a nota voltou para a \
                             conferência",
                            feito.pagamento_estornado.formatar_com_simbolo()
                        )
                    } else {
                        "Entrada estornada — a nota voltou para a conferência".to_owned()
                    };
                    notificar(ui.ctx(), Notificacao::sucesso(msg));
                    *estado = EstadoEstorno::default();
                    estornou = true;
                }
                Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
            }
        }
        if ui.add(Botao::fantasma("Cancelar")).clicked() {
            *estado = EstadoEstorno::default();
        }
    });
    if !pode {
        ui.add(Rotulo::campo(
            if tem_pagamento && !estado.estornar_pagamento {
                "Para estornar, marque o estorno do pagamento e explique o motivo."
            } else {
                "Explique o motivo (ao menos 10 caracteres)."
            },
        ));
    }
    estornou
}
