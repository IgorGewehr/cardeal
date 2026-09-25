//! Peça sob encomenda na tela: a situação de cada peça do orçamento (aplicada, em estoque,
//! encomendada, a comprar), os diálogos "Encomendar" e "Chegou" e a aba "Peças a comprar"
//! (a lista de compras de todas as OS, `os.pecas_aguardando_estoque.v1`).

use super::*;
use mod_os::{
    ChegadaRegistrada, Encomenda, EncomendarPeca, ItemAguardandoEstoque, ItemPeca, PagamentoDaPeca,
    RegistrarChegadaDaPeca,
};

/// O formulário "Encomendar peça".
#[derive(Debug, Clone)]
pub(super) struct FormEncomenda {
    pub(super) ordem: Id,
    pub(super) item: Id,
    pub(super) rotulo: String,
    pub(super) fornecedor: String,
    pub(super) custo: String,
    pub(super) previsao: String,
    /// Voltar para a gaveta da OS depois (aberto de lá) ou ficar na lista de compras.
    pub(super) volta_ao_detalhe: bool,
}

/// O formulário "A peça chegou".
#[derive(Debug, Clone)]
pub(super) struct FormChegada {
    pub(super) ordem: Id,
    pub(super) item: Id,
    pub(super) rotulo: String,
    pub(super) custo: String,
    pub(super) fornecedor: String,
    pub(super) pagamento: crate::pagamento::EstadoPagamento,
    pub(super) volta_ao_detalhe: bool,
}

/// O nome do produto no catálogo carregado.
pub(super) fn nome_do_produto(produtos: &[ItemProdutoComSaldo], produto: Id) -> String {
    produtos
        .iter()
        .find(|p| p.produto == produto)
        .map_or_else(|| "Peça".to_owned(), |p| p.nome.clone())
}

/// A etiqueta de situação de uma peça do orçamento.
pub(super) fn etiqueta_da_peca(item: &ItemPeca, produtos: &[ItemProdutoComSaldo]) -> Etiqueta {
    if item.aplicada {
        return Etiqueta::positiva("aplicada");
    }
    if let Some(e) = &item.encomenda {
        let quando = e
            .previsao_chegada
            .map(|d| format!(" · chega {}", d.formatar_curta()))
            .unwrap_or_default();
        return Etiqueta::nova(format!("encomendada{quando}"), Tom::Info);
    }
    let disponivel = produtos
        .iter()
        .find(|p| p.produto == item.produto)
        .map_or(Quantidade::ZERO, |p| p.disponivel);
    if disponivel >= item.quantidade {
        Etiqueta::neutra("em estoque")
    } else {
        Etiqueta::atencao("a comprar")
    }
}

/// Se a peça ainda não tem saldo suficiente (o "Encomendar" faz sentido).
pub(super) fn falta_no_estoque(item: &ItemPeca, produtos: &[ItemProdutoComSaldo]) -> bool {
    produtos
        .iter()
        .find(|p| p.produto == item.produto)
        .is_none_or(|p| p.disponivel < item.quantidade)
}

impl FormEncomenda {
    pub(super) fn novo(
        ordem: Id,
        item: Id,
        e: Option<&Encomenda>,
        rotulo: String,
        volta: bool,
    ) -> Self {
        Self {
            ordem,
            item,
            rotulo,
            fornecedor: e.map(|e| e.fornecedor.clone()).unwrap_or_default(),
            custo: e
                .and_then(|e| e.custo_previsto)
                .map(|c| c.formatar())
                .unwrap_or_default(),
            previsao: e
                .and_then(|e| e.previsao_chegada)
                .map(|d| d.to_string())
                .unwrap_or_default(),
            volta_ao_detalhe: volta,
        }
    }
}

impl FormChegada {
    pub(super) fn novo(
        ordem: Id,
        item: Id,
        e: Option<&Encomenda>,
        rotulo: String,
        volta: bool,
    ) -> Self {
        Self {
            ordem,
            item,
            rotulo,
            custo: e
                .and_then(|e| e.custo_previsto)
                .map(|c| c.formatar())
                .unwrap_or_default(),
            fornecedor: e.map(|e| e.fornecedor.clone()).unwrap_or_default(),
            pagamento: crate::pagamento::EstadoPagamento::novo(MeioPagamento::Pix),
            volta_ao_detalhe: volta,
        }
    }
}

/// Depois de encomendar ou registrar a chegada: volta à gaveta da OS ou fica na lista.
fn depois_de_salvar(
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
    ordem: Id,
    volta: bool,
) {
    estado.carregar_produtos(motor, sessao);
    estado.recarregar_lista(motor, sessao);
    if volta {
        estado.abrir_detalhe(motor, sessao, ordem);
    } else {
        estado.dlg = Dlg::Fechado;
    }
}

pub(super) fn dialogo_encomendar(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
) {
    let Dlg::Encomendar(f) = &estado.dlg else {
        return;
    };
    let titulo = format!("Encomendar · {}", f.rotulo);
    let volta = f.volta_ao_detalhe;
    let ordem = f.ordem;
    let fechar = Dialogo::nova(titulo)
        .descricao("Fica na lista \"Peças a comprar\" até você registrar a chegada.")
        .medio()
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                let Dlg::Encomendar(f) = &mut estado.dlg else {
                    return;
                };
                ui.add(
                    Campo::novo("Fornecedor", &mut f.fornecedor)
                        .marcador("loja, site, representante"),
                );
                ui.add_space(Espaco::E8);
                ui.columns(2, |c| {
                    c[0].add(
                        Campo::novo("Custo combinado (opcional)", &mut f.custo).marcador("0,00"),
                    );
                    c[1].add(
                        Campo::novo("Previsão de chegada (opcional)", &mut f.previsao)
                            .mascara(Mascara::Data),
                    );
                });
            },
            |ui, estado| {
                if ui.add(Botao::primario("Encomendar")).clicked() {
                    encomendar(ui.ctx(), motor, sessao, estado);
                }
                if ui.add(Botao::secundario("Cancelar")).clicked() {
                    voltar(motor, sessao, estado, ordem, volta);
                }
            },
        );
    if fechar {
        voltar(motor, sessao, estado, ordem, volta);
    }
}

fn voltar(
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
    ordem: Id,
    volta: bool,
) {
    if volta {
        estado.abrir_detalhe(motor, sessao, ordem);
    } else {
        estado.dlg = Dlg::Fechado;
    }
}

fn encomendar(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
) {
    let Dlg::Encomendar(f) = &estado.dlg else {
        return;
    };
    let custo_previsto = match f.custo.trim() {
        "" => None,
        t => match t.parse::<Preco>() {
            Ok(p) => Some(p),
            Err(_) => {
                notificar(ctx, Notificacao::aviso("Custo: use o formato 0,00."));
                return;
            }
        },
    };
    let previsao_chegada = match f.previsao.trim() {
        "" => None,
        t => match t.parse::<Data>() {
            Ok(d) => Some(d),
            Err(_) => {
                notificar(ctx, Notificacao::aviso("Previsão: use dd/mm/aaaa."));
                return;
            }
        },
    };
    let (ordem, volta) = (f.ordem, f.volta_ao_detalhe);
    match motor.executar(
        sessao,
        "os.encomendar_peca.v1",
        &EncomendarPeca {
            ordem_servico: f.ordem,
            item_peca: f.item,
            fornecedor: f.fornecedor.clone(),
            custo_previsto,
            previsao_chegada,
        },
    ) {
        Ok(()) => {
            depois_de_salvar(motor, sessao, estado, ordem, volta);
            notificar(ctx, Notificacao::sucesso("Peça encomendada"));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

pub(super) fn dialogo_chegada(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
) {
    let Dlg::Chegou(f) = &estado.dlg else {
        return;
    };
    let titulo = format!("Chegou · {}", f.rotulo);
    let volta = f.volta_ao_detalhe;
    let ordem = f.ordem;
    let fechar = Dialogo::nova(titulo)
        .descricao(
            "Entra no estoque com o custo real, sai para esta OS e vira conta a pagar (ou já \
             paga) — tudo de uma vez, sem nota de compra.",
        )
        .medio()
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                let Dlg::Chegou(f) = &mut estado.dlg else {
                    return;
                };
                ui.columns(2, |c| {
                    c[0].add(Campo::novo("Custo por unidade", &mut f.custo).marcador("0,00"));
                    c[1].add(
                        Campo::novo("Fornecedor (opcional)", &mut f.fornecedor)
                            .marcador("de quem veio"),
                    );
                });
                ui.add_space(Espaco::E12);
                f.pagamento.mostrar(
                    ui,
                    motor,
                    sessao,
                    "chegada-peca",
                    crate::pagamento::Prazo::Unico,
                );
            },
            |ui, estado| {
                if ui.add(Botao::primario("Registrar chegada")).clicked() {
                    registrar_chegada(ui.ctx(), motor, sessao, estado);
                }
                if ui.add(Botao::secundario("Cancelar")).clicked() {
                    voltar(motor, sessao, estado, ordem, volta);
                }
            },
        );
    if fechar {
        voltar(motor, sessao, estado, ordem, volta);
    }
}

pub(super) fn registrar_chegada(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
) {
    let Dlg::Chegou(f) = &estado.dlg else {
        return;
    };
    let Ok(custo_unitario) = f.custo.parse::<Preco>() else {
        notificar(
            ctx,
            Notificacao::aviso("Informe o custo por unidade (0,00)."),
        );
        return;
    };
    let pagamento = if f.pagamento.condicao.a_prazo {
        match f.pagamento.vencimento_validado() {
            Ok(vencimento) => PagamentoDaPeca::APrazo { vencimento },
            Err(msg) => {
                notificar(ctx, Notificacao::aviso(msg));
                return;
            }
        }
    } else {
        match f.pagamento.meio_validado() {
            Ok(meio_pagamento) => PagamentoDaPeca::PagoAgora {
                meio_pagamento,
                conta_origem: f.pagamento.conta_destino(),
            },
            Err(msg) => {
                notificar(ctx, Notificacao::aviso(msg));
                return;
            }
        }
    };
    let (ordem, volta) = (f.ordem, f.volta_ao_detalhe);
    match motor.executar(
        sessao,
        "os.registrar_chegada_da_peca.v1",
        &RegistrarChegadaDaPeca {
            ordem_servico: f.ordem,
            item_peca: f.item,
            custo_unitario,
            fornecedor: f.fornecedor.clone(),
            pagamento,
        },
    ) {
        Ok(r) => {
            let r: ChegadaRegistrada = r;
            depois_de_salvar(motor, sessao, estado, ordem, volta);
            notificar(
                ctx,
                Notificacao::sucesso(format!(
                    "Peça aplicada na OS — {} {}",
                    r.custo_total.formatar_com_simbolo(),
                    if r.pago { "pagos" } else { "em Contas a Pagar" }
                )),
            );
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

/// A aba "Peças a comprar": o que falta comprar ou está a caminho, de todas as OS.
pub(super) fn aba_compras(ui: &mut egui::Ui, estado: &mut EstadoTelaOs) {
    let pecas = estado.pecas_a_comprar.clone();
    let a_encomendar = pecas.iter().filter(|p| p.encomenda.is_none()).count();
    let a_caminho = pecas.len() - a_encomendar;
    let os_paradas = {
        let mut ids: Vec<Id> = pecas.iter().map(|p| p.ordem_servico).collect();
        ids.sort_unstable();
        ids.dedup();
        ids.len()
    };
    FaixaKpi::nova(vec![
        CartaoKpi::contagem("A encomendar", a_encomendar).variacao("sem saldo e sem pedido"),
        CartaoKpi::contagem("A caminho", a_caminho).variacao("encomendadas"),
        CartaoKpi::contagem("OS esperando peça", os_paradas),
    ])
    .mostrar(ui);

    if pecas.is_empty() {
        EstadoVazio::novo(Icone::Ferramenta, "Nenhuma peça para comprar agora.").mostrar(ui);
        return;
    }
    BarraFiltros::nova(&mut estado.busca_compras)
        .marcador("Buscar por peça, fornecedor, OS ou cliente")
        .mostrar(ui);
    let termo = estado.busca_compras.trim().to_owned();
    let linhas: Vec<&ItemAguardandoEstoque> = pecas
        .iter()
        .filter(|p| {
            termo.is_empty()
                || cardeal_kernel::texto::casa_por_palavras(
                    &format!(
                        "{} {} {} {} {}",
                        nome_do_produto(&estado.produtos, p.produto),
                        p.encomenda.as_ref().map_or("", |e| e.fornecedor.as_str()),
                        p.numero,
                        p.equipamento,
                        nome_cliente(&estado.clientes, p.cliente)
                    ),
                    &termo,
                )
        })
        .collect();
    let colunas = vec![
        ColunaGrade::nova("Peça"),
        ColunaGrade::nova("OS").largura(240.0),
        ColunaGrade::nova("Qtd").largura(60.0).numero(),
        ColunaGrade::nova("Situação").largura(260.0),
        ColunaGrade::nova("Ações").largura(200.0),
    ];
    let mut acao: Option<(bool, usize)> = None;
    Grade::nova(colunas)
        .com_acoes()
        .vazio("Nenhuma peça para essa busca.")
        .mostrar(ui, linhas.len(), |i, row| {
            let p = linhas[i];
            row.col(|ui| {
                ui.add(Rotulo::interface(nome_do_produto(
                    &estado.produtos,
                    p.produto,
                )));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(format!(
                    "#{} · {} · {}",
                    p.numero,
                    p.equipamento,
                    nome_cliente(&estado.clientes, p.cliente)
                )));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(p.quantidade_necessaria.to_string()));
            });
            row.col(|ui| match &p.encomenda {
                Some(e) => {
                    let quando = e
                        .previsao_chegada
                        .map(|d| format!(" · chega {}", d.formatar_curta()))
                        .unwrap_or_default();
                    ui.add(Etiqueta::nova(
                        format!("{}{quando}", e.fornecedor),
                        Tom::Info,
                    ));
                }
                None => {
                    ui.add(Etiqueta::atencao(format!(
                        "a encomendar · saldo {}",
                        p.saldo_disponivel
                    )));
                }
            });
            row.col(|ui| {
                ui.horizontal(|ui| {
                    if ui.add(Botao::primario("Chegou").pequeno()).clicked() {
                        acao = Some((true, i));
                    }
                    let rotulo = if p.encomenda.is_some() {
                        "Editar pedido"
                    } else {
                        "Encomendar"
                    };
                    if ui.add(Botao::fantasma(rotulo).pequeno()).clicked() {
                        acao = Some((false, i));
                    }
                });
            });
        });
    if let Some((chegou, i)) = acao {
        let p = linhas[i];
        let rotulo = format!(
            "{} (OS #{})",
            nome_do_produto(&estado.produtos, p.produto),
            p.numero
        );
        estado.dlg = if chegou {
            Dlg::Chegou(Box::new(FormChegada::novo(
                p.ordem_servico,
                p.item_peca,
                p.encomenda.as_ref(),
                rotulo,
                false,
            )))
        } else {
            Dlg::Encomendar(Box::new(FormEncomenda::novo(
                p.ordem_servico,
                p.item_peca,
                p.encomenda.as_ref(),
                rotulo,
                false,
            )))
        };
    }
}

/// O custo das peças da OS: o real das aplicadas e, para as que faltam, o combinado na
/// encomenda ou o custo médio do estoque. `true` = há estimativa na conta.
pub(super) fn custo_das_pecas(
    detalhe: &DetalheOrdem,
    produtos: &[ItemProdutoComSaldo],
) -> (Dinheiro, bool) {
    let mut estimado = false;
    let total =
        detalhe
            .itens_peca
            .iter()
            .filter(|i| !i.estornada)
            .fold(Dinheiro::ZERO, |acc, i| {
                let unitario = if i.aplicada {
                    i.custo_unitario
                } else {
                    estimado = true;
                    i.encomenda
                        .as_ref()
                        .and_then(|e| e.custo_previsto)
                        .or_else(|| {
                            produtos
                                .iter()
                                .find(|p| p.produto == i.produto)
                                .map(|p| p.custo_medio)
                        })
                        .unwrap_or(Preco::ZERO)
                };
                acc + Dinheiro::de_total(
                    i.quantidade,
                    unitario,
                    cardeal_kernel::Arredondamento::MeioAcima,
                )
            });
    (total, estimado)
}
