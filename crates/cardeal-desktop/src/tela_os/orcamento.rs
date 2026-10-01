//! Itens do orçamento: mão de obra, peça do estoque e a aplicação das peças na execução.

use super::*;

pub(super) fn adicionar_mao_de_obra(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
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
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaOs,
    os: Id,
) {
    let quantidade_txt = if estado.peca_qtd.trim().is_empty() {
        "1"
    } else {
        estado.peca_qtd.as_str()
    };
    let (Ok(quantidade), Ok(preco_unitario)) = (
        quantidade_txt.parse::<Quantidade>(),
        estado.peca_preco.parse::<Preco>(),
    ) else {
        notificar(ctx, Notificacao::aviso("Quantidade ou preço inválidos."));
        return;
    };
    // Nada escolhido na lista mas algo digitado: é uma peça que ainda não existe no
    // catálogo — cadastra só pelo nome, junto com o orçamento.
    let item = match (estado.peca_produto, estado.peca_busca.trim()) {
        (Some(produto), _) => ItemOrcamentoNovo::Peca {
            produto,
            quantidade,
            preco_unitario,
        },
        (None, "") => {
            notificar(
                ctx,
                Notificacao::aviso("Busque a peça na lista ou digite o nome de uma nova."),
            );
            return;
        }
        (None, nome) => ItemOrcamentoNovo::PecaNova {
            nome: nome.to_owned(),
            quantidade,
            preco_unitario,
        },
    };
    let nova = matches!(item, ItemOrcamentoNovo::PecaNova { .. });
    let r = motor.executar(
        sessao,
        "os.montar_orcamento.v1",
        &MontarOrcamentoOs {
            ordem_servico: os,
            item,
        },
    );
    match r {
        Ok(id) => {
            let _: Id = id;
            estado.peca_produto = None;
            estado.peca_busca.clear();
            estado.peca_qtd.clear();
            estado.peca_preco.clear();
            if nova {
                // A peça nasceu no catálogo agora: o nome dela vem do catálogo recarregado.
                estado.carregar_produtos(motor, sessao);
            }
            estado.recarregar_lista(motor, sessao);
            estado.abrir_detalhe(motor, sessao, os);
            estado.dlg = Dlg::Detalhe;
            notificar(
                ctx,
                Notificacao::sucesso(if nova {
                    "Peça nova cadastrada e adicionada ao orçamento"
                } else {
                    "Peça adicionada ao orçamento"
                }),
            );
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
    motor: &Motor,
    sessao: &Sessao,
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

/// Preenche quantidade 1 e o preço sugerido ao escolher a peça — antes o balcão digitava os
/// dois toda vez. Ordem: a tabela de preço (se a empresa mantém uma), senão o último preço
/// cobrado dessa peça numa OS, senão só mostra o custo como referência.
pub(super) fn sugerir_preco(
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaOs,
    produto: Id,
) {
    if estado.peca_qtd.trim().is_empty() {
        estado.peca_qtd = "1".to_owned();
    }
    let custo = estado
        .produtos
        .iter()
        .find(|p| p.produto == produto)
        .map(|p| p.custo_medio);
    let da_tabela = motor
        .consultar(
            sessao,
            "vendas.tabelas_de_preco.v1",
            &mod_vendas::TabelasDePreco,
        )
        .ok()
        .and_then(|tabelas: Vec<mod_vendas::TabelaPreco>| tabelas.into_iter().next())
        .and_then(|tabela| {
            motor
                .consultar(
                    sessao,
                    "pdv.preco_do_produto.v1",
                    &mod_pdv::PrecoDoProduto {
                        tabela_preco: tabela.id,
                        produto,
                        quantidade: Quantidade::unidades(1),
                    },
                )
                .ok()
                .map(|p: mod_pdv::PrecoConsultado| (p.preco, tabela.nome))
        })
        .filter(|(p, _)| p.unidades_internas() > 0);
    let ultimo = || -> Option<Preco> {
        motor
            .consultar(
                sessao,
                "os.ultimo_preco_da_peca.v1",
                &mod_os::UltimoPrecoDaPeca { produto },
            )
            .ok()
            .flatten()
    };
    let custo_txt = custo
        .filter(|c| c.unidades_internas() > 0)
        .map(|c| format!(" · custo {}", c.formatar_com_simbolo()))
        .unwrap_or_default();
    if let Some((preco, tabela)) = da_tabela {
        estado.peca_preco = preco.formatar();
        estado.peca_preco_dica = format!("Preço da tabela \"{tabela}\"{custo_txt}");
    } else if let Some(preco) = ultimo() {
        estado.peca_preco = preco.formatar();
        estado.peca_preco_dica = format!("Último preço cobrado desta peça{custo_txt}");
    } else {
        estado.peca_preco.clear();
        estado.peca_preco_dica = if custo_txt.is_empty() {
            "Sem preço anterior — informe o valor".to_owned()
        } else {
            format!("Sem preço anterior{custo_txt}")
        };
    }
}
