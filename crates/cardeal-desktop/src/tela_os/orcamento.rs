//! Itens do orçamento: mão de obra, peça do estoque e a aplicação das peças na execução.

use super::*;

pub(super) fn adicionar_mao_de_obra(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
    os: Id,
) {
    match estado.mao_de_obra_valor.parse::<Dinheiro>() {
        Ok(valor) => {
            let r = motor.executar(
                sessao,
                "os.montar_orcamento.v1",
                &MontarOrcamentoOs {
                    ordem_servico: os,
                    item: ItemOrcamentoNovo::MaoDeObra {
                        descricao: estado.mao_de_obra_descricao.clone(),
                        valor,
                        tecnico: sessao.usuario(),
                        horas: None,
                    },
                },
            );
            match r {
                Ok(id) => {
                    let _: Id = id;
                    estado.mao_de_obra_descricao.clear();
                    estado.mao_de_obra_valor.clear();
                    estado.abrir_detalhe(motor, sessao, os);
                    estado.dlg = Dlg::Detalhe;
                    notificar(ctx, Notificacao::sucesso("Mão de obra adicionada"));
                }
                Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
            }
        }
        Err(_) => notificar(
            ctx,
            Notificacao::aviso("Valor inválido — use o formato 80,00"),
        ),
    }
}

pub(super) fn adicionar_peca(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
    os: Id,
) {
    let Some(produto) = estado.peca_produto else {
        notificar(
            ctx,
            Notificacao::aviso("Escolha o produto da lista de estoque."),
        );
        return;
    };
    let (Ok(quantidade), Ok(preco_unitario)) = (
        estado.peca_qtd.parse::<Quantidade>(),
        estado.peca_preco.parse::<Preco>(),
    ) else {
        notificar(ctx, Notificacao::aviso("Quantidade ou preço inválidos."));
        return;
    };
    let r = motor.executar(
        sessao,
        "os.montar_orcamento.v1",
        &MontarOrcamentoOs {
            ordem_servico: os,
            item: ItemOrcamentoNovo::Peca {
                produto,
                quantidade,
                preco_unitario,
            },
        },
    );
    match r {
        Ok(id) => {
            let _: Id = id;
            estado.peca_produto = None;
            estado.peca_busca.clear();
            estado.peca_qtd.clear();
            estado.peca_preco.clear();
            estado.abrir_detalhe(motor, sessao, os);
            estado.dlg = Dlg::Detalhe;
            notificar(ctx, Notificacao::sucesso("Peça adicionada ao orçamento"));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

/// Aplica as peças `itens` da OS em execução, tirando-as do estoque do local escolhido.
///
/// Uma a uma, na ordem: se alguma falhar (saldo, permissão), para nela, avisa o motivo e
/// recarrega o detalhe — as que já saíram continuam aplicadas, e a tela reflete isso. O
/// aviso de saldo negativo (`gerou_divergencia`) vem do estoque e aparece na hora, sem
/// precisar abrir o estoque para descobrir.
pub(super) fn aplicar_pecas(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
    ordem: Id,
    itens: &[Id],
) {
    let Some(local) = estado.aplicar_local else {
        notificar(
            ctx,
            Notificacao::aviso("Nenhum local de estoque cadastrado — cadastre um no Estoque."),
        );
        return;
    };
    // Um comando só (`os.aplicar_pecas.v1`): ou todas saem do estoque, ou nenhuma.
    match motor.executar(
        sessao,
        "os.aplicar_pecas.v1",
        &AplicarPecas {
            ordem_servico: ordem,
            itens: itens.to_vec(),
            local,
        },
    ) {
        Ok(feitas) => {
            let feitas: Vec<PecaFoiAplicada> = feitas;
            estado.recarregar_lista(motor, sessao);
            estado.carregar_produtos(motor, sessao);
            estado.abrir_detalhe(motor, sessao, ordem);
            estado.dlg = Dlg::Detalhe;
            notificar(
                ctx,
                Notificacao::sucesso(if feitas.len() == 1 {
                    "Peça aplicada — saiu do estoque".to_owned()
                } else {
                    format!("{} peças aplicadas — saíram do estoque", feitas.len())
                }),
            );
            if feitas.iter().any(|f| f.gerou_divergencia) {
                notificar(
                    ctx,
                    Notificacao::aviso(
                        "O saldo de alguma peça ficou negativo no local — confira o estoque.",
                    ),
                );
            }
        }
        Err(e) => notificar(
            ctx,
            Notificacao::erro("Nenhuma peça foi aplicada").detalhe(e.mensagem),
        ),
    }
}
