//! Itens do orçamento: mão de obra, peça do estoque e a aplicação das peças na execução.

use super::*;

pub(super) fn adicionar_mao_de_obra(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaOs,
    os: Id,
) {
    let horas = match estado.mao_de_obra_horas.trim() {
        "" => None,
        h => match Quantidade::de_str(h) {
            Ok(q) if q.e_positiva() => Some(q),
            _ => {
                notificar(
                    ctx,
                    Notificacao::aviso(
                        "Horas inválidas — use, por exemplo, 1,5 ou deixe em branco",
                    ),
                );
                return;
            }
        },
    };
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
                        horas,
                    },
                },
            );
            match r {
                Ok(id) => {
                    let _: Id = id;
                    estado.mao_de_obra_descricao.clear();
                    estado.mao_de_obra_valor.clear();
                    estado.mao_de_obra_horas.clear();
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

/// O custo e o preço de uma peça a partir do que foi digitado (os dois opcionais): só o custo
/// → o cliente paga o custo; só o preço → o preço inteiro é custo (repasse; o lucro sai do
/// serviço); nenhum → zero, a definir depois. `None` se algum não for um valor.
pub(super) fn valores_da_peca(custo: &str, preco: &str) -> Option<(Preco, Preco)> {
    let ler = |t: &str| -> Option<Option<Preco>> {
        match t.trim() {
            "" => Some(None),
            t => t.parse::<Preco>().ok().map(Some),
        }
    };
    let (c, p) = (ler(custo)?, ler(preco)?);
    Some(match (c, p) {
        (None, None) => (Preco::ZERO, Preco::ZERO),
        (Some(c), None) => (c, c),
        (None, Some(p)) => (p, p),
        (Some(c), Some(p)) => (c, p),
    })
}

/// A peça do formulário não tem saldo: nova (digitada) ou do catálogo com estoque zerado.
pub(super) fn peca_fora_do_estoque(estado: &EstadoTelaOs) -> bool {
    match estado.peca_produto {
        None => true,
        Some(id) => estado
            .produtos
            .iter()
            .find(|p| p.produto == id)
            .is_none_or(|p| !p.disponivel.e_positiva()),
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
    let Ok(quantidade) = quantidade_txt.parse::<Quantidade>() else {
        notificar(ctx, Notificacao::aviso("Quantidade inválida."));
        return;
    };
    let fora_do_estoque = peca_fora_do_estoque(estado);
    let custo_txt = if fora_do_estoque {
        estado.peca_custo.as_str()
    } else {
        ""
    };
    let Some((custo, preco_unitario)) = valores_da_peca(custo_txt, &estado.peca_preco) else {
        notificar(
            ctx,
            Notificacao::aviso("Custo ou preço inválidos (ex.: 120,00)."),
        );
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
            grupo: estado.peca_grupo,
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
            let item: Id = id;
            // Fora do estoque, o custo previsto fica no item (a margem usa até a peça chegar).
            if fora_do_estoque && !custo.e_zero() {
                if let Err(e) = motor.executar(
                    sessao,
                    "os.definir_preco_da_peca.v1",
                    &mod_os::DefinirPrecoDaPeca {
                        ordem_servico: os,
                        item_peca: item,
                        preco_unitario,
                        custo_previsto: Some(custo),
                    },
                ) {
                    notificar(ctx, Notificacao::erro(e.mensagem));
                }
            }
            estado.peca_produto = None;
            estado.peca_custo.clear();
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
/// dois toda vez. Ordem (`escolher_sugestao`): a tabela de preço (se a empresa mantém uma), o
/// markup da categoria sobre o custo, o último preço cobrado dessa peça numa OS; sem nenhum,
/// só mostra o custo como referência.
pub(super) fn sugerir_preco(
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaOs,
    produto: Id,
) {
    if estado.peca_qtd.trim().is_empty() {
        estado.peca_qtd = "1".to_owned();
    }
    estado.peca_preco_manual = false;
    let custo = estado
        .produtos
        .iter()
        .find(|p| p.produto == produto)
        .map(|p| p.custo_medio);
    let custo_txt = custo
        .filter(|c| c.unidades_internas() > 0)
        .map(|c| format!(" · custo {}", c.formatar_com_simbolo()))
        .unwrap_or_default();
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
        .filter(|(p, _)| p.unidades_internas() > 0)
        .map(|(p, tabela)| (p, format!("Preço da tabela \"{tabela}\"{custo_txt}")));
    let markup = motor
        .consultar(
            sessao,
            "estoque.preco_sugerido_do_produto.v1",
            &mod_estoque::PrecoSugeridoDoProduto {
                produto,
                custo: None,
            },
        )
        .ok()
        .and_then(|s: mod_estoque::SugestaoDePreco| dica_do_markup(&s));
    let ultimo = motor
        .consultar(
            sessao,
            "os.ultimo_preco_da_peca.v1",
            &mod_os::UltimoPrecoDaPeca { produto },
        )
        .ok()
        .flatten()
        .map(|p: Preco| (p, format!("Último preço cobrado desta peça{custo_txt}")));
    match escolher_sugestao(da_tabela, markup, ultimo) {
        Some((preco, dica)) => {
            estado.peca_preco = preco.formatar();
            estado.peca_preco_dica = dica;
        }
        None => {
            estado.peca_preco.clear();
            estado.peca_preco_dica = if custo_txt.is_empty() {
                "Sem preço anterior — informe o valor".to_owned()
            } else {
                format!("Sem preço anterior{custo_txt}")
            };
        }
    }
}

/// A sugestão que vale: a tabela de preço (o preço que a empresa decidiu) vence o markup, e o
/// markup (a margem da categoria sobre o custo) vence o último preço cobrado.
pub(super) fn escolher_sugestao(
    tabela: Option<(Preco, String)>,
    markup: Option<(Preco, String)>,
    ultimo: Option<(Preco, String)>,
) -> Option<(Preco, String)> {
    tabela.or(markup).or(ultimo)
}

/// O preço do markup e a frase que diz de onde ele veio; `None` sem preço possível (sem custo,
/// sem margem, percentuais que não fecham).
pub(super) fn dica_do_markup(s: &mod_estoque::SugestaoDePreco) -> Option<(Preco, String)> {
    let p = s.sugerido.as_ref()?;
    let origem = match &s.origem_margem {
        mod_estoque::OrigemMargem::Grupo(nome) => format!("pela categoria {nome}"),
        mod_estoque::OrigemMargem::Produto => "pela margem do produto".to_owned(),
        mod_estoque::OrigemMargem::Padrao => "pela margem padrão da empresa".to_owned(),
        mod_estoque::OrigemMargem::Nenhuma => return None,
    };
    let margem = s
        .margem
        .map(Percentual::formatar_com_simbolo)
        .unwrap_or_default();
    Some((
        Preco::centavos(p.preco.em_centavos()),
        format!(
            "Sugerido {origem} · custo {} · lucro {margem}",
            p.custo.formatar_com_simbolo()
        ),
    ))
}

/// A categoria da peça nova digitada: a escolhida ou, sem escolha, o grupo padrão "Peças"
/// (o mesmo em que `estoque.criar_peca_rapida` a cadastra).
pub(super) fn grupo_da_peca_nova(estado: &EstadoTelaOs) -> Option<Id> {
    estado.peca_grupo.or_else(|| {
        estado
            .grupos
            .iter()
            .find(|g| g.codigo == "PECAS")
            .map(|g| g.id)
    })
}

/// O custo digitado mudou: o preço ao cliente acompanha pelo markup — a não ser que o usuário
/// já tenha digitado o preço (aí o dele manda). Peça do catálogo usa a margem do produto e da
/// categoria dele; peça nova, a da categoria escolhida.
pub(super) fn recalcular_preco_pelo_custo(
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaOs,
) {
    if estado.peca_preco_manual {
        return;
    }
    let Ok(custo) = estado.peca_custo.trim().parse::<Dinheiro>() else {
        return;
    };
    if !custo.e_positivo() {
        return;
    }
    let sugestao: Option<mod_estoque::SugestaoDePreco> = match estado.peca_produto {
        Some(produto) => motor
            .consultar(
                sessao,
                "estoque.preco_sugerido_do_produto.v1",
                &mod_estoque::PrecoSugeridoDoProduto {
                    produto,
                    custo: Some(custo),
                },
            )
            .ok(),
        None => grupo_da_peca_nova(estado).and_then(|grupo| {
            motor
                .consultar(
                    sessao,
                    "estoque.preco_sugerido_no_grupo.v1",
                    &mod_estoque::PrecoSugeridoNoGrupo { grupo, custo },
                )
                .ok()
        }),
    };
    match sugestao.as_ref().and_then(dica_do_markup) {
        Some((preco, dica)) => {
            estado.peca_preco = preco.formatar();
            estado.peca_preco_dica = dica;
        }
        None => {
            estado.peca_preco_dica = sugestao
                .and_then(|s| s.problema)
                .map(|p| format!("Sem preço sugerido: {p}"))
                .unwrap_or_default();
        }
    }
}
