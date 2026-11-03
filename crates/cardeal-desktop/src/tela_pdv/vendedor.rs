//! `F4`: o vendedor da venda — quem atendeu e ganha a comissão (o operador do caixa só passa a
//! venda). Vale para uma venda: some junto com o cliente ao finalizar ou cancelar, para a
//! comissão não cair no vendedor errado quando o caixa esquece de trocar. Sem vendedor, a venda
//! não gera comissão.

use super::*;
use mod_pdv::DefinirVendedorDoCupom;

/// Os vendedores que casam com a busca (pelo nome, sem acento nem caixa).
pub(super) fn que_casam<'a>(vendedores: &'a [(Id, String)], busca: &str) -> Vec<&'a (Id, String)> {
    vendedores
        .iter()
        .filter(|(_, nome)| casa_por_palavras(nome, busca))
        .collect()
}

/// O diálogo do `F4`. Não fala com o motor: escolher empilha [`Acao::EscolherVendedor`]. `↑↓`
/// percorrem, `Enter` escolhe, `Esc` fecha; "Sem vendedor" tira o vendedor da venda.
pub(super) fn dialogo(ctx: &egui::Context, estado: &mut EstadoTelaPdv) {
    let Dlg::Vendedor { busca, sel } = &mut estado.dlg else {
        return;
    };
    let quantos = que_casam(&estado.vendedores, busca).len();
    for (tecla, subir) in [(egui::Key::ArrowUp, true), (egui::Key::ArrowDown, false)] {
        if consumir(ctx, tecla) {
            *sel = if subir {
                sel.saturating_sub(1)
            } else {
                (*sel + 1).min(quantos.saturating_sub(1))
            };
        }
    }
    let fechar = Dialogo::nova("Vendedor da venda")
        .descricao("Quem atendeu: a comissão da venda vai para ele.")
        .medio()
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                let Dlg::Vendedor { busca, sel } = &mut estado.dlg else {
                    return;
                };
                let resp =
                    ui.add(Campo::novo("Buscar vendedor", busca).marcador("Nome do vendedor"));
                let confirmou = resp.lost_focus() && enter_pressionado(ui);
                if resp.changed() {
                    *sel = 0;
                }
                focar_primeiro(ctx, &resp);
                ui.add_space(Espaco::E12);
                let achados: Vec<(Id, String)> = que_casam(&estado.vendedores, busca)
                    .into_iter()
                    .cloned()
                    .collect();
                if achados.is_empty() {
                    ui.add(Rotulo::campo(if estado.vendedores.is_empty() {
                        "Nenhum usuário ativo na empresa."
                    } else {
                        "Nenhum vendedor encontrado."
                    }));
                    return;
                }
                let destacado = (*sel).min(achados.len() - 1);
                for (i, (id, nome)) in achados.iter().enumerate() {
                    let item = ItemDeLista::novo(nome.clone()).selecionado(i == destacado);
                    if item.mostrar(ui, |_| {}).clicked() {
                        estado
                            .pendentes
                            .push(Acao::EscolherVendedor(Some((*id, nome.clone()))));
                    }
                }
                if confirmou {
                    estado
                        .pendentes
                        .push(Acao::EscolherVendedor(Some(achados[destacado].clone())));
                }
            },
            |ui, estado| {
                if ui.add(Botao::secundario("Fechar").atalho("Esc")).clicked() {
                    estado.dlg = Dlg::Fechado;
                    return;
                }
                if ui.add(Botao::secundario("Sem vendedor")).clicked() {
                    estado.pendentes.push(Acao::EscolherVendedor(None));
                }
            },
        );
    if fechar {
        estado.dlg = Dlg::Fechado;
        estado.foco_entrada = true;
    }
}

/// Grava a escolha: no cupom aberto, já; sem cupom, ela vai na abertura do próximo.
pub(super) fn escolher(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaPdv,
    escolhido: Option<(Id, String)>,
) {
    if let Some(cupom) = estado.cupom {
        if let Err(e) = motor.executar(
            sessao,
            "pdv.definir_vendedor_do_cupom.v1",
            &DefinirVendedorDoCupom {
                cupom,
                vendedor: escolhido.as_ref().map(|v| v.0),
            },
        ) {
            notificar(ctx, Notificacao::erro(e.mensagem));
            return;
        }
    }
    let aviso = match &escolhido {
        Some((_, nome)) => Notificacao::sucesso(format!("Vendedor: {nome}")),
        None => Notificacao::info("Venda sem vendedor (sem comissão)"),
    };
    notificar(ctx, aviso);
    estado.vendedor = escolhido;
    estado.dlg = Dlg::Fechado;
    estado.foco_entrada = true;
}
