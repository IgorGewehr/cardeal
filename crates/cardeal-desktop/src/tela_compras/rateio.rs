//! "Frete e despesas": na conferência da nota, escolher **quais itens** levam o frete (pedido
//! do usuário, 2026-11-07 — numa compra de 10 itens, o frete pode ser só das 5 peças de uma
//! OS). O custo final de cada item já mostra o rateio, e é ele que entra no estoque e no
//! custo da peça na OS.

use super::*;

/// A seção, acima dos itens. Só aparece quando a nota tem despesas a ratear.
pub(super) fn secao(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaCompras,
    nota: Id,
    editavel: bool,
) {
    let despesas = estado
        .itens_nota
        .iter()
        .fold(Dinheiro::ZERO, |acc, i| acc + i.valor_rateio);
    if !despesas.e_positivo() {
        return;
    }
    let atuais: Vec<Id> = estado
        .itens_nota
        .iter()
        .filter(|i| i.valor_rateio.e_positivo())
        .map(|i| i.id)
        .collect();
    if estado
        .frete_escolhidos
        .as_ref()
        .is_none_or(|(n, _)| *n != nota)
    {
        estado.frete_escolhidos = Some((nota, atuais.clone()));
    }
    let mut aplicar = false;
    Painel::novo()
        .plano()
        .compacto()
        .titulo("Frete e despesas")
        .mostrar_com_acoes(
            ui,
            |ui| {
                ui.add(ValorDinheiro::novo(despesas));
            },
            |ui| {
                let Some((_, escolhidos)) = estado.frete_escolhidos.as_mut() else {
                    return;
                };
                ui.add(
                    Rotulo::apoio(if editavel {
                        "Marque os itens que levam o frete — o valor se divide só entre eles \
                         (pelo valor de cada um). Os outros ficam sem frete no custo."
                    } else {
                        "Como o frete foi dividido entre os itens desta nota."
                    })
                    .quebravel(),
                );
                ui.add_space(Espaco::E8);
                for item in &estado.itens_nota {
                    let mut marcado = escolhidos.contains(&item.id);
                    ui.horizontal(|ui| {
                        let rotulo = format!("{} × {}", item.quantidade, item.descricao_fornecedor);
                        if editavel {
                            ui.add(Caixa::nova(&mut marcado, rotulo));
                        } else {
                            ui.add(Rotulo::interface(rotulo));
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if item.valor_rateio.e_positivo() {
                                ui.add(ValorDinheiro::novo(item.valor_rateio));
                                ui.add(Rotulo::campo("frete"));
                            } else {
                                ui.add(Rotulo::campo("sem frete"));
                            }
                        });
                    });
                    if marcado && !escolhidos.contains(&item.id) {
                        escolhidos.push(item.id);
                    } else if !marcado {
                        escolhidos.retain(|i| *i != item.id);
                    }
                }
                if editavel {
                    let mudou = {
                        let mut a = atuais.clone();
                        let mut b = escolhidos.clone();
                        a.sort_by_key(|i| i.em_bytes());
                        b.sort_by_key(|i| i.em_bytes());
                        a != b
                    };
                    ui.add_space(Espaco::E8);
                    ui.horizontal(|ui| {
                        aplicar = ui
                            .add(
                                Botao::primario("Dividir o frete entre os marcados")
                                    .pequeno()
                                    .habilitado(mudou && !escolhidos.is_empty()),
                            )
                            .clicked();
                        if escolhidos.is_empty() {
                            ui.add(Etiqueta::atencao("marque ao menos um item"));
                        }
                    });
                }
            },
        );
    ui.add_space(Espaco::E12);

    if !aplicar {
        return;
    }
    let itens = estado
        .frete_escolhidos
        .as_ref()
        .map(|(_, v)| v.clone())
        .unwrap_or_default();
    match motor.executar(
        sessao,
        "compras.ratear_despesas_entre.v1",
        &mod_compras::RatearDespesasEntre {
            nota_entrada: nota,
            itens,
        },
    ) {
        Ok(()) => {
            estado.frete_escolhidos = None;
            estado.abrir_nota(motor, sessao, nota);
            notificar(
                ui.ctx(),
                Notificacao::sucesso("Frete dividido entre os itens marcados"),
            );
        }
        Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
    }
}
