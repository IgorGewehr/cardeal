//! Tela de Vendas — pedidos + tabelas de preço. `docs/modulos/vendas.md`.
//!
//! Fluxo completo: criar rascunho (cliente + tabela + local) → adicionar itens (preço
//! resolvido da tabela) → confirmar → faturar. As tabelas de preço e suas regras são
//! geridas num dialog à parte, já que `AdicionarItemPedido` exige uma regra que cubra o
//! produto.

use std::collections::HashMap;

use cardeal_cliente::{MotorLocal, SessaoLocal};
use cardeal_kernel::{Id, Percentual};
use cardeal_modkit::Icone;
use cardeal_ui::atoms::{Botao, Rotulo, ValorDinheiro};
use cardeal_ui::molecules::{Campo, EstadoVazio, SeletorOpcao};
use cardeal_ui::organisms::{notificar, ColunaGrade, Dialogo, Grade, LayoutTela, Notificacao};
use cardeal_ui::tokens::{Espaco, TemaUi};
use eframe::egui;
use mod_clientes::{ItemPessoa, Papel, PessoasPorPapel};
use mod_estoque::{ItemLocal, ItemProdutoComSaldo, Locais, ProdutosComSaldo};
use mod_vendas::{
    AdicionarItemPedido, CancelarPedido, ConfirmarPedido, CriarPedido, CriarRegraPreco,
    CriarTabelaPreco, EstadoPedido, FaturarPedido, ItemPedido, ItemVenda, ItensDoPedido,
    PedidoCriado, PedidosRecentes, RegraPreco, RegrasDaTabela, TabelaPreco, TabelasDePreco,
    TipoTabela,
};

#[derive(Default, PartialEq)]
enum Dlg {
    #[default]
    Fechado,
    Ver(usize),
    Novo,
    Tabelas,
}

/// Estado local da tela.
#[derive(Default)]
pub struct EstadoTelaVendas {
    pedidos: Vec<ItemPedido>,
    nomes: HashMap<Id, String>,
    clientes: Vec<ItemPessoa>,
    produtos: Vec<ItemProdutoComSaldo>,
    tabelas: Vec<TabelaPreco>,
    locais: Vec<ItemLocal>,
    itens_ped: Vec<ItemVenda>,
    regras: Vec<RegraPreco>,
    erro: Option<String>,
    dlg: Dlg,

    // rascunhos de formulário
    novo_cliente: Option<Id>,
    novo_tabela: Option<Id>,
    novo_local: Option<Id>,
    item_produto: Option<Id>,
    item_qtd: String,
    tab_nome: String,
    tab_sel: Option<Id>,
    regra_produto: Option<Id>,
    regra_preco: String,
}

impl EstadoTelaVendas {
    /// Recarrega pedidos + catálogos (clientes, produtos, tabelas, locais).
    pub fn carregar(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        match motor.consultar(sessao, "vendas.pedidos_recentes.v1", &PedidosRecentes) {
            Ok(p) => {
                self.pedidos = p;
                self.erro = None;
            }
            Err(e) => self.erro = Some(e.mensagem),
        }
        if let Ok(c) = motor.consultar(
            sessao,
            "clientes.pessoas_por_papel.v1",
            &PessoasPorPapel {
                papel: Papel::Cliente,
                busca: None,
            },
        ) {
            self.nomes = c.iter().map(|p| (p.pessoa, p.nome.clone())).collect();
            self.clientes = c;
        }
        if let Ok(p) = motor.consultar(sessao, "estoque.produtos_com_saldo.v1", &ProdutosComSaldo) {
            self.produtos = p;
        }
        if let Ok(t) = motor.consultar(sessao, "vendas.tabelas_de_preco.v1", &TabelasDePreco) {
            self.tabelas = t;
        }
        if let Ok(l) = motor.consultar(sessao, "estoque.locais.v1", &Locais) {
            self.locais = l;
        }
    }

    fn abrir_pedido(&mut self, motor: &MotorLocal, sessao: &SessaoLocal, i: usize) {
        let Some(p) = self.pedidos.get(i) else { return };
        let id = p.pedido;
        self.itens_ped = motor
            .consultar(
                sessao,
                "vendas.itens_do_pedido.v1",
                &ItensDoPedido { pedido: id },
            )
            .unwrap_or_default();
        self.dlg = Dlg::Ver(i);
    }

    fn recarregar_itens(&mut self, motor: &MotorLocal, sessao: &SessaoLocal, pedido: Id) {
        self.itens_ped = motor
            .consultar(
                sessao,
                "vendas.itens_do_pedido.v1",
                &ItensDoPedido { pedido },
            )
            .unwrap_or_default();
    }

    fn carregar_regras(&mut self, motor: &MotorLocal, sessao: &SessaoLocal, tabela: Id) {
        self.regras = motor
            .consultar(
                sessao,
                "vendas.regras_da_tabela.v1",
                &RegrasDaTabela { tabela },
            )
            .unwrap_or_default();
    }

    fn nome(&self, cliente: Id) -> String {
        self.nomes
            .get(&cliente)
            .cloned()
            .unwrap_or_else(|| "—".to_owned())
    }

    fn nome_produto(&self, produto: Id) -> String {
        self.produtos
            .iter()
            .find(|p| p.produto == produto)
            .map_or_else(|| "produto".to_owned(), |p| p.nome.clone())
    }
}

/// Desenha a tela inteira.
pub fn mostrar(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaVendas,
) {
    LayoutTela::nova("Vendas").mostrar(
        ui,
        estado,
        |ui, estado| {
            if ui.add(Botao::secundario("Tabelas de preço")).clicked() {
                estado.dlg = Dlg::Tabelas;
            }
            if ui
                .add(Botao::primario("+ Novo pedido").atalho("Ctrl+N"))
                .clicked()
            {
                estado.novo_cliente = None;
                estado.novo_tabela = estado.tabelas.first().map(|t| t.id);
                estado.novo_local = estado.locais.first().map(|l| l.id);
                estado.dlg = Dlg::Novo;
            }
        },
        |ui, estado| {
            if let Some(erro) = &estado.erro {
                ui.add(
                    Rotulo::interface(erro.clone())
                        .quebravel()
                        .cor(ui.cores().negativo),
                );
                ui.add_space(Espaco::E12);
            }
            lista(ui, motor, sessao, estado);
        },
    );

    match estado.dlg {
        Dlg::Fechado => {}
        Dlg::Ver(i) => dialogo_ver(ui.ctx(), motor, sessao, estado, i),
        Dlg::Novo => dialogo_novo(ui.ctx(), motor, sessao, estado),
        Dlg::Tabelas => dialogo_tabelas(ui.ctx(), motor, sessao, estado),
    }
}

fn lista(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaVendas,
) {
    if estado.pedidos.is_empty() {
        if estado.erro.is_none() {
            EstadoVazio::novo(Icone::Carrinho, "Nenhum pedido ainda.").mostrar(ui);
        }
        return;
    }
    let colunas = vec![
        ColunaGrade::nova("Cliente"),
        ColunaGrade::nova("Data").largura(120.0),
        ColunaGrade::nova("Itens").largura(70.0),
        ColunaGrade::nova("Total").largura(130.0),
        ColunaGrade::nova("Estado").largura(120.0),
    ];
    let clicada =
        Grade::nova(colunas)
            .selecionavel(None)
            .mostrar(ui, estado.pedidos.len(), |i, row| {
                let p = &estado.pedidos[i];
                row.col(|ui| {
                    ui.add(Rotulo::interface(estado.nome(p.cliente)));
                });
                row.col(|ui| {
                    ui.add(Rotulo::interface(p.data.to_string()));
                });
                row.col(|ui| {
                    ui.add(Rotulo::interface(p.itens.to_string()));
                });
                row.col(|ui| {
                    ui.add(ValorDinheiro::novo(p.total));
                });
                row.col(|ui| {
                    ui.add(Rotulo::campo(p.estado.rotulo()));
                });
            });
    if let Some(i) = clicada {
        estado.abrir_pedido(motor, sessao, i);
    }
}

fn dialogo_novo(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaVendas,
) {
    let ops_cli: Vec<(Id, String)> = estado
        .clientes
        .iter()
        .map(|c| (c.pessoa, c.nome.clone()))
        .collect();
    let ops_tab: Vec<(Id, String)> = estado
        .tabelas
        .iter()
        .map(|t| (t.id, t.nome.clone()))
        .collect();
    let ops_loc: Vec<(Id, String)> = estado
        .locais
        .iter()
        .map(|l| (l.id, l.nome.clone()))
        .collect();
    let sem_tabela = estado.tabelas.is_empty();

    let fechar = Dialogo::nova("Novo pedido").largura(620.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            SeletorOpcao::novo("Cliente", &mut estado.novo_cliente)
                .opcoes(ops_cli.clone())
                .placeholder("Buscar cliente…")
                .mostrar(ui);
            ui.add_space(Espaco::E12);
            ui.columns(2, |c| {
                SeletorOpcao::novo("Tabela de preço", &mut estado.novo_tabela)
                    .opcoes(ops_tab.clone())
                    .mostrar(&mut c[0]);
                SeletorOpcao::novo("Local de expedição", &mut estado.novo_local)
                    .opcoes(ops_loc.clone())
                    .mostrar(&mut c[1]);
            });
            if sem_tabela {
                ui.add_space(Espaco::E8);
                ui.add(
                    Rotulo::interface(
                        "Nenhuma tabela de preço — crie uma em \"Tabelas de preço\" antes.",
                    )
                    .quebravel()
                    .cor(ui.cores().atencao),
                );
            }
        },
        |ui, estado| {
            let pronto = estado.novo_cliente.is_some()
                && estado.novo_tabela.is_some()
                && estado.novo_local.is_some();
            if ui
                .add(Botao::primario("Criar rascunho").habilitado(pronto))
                .clicked()
                && pronto
            {
                criar_pedido(ui.ctx(), motor, sessao, estado);
            }
            if ui.add(Botao::secundario("Cancelar")).clicked() {
                estado.dlg = Dlg::Fechado;
            }
        },
    );
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

fn criar_pedido(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaVendas,
) {
    let (Some(cliente), Some(tabela), Some(local)) =
        (estado.novo_cliente, estado.novo_tabela, estado.novo_local)
    else {
        return;
    };
    match motor.executar(
        sessao,
        "vendas.criar_pedido.v1",
        &CriarPedido {
            cliente,
            vendedor: sessao.usuario(),
            // Não há entidade de condição de pagamento no domínio ainda; o campo é só
            // armazenado. `Id::NULO` mantém o pedido válido.
            condicao_pagamento: Id::NULO,
            tabela_preco: tabela,
            local_expedicao: local,
        },
    ) {
        Ok(r) => {
            let r: PedidoCriado = r;
            estado.carregar(motor, sessao);
            if let Some(i) = estado.pedidos.iter().position(|p| p.pedido == r.pedido) {
                estado.abrir_pedido(motor, sessao, i);
            } else {
                estado.dlg = Dlg::Fechado;
            }
            notificar(ctx, Notificacao::sucesso("Pedido criado"));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

fn dialogo_ver(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaVendas,
    i: usize,
) {
    let Some(p) = estado.pedidos.get(i).cloned() else {
        estado.dlg = Dlg::Fechado;
        return;
    };
    let nome = estado.nome(p.cliente);

    let fechar = Dialogo::nova(format!("Pedido · {nome}"))
        .largura(680.0)
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                ui.columns(2, |c| {
                    kv(&mut c[0], "Data", &p.data.to_string());
                    kv(&mut c[1], "Estado", p.estado.rotulo());
                });
                ui.add_space(Espaco::E8);

                ui.add(Rotulo::titulo_secao("Itens"));
                ui.add_space(Espaco::E4);
                if estado.itens_ped.is_empty() {
                    ui.add(Rotulo::campo("Nenhum item ainda."));
                }
                for it in &estado.itens_ped {
                    ui.horizontal(|ui| {
                        ui.add(Rotulo::interface(format!(
                            "{} × {}",
                            it.quantidade,
                            estado.nome_produto(it.produto)
                        )));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(ValorDinheiro::novo(it.total_item));
                        });
                    });
                }

                if matches!(p.estado, EstadoPedido::Rascunho) {
                    ui.add_space(Espaco::E12);
                    let ops: Vec<(Id, String)> = estado
                        .produtos
                        .iter()
                        .map(|pr| (pr.produto, pr.nome.clone()))
                        .collect();
                    ui.columns(2, |c| {
                        SeletorOpcao::novo("Produto", &mut estado.item_produto)
                            .opcoes(ops.clone())
                            .placeholder("Buscar produto…")
                            .mostrar(&mut c[0]);
                        c[1].add(Campo::novo("Quantidade", &mut estado.item_qtd).marcador("1"));
                    });
                    ui.add_space(Espaco::E4);
                    if ui.add(Botao::secundario("+ Adicionar item")).clicked() {
                        adicionar_item(ui.ctx(), motor, sessao, estado, p.pedido);
                    }
                }

                ui.add_space(Espaco::E12);
                ui.separator();
                ui.add_space(Espaco::E12);
                acoes(ui, motor, sessao, estado, &p);
            },
            |ui, estado| {
                if ui.add(Botao::secundario("Fechar")).clicked() {
                    estado.dlg = Dlg::Fechado;
                }
            },
        );
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

fn adicionar_item(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaVendas,
    pedido: Id,
) {
    let Some(produto) = estado.item_produto else {
        notificar(ctx, Notificacao::aviso("Escolha o produto."));
        return;
    };
    let Ok(quantidade) = estado.item_qtd.parse() else {
        notificar(ctx, Notificacao::aviso("Quantidade inválida."));
        return;
    };
    match motor.executar(
        sessao,
        "vendas.adicionar_item_pedido.v1",
        &AdicionarItemPedido {
            pedido,
            produto,
            variacao: None,
            quantidade,
            desconto_percentual: Percentual::ZERO,
        },
    ) {
        Ok(_) => {
            estado.item_produto = None;
            estado.item_qtd.clear();
            estado.recarregar_itens(motor, sessao, pedido);
            estado.carregar(motor, sessao);
            notificar(ctx, Notificacao::sucesso("Item adicionado"));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

fn acoes(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaVendas,
    p: &ItemPedido,
) {
    match p.estado {
        EstadoPedido::Rascunho => {
            ui.horizontal(|ui| {
                let tem_item = !estado.itens_ped.is_empty();
                if ui
                    .add(Botao::primario("Confirmar pedido").habilitado(tem_item))
                    .clicked()
                    && tem_item
                {
                    aplicar(
                        ui.ctx(),
                        motor,
                        sessao,
                        estado,
                        "vendas.confirmar_pedido.v1",
                        &ConfirmarPedido { pedido: p.pedido },
                        "Pedido confirmado",
                    );
                }
                if ui.add(Botao::destrutivo("Cancelar pedido")).clicked() {
                    aplicar(
                        ui.ctx(),
                        motor,
                        sessao,
                        estado,
                        "vendas.cancelar_pedido.v1",
                        &CancelarPedido { pedido: p.pedido },
                        "Pedido cancelado",
                    );
                }
            });
        }
        EstadoPedido::Confirmado => {
            ui.horizontal(|ui| {
                if ui.add(Botao::primario("Faturar à vista")).clicked() {
                    aplicar(
                        ui.ctx(),
                        motor,
                        sessao,
                        estado,
                        "vendas.faturar_pedido.v1",
                        &FaturarPedido {
                            pedido: p.pedido,
                            a_vista: true,
                        },
                        "Pedido faturado — NFC-e emitida",
                    );
                }
                if ui.add(Botao::secundario("Faturar a prazo")).clicked() {
                    aplicar(
                        ui.ctx(),
                        motor,
                        sessao,
                        estado,
                        "vendas.faturar_pedido.v1",
                        &FaturarPedido {
                            pedido: p.pedido,
                            a_vista: false,
                        },
                        "Pedido faturado a prazo",
                    );
                }
                if ui.add(Botao::destrutivo("Cancelar")).clicked() {
                    aplicar(
                        ui.ctx(),
                        motor,
                        sessao,
                        estado,
                        "vendas.cancelar_pedido.v1",
                        &CancelarPedido { pedido: p.pedido },
                        "Pedido cancelado",
                    );
                }
            });
        }
        EstadoPedido::Faturado
        | EstadoPedido::EmEntrega
        | EstadoPedido::Concluido
        | EstadoPedido::Cancelado => {
            ui.add(Rotulo::campo("Sem ações disponíveis neste estado."));
        }
    }
}

fn dialogo_tabelas(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaVendas,
) {
    let ops_prod: Vec<(Id, String)> = estado
        .produtos
        .iter()
        .map(|p| (p.produto, p.nome.clone()))
        .collect();

    let fechar = Dialogo::nova("Tabelas de preço").largura(700.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            ui.add(Rotulo::titulo_secao("Tabelas"));
            ui.add_space(Espaco::E4);
            for t in estado.tabelas.clone() {
                let sel = estado.tab_sel == Some(t.id);
                if cardeal_ui::atoms::superficie_clicavel(
                    ui,
                    sel,
                    cardeal_ui::atoms::altura_navegacao(),
                    |ui| {
                        ui.add(Rotulo::interface(t.nome.clone()));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(Rotulo::campo(format!("{:?}", t.tipo)));
                        });
                    },
                )
                .clicked()
                {
                    estado.tab_sel = Some(t.id);
                    estado.carregar_regras(motor, sessao, t.id);
                }
            }

            ui.add_space(Espaco::E8);
            ui.horizontal(|ui| {
                ui.add(Campo::novo("Nova tabela", &mut estado.tab_nome).marcador("ex.: Balcão"));
                if ui.add(Botao::secundario("Criar")).clicked() {
                    criar_tabela(ui.ctx(), motor, sessao, estado);
                }
            });

            if let Some(tab) = estado.tab_sel {
                ui.add_space(Espaco::E16);
                ui.separator();
                ui.add_space(Espaco::E12);
                ui.add(Rotulo::titulo_secao("Regras de preço"));
                ui.add_space(Espaco::E4);
                if estado.regras.is_empty() {
                    ui.add(Rotulo::campo(
                        "Nenhuma regra — os itens não terão preço sem isto.",
                    ));
                }
                for r in estado.regras.clone() {
                    let alvo = match r.alvo {
                        mod_vendas::AlvoRegra::Produto(p) => estado.nome_produto(p),
                        mod_vendas::AlvoRegra::Grupo(_) => "grupo".to_owned(),
                    };
                    ui.horizontal(|ui| {
                        ui.add(Rotulo::interface(alvo));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(Rotulo::interface(r.preco.formatar_com_simbolo()));
                        });
                    });
                }
                ui.add_space(Espaco::E8);
                ui.columns(2, |c| {
                    SeletorOpcao::novo("Produto", &mut estado.regra_produto)
                        .opcoes(ops_prod.clone())
                        .mostrar(&mut c[0]);
                    c[1].add(Campo::novo("Preço", &mut estado.regra_preco).marcador("0,00"));
                });
                ui.add_space(Espaco::E4);
                if ui.add(Botao::secundario("+ Adicionar regra")).clicked() {
                    criar_regra(ui.ctx(), motor, sessao, estado, tab);
                }
            }
        },
        |ui, estado| {
            if ui.add(Botao::secundario("Fechar")).clicked() {
                estado.dlg = Dlg::Fechado;
            }
        },
    );
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

fn criar_tabela(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaVendas,
) {
    if estado.tab_nome.trim().is_empty() {
        notificar(ctx, Notificacao::aviso("Dê um nome à tabela."));
        return;
    }
    let hoje = cardeal_kernel::Data::hoje(cardeal_kernel::Fuso::BRASILIA);
    match motor.executar(
        sessao,
        "vendas.criar_tabela_preco.v1",
        &CriarTabelaPreco {
            nome: estado.tab_nome.clone(),
            tipo: TipoTabela::Venda,
            vigente_de: hoje,
            vigente_ate: None,
        },
    ) {
        Ok(_) => {
            estado.tab_nome.clear();
            estado.carregar(motor, sessao);
            estado.dlg = Dlg::Tabelas;
            notificar(ctx, Notificacao::sucesso("Tabela de preço criada"));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

fn criar_regra(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaVendas,
    tabela: Id,
) {
    let Some(alvo_produto) = estado.regra_produto else {
        notificar(ctx, Notificacao::aviso("Escolha o produto da regra."));
        return;
    };
    let Ok(preco) = estado.regra_preco.parse() else {
        notificar(ctx, Notificacao::aviso("Preço inválido — use 0,00."));
        return;
    };
    match motor.executar(
        sessao,
        "vendas.criar_regra_preco.v1",
        &CriarRegraPreco {
            tabela_preco: tabela,
            alvo_produto: Some(alvo_produto),
            alvo_grupo: None,
            quantidade_minima: None,
            preco,
            periodo_de: None,
            periodo_ate: None,
        },
    ) {
        Ok(_) => {
            estado.regra_produto = None;
            estado.regra_preco.clear();
            estado.carregar_regras(motor, sessao, tabela);
            estado.dlg = Dlg::Tabelas;
            notificar(ctx, Notificacao::sucesso("Regra de preço adicionada"));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

fn aplicar<C: cardeal_modkit::Comando + serde::Serialize>(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaVendas,
    nome: &str,
    comando: &C,
    sucesso: &str,
) where
    C::Saida: serde::de::DeserializeOwned,
{
    match motor.executar(sessao, nome, comando) {
        Ok(_) => {
            estado.dlg = Dlg::Fechado;
            estado.carregar(motor, sessao);
            notificar(ctx, Notificacao::sucesso(sucesso.to_owned()));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

fn kv(ui: &mut egui::Ui, chave: &str, valor: &str) {
    ui.add(Rotulo::campo(chave));
    ui.add(Rotulo::interface(if valor.trim().is_empty() {
        "—"
    } else {
        valor
    }));
    ui.add_space(Espaco::E8);
}
