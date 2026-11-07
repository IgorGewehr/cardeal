//! Os blocos "Peças" e "Serviços" do detalhe da OS. Cada bloco lista o que já está na OS e,
//! enquanto o orçamento aceita edição, traz o formulário de inclusão logo abaixo — o botão só
//! fica ativo com o formulário completo.
//!
//! Peças vêm primeiro e com a situação à vista ("em estoque", "a comprar", "encomendada em …
//! de …", "aplicada"): quando o técnico volta à OS dias depois, a pergunta é "já encomendei?".
//! Um botão só no cabeçalho encomenda todas as que faltam (Compras › Pedidos).

use super::*;

/// O que uma linha pediu (decidido no laço, executado depois — o laço segura `estado`
/// emprestado para ler os nomes das peças).
enum PedidoItem {
    Remover(Id, TipoItemOrcamento),
    Aplicar(Id),
    Abrir(Box<Dlg>),
}

/// O bloco "Peças".
pub(super) fn secao_pecas(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaOs,
    detalhe: &DetalheOrdem,
) {
    let os = &detalhe.ordem;
    let total_pecas = detalhe
        .itens_peca
        .iter()
        .fold(Dinheiro::ZERO, |acc, i| acc + i.total_cobrado());
    // Em falta: nova (digitada, sem saldo) ou do estoque zerado, e ainda não encomendada.
    let a_comprar: Vec<&mod_os::ItemPeca> = detalhe
        .itens_peca
        .iter()
        .filter(|i| !i.aplicada && i.encomenda.is_none() && falta_no_estoque(i, &estado.produtos))
        .collect();
    let mut pedido = None;
    let mut encomendar = false;
    Painel::novo().plano().titulo("Peças").mostrar_com_acoes(
        ui,
        |ui| {
            ui.add(ValorDinheiro::novo(total_pecas));
            if !a_comprar.is_empty() && pode_faturar(os.estado) {
                encomendar = ui
                    .add(
                        Botao::primario(format!("Encomendar peças em falta ({})", a_comprar.len()))
                            .pequeno(),
                    )
                    .clicked();
            }
        },
        |ui| {
            if detalhe.itens_peca.is_empty() {
                ui.add(Rotulo::apoio("Nenhuma peça nesta OS."));
            } else {
                pedido = linhas_de_pecas(ui, estado, detalhe);
            }
            if os.estado.aceita_edicao_de_orcamento() {
                ui.add_space(Espaco::E12);
                ui.add(Divisor::novo());
                ui.add_space(Espaco::E12);
                formulario_peca(ui, motor, sessao, estado, os.id);
            }
        },
    );
    if encomendar {
        let pecas = a_comprar
            .iter()
            .map(|i| crate::tela_compras::PecaEmFalta {
                item_peca: i.id,
                produto: i.produto,
                nome: nome_do_produto(&estado.produtos, i.produto),
                quantidade: i.quantidade,
                custo: i.custo_unitario,
                marcada: true,
            })
            .collect();
        estado.pedido_encomenda = Some(crate::tela_compras::PecasDaOs {
            ordem: os.id,
            numero: os.numero,
            equipamento: equipamento_label(os).to_owned(),
            pecas,
        });
        estado.dlg = Dlg::Fechado;
        return;
    }
    executar_pedido(ui, motor, sessao, estado, os.id, pedido);
}

/// O bloco "Serviços" (mão de obra).
pub(super) fn secao_servicos(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaOs,
    detalhe: &DetalheOrdem,
) {
    let os = &detalhe.ordem;
    let pode_ajustar = os.estado.aceita_ajuste_de_itens();
    let total = detalhe
        .itens_mao_de_obra
        .iter()
        .fold(Dinheiro::ZERO, |acc, i| acc + i.valor);
    let mut pedido = None;
    Painel::novo().plano().titulo("Serviços").mostrar_com_acoes(
        ui,
        |ui| {
            ui.add(ValorDinheiro::novo(total));
        },
        |ui| {
            if detalhe.itens_mao_de_obra.is_empty() {
                ui.add(Rotulo::apoio("Nenhum serviço nesta OS."));
            }
            for (i, item) in detalhe.itens_mao_de_obra.iter().enumerate() {
                if i > 0 {
                    separador(ui);
                }
                ui.horizontal(|ui| {
                    ui.add(Rotulo::interface(item.descricao.clone()));
                    if let Some(h) = item.horas {
                        ui.add(Etiqueta::neutra(format!("{} h previstas", h.formatar(1))));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if pode_ajustar && ui.add(Botao::fantasma("Remover").pequeno()).clicked() {
                            pedido =
                                Some(PedidoItem::Remover(item.id, TipoItemOrcamento::MaoDeObra));
                        }
                        ui.add(ValorDinheiro::novo(item.valor));
                    });
                });
            }
            if os.estado.aceita_edicao_de_orcamento() {
                ui.add_space(Espaco::E12);
                ui.add(Divisor::novo());
                ui.add_space(Espaco::E12);
                formulario_servico(ui, motor, sessao, estado, os.id);
            }
        },
    );
    executar_pedido(ui, motor, sessao, estado, os.id, pedido);
}

fn separador(ui: &mut egui::Ui) {
    ui.add_space(Espaco::E4);
    ui.add(Divisor::novo());
    ui.add_space(Espaco::E4);
}

fn executar_pedido(
    ui: &egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaOs,
    ordem: Id,
    pedido: Option<PedidoItem>,
) {
    match pedido {
        Some(PedidoItem::Abrir(d)) => estado.dlg = *d,
        Some(PedidoItem::Remover(item, tipo)) => aplicar_e_recarregar(
            ui.ctx(),
            motor,
            sessao,
            estado,
            ordem,
            "os.remover_item_orcamento.v1",
            &RemoverItemOrcamento {
                ordem_servico: ordem,
                item,
                tipo,
            },
            "Item removido",
        ),
        Some(PedidoItem::Aplicar(item)) => {
            aplicar_pecas(ui.ctx(), motor, sessao, estado, ordem, &[item]);
        }
        None => {}
    }
}

/// O texto de apoio de uma peça: preço, de quem e quando foi encomendada e o custo.
fn detalhe_da_peca(item: &mod_os::ItemPeca) -> String {
    let mut partes = vec![if item.preco_unitario.e_zero() {
        "preço a definir".to_owned()
    } else {
        format!("{} /un", item.preco_unitario.formatar_com_simbolo())
    }];
    if let Some(e) = &item.encomenda {
        let mut enc = format!("encomendada em {}", e.encomendada_em.formatar());
        if !e.fornecedor.trim().is_empty() {
            enc.push_str(&format!(" de {}", e.fornecedor.trim()));
        }
        partes.push(enc);
        if let Some(c) = e.custo_previsto {
            partes.push(format!("custo combinado {}", c.formatar_com_simbolo()));
        }
    } else if !item.custo_unitario.e_zero() {
        let rotulo = if item.aplicada {
            "custo"
        } else {
            "custo previsto"
        };
        partes.push(format!(
            "{rotulo} {}",
            item.custo_unitario.formatar_com_simbolo()
        ));
    }
    partes.join(" · ")
}

/// Uma linha por peça: nome, situação, valor e ações; embaixo, o detalhe da encomenda.
fn linhas_de_pecas(
    ui: &mut egui::Ui,
    estado: &EstadoTelaOs,
    detalhe: &DetalheOrdem,
) -> Option<PedidoItem> {
    let os = &detalhe.ordem;
    let pode_ajustar = os.estado.aceita_ajuste_de_itens();
    let em_execucao = os.estado == EstadoOs::EmExecucao;
    let pode_valores = pode_ajustar || em_execucao;
    let ativa = pode_faturar(os.estado);
    let mut pedido = None;
    for (i, item) in detalhe.itens_peca.iter().enumerate() {
        if i > 0 {
            separador(ui);
        }
        let nome = nome_do_produto(&estado.produtos, item.produto);
        ui.horizontal(|ui| {
            ui.add(Rotulo::interface(format!(
                "{} × {nome}",
                item.quantidade.formatar(0)
            )));
            ui.add(etiqueta_da_peca(item, &estado.produtos));
            if item.preco_unitario.e_zero() && !item.coberto_garantia {
                ui.add(Etiqueta::atencao("sem preço"));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(ValorDinheiro::novo(item.total_cobrado()));
                // Uma peça já aplicada não sai da OS: o estoque já foi baixado.
                if item.aplicada {
                    return;
                }
                if pode_ajustar && ui.add(Botao::fantasma("Remover").pequeno()).clicked() {
                    pedido = Some(PedidoItem::Remover(item.id, TipoItemOrcamento::Peca));
                }
                if pode_valores && ui.add(Botao::fantasma("Valores").pequeno()).clicked() {
                    let form = FormValoresPeca::de_item(item, nome.clone());
                    pedido = Some(PedidoItem::Abrir(Box::new(Dlg::ValoresPeca(Box::new(
                        form,
                    )))));
                }
                if !ativa {
                    return;
                }
                let falta = falta_no_estoque(item, &estado.produtos);
                if em_execucao && !falta && ui.add(Botao::secundario("Aplicar").pequeno()).clicked()
                {
                    pedido = Some(PedidoItem::Aplicar(item.id));
                }
                if item.encomenda.is_some()
                    && ui.add(Botao::secundario("Chegou").pequeno()).clicked()
                {
                    let form = FormChegada::novo(
                        os.id,
                        item.id,
                        item.encomenda.as_ref(),
                        nome.clone(),
                        true,
                    );
                    pedido = Some(PedidoItem::Abrir(Box::new(Dlg::Chegou(Box::new(form)))));
                }
            });
        });
        ui.add(Rotulo::apoio(detalhe_da_peca(item)).quebravel());
    }
    pedido
}

/// "Adicionar serviço": descrição, valor e horas previstas numa linha.
fn formulario_servico(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaOs,
    ordem: Id,
) {
    let ok = !estado.mao_de_obra_descricao.trim().is_empty()
        && !estado.mao_de_obra_valor.trim().is_empty();
    let mut adicionar = false;
    ui.columns(4, |c| {
        c[0].add(
            Campo::novo("Novo serviço", &mut estado.mao_de_obra_descricao)
                .marcador("Ex.: troca de tela"),
        );
        c[1].add(Campo::novo("Valor", &mut estado.mao_de_obra_valor).marcador("80,00"));
        c[2].add(
            Campo::novo("Horas previstas", &mut estado.mao_de_obra_horas).marcador("opcional"),
        );
        c[3].add_space(Espaco::E24);
        adicionar = c[3]
            .add(
                Botao::secundario("+ Adicionar serviço")
                    .preenche_largura()
                    .habilitado(ok),
            )
            .clicked();
    });
    if adicionar {
        adicionar_mao_de_obra(ui.ctx(), motor, sessao, estado, ordem);
    }
}

/// "Adicionar peça": busca no estoque (ou nome novo), quantidade, custo (fora do estoque) e
/// preço ao cliente — custo e preço opcionais (`orcamento::valores_da_peca`).
fn formulario_peca(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaOs,
    ordem: Id,
) {
    // TODO(backend): a busca compara só o nome do produto — quando o código curto de
    // rastreabilidade (post-it físico na peça) existir em `ItemProdutoComSaldo`, somar
    // `p.codigo_curto` ao `subtitulo` basta para o técnico digitar o código de bancada.
    let opcoes: Vec<OpcaoBusca<Id>> = estado
        .produtos
        .iter()
        .map(|p| {
            OpcaoBusca::nova(p.produto, p.nome.clone())
                .subtitulo(format!("{} disponível(is)", p.disponivel))
        })
        .collect();
    let ok = estado.peca_produto.is_some() || !estado.peca_busca.trim().is_empty();
    let fora = ok && peca_fora_do_estoque(estado);
    let antes = estado.peca_produto;
    let mut adicionar = false;
    ui.columns(5, |c| {
        SeletorBusca::novo(
            "Nova peça",
            &mut estado.peca_busca,
            &mut estado.peca_produto,
        )
        .opcoes(opcoes)
        .marcador("Buscar no estoque ou digitar uma nova…")
        .mostrar(&mut c[0]);
        c[1].add(Campo::novo("Quantidade", &mut estado.peca_qtd).marcador("1"));
        if fora {
            c[2].add(Campo::novo("Meu custo (un.)", &mut estado.peca_custo).marcador("opcional"));
        }
        c[3].add(
            Campo::novo("Preço ao cliente (un.)", &mut estado.peca_preco).marcador(if fora {
                "= custo"
            } else {
                "opcional"
            }),
        );
        c[4].add_space(Espaco::E24);
        adicionar = c[4]
            .add(
                Botao::secundario("+ Adicionar peça")
                    .preenche_largura()
                    .habilitado(ok),
            )
            .clicked();
    });
    if fora {
        ui.add(
            Rotulo::apoio(
                "Fora do estoque: custo e preço são opcionais. Só um preenchido vale para os dois \
                 (repasse a preço de custo — o lucro vem do serviço); nenhum, defina depois em \
                 \"Valores\".",
            )
            .quebravel(),
        );
    } else if !estado.peca_preco_dica.is_empty() {
        ui.add(Rotulo::apoio(estado.peca_preco_dica.clone()).quebravel());
    }
    if let Some(produto) = estado.peca_produto.filter(|p| Some(*p) != antes) {
        sugerir_preco(motor, sessao, estado, produto);
    }
    if adicionar {
        adicionar_peca(ui.ctx(), motor, sessao, estado, ordem);
    }
}
