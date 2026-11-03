//! A aba **Pedidos** de Compras: os pedidos de compra (`mod_compras::pedido`) — a lista, o
//! formulário (que puxa do radar de reposição o que está abaixo do ponto de pedido, descontado
//! o que já vem a caminho, com o último preço pago) e o pedido aberto: aprovar, editar, mandar
//! ao fornecedor (texto para o WhatsApp ou PDF), cancelar ou encerrar o saldo.

use std::collections::HashMap;

use cardeal_cliente::{IdentidadeVisual, Motor, Sessao};
use cardeal_kernel::{Data, Dinheiro, Fuso, Id, Instante, Preco, Quantidade};
use cardeal_modkit::Icone;
use cardeal_pdf::{gerar_pedido_compra, IdentidadeEmpresa, ItemPdf, PedidoCompraPdf};
use cardeal_ui::atoms::{Botao, Etiqueta, Rotulo, Tom, ValorDinheiro};
use cardeal_ui::molecules::{
    dado, BarraFiltros, Campo, CartaoKpi, EstadoVazio, Mascara, OpcaoBusca, SeletorBusca,
    SeletorOpcao,
};
use cardeal_ui::organisms::{
    notificar, ColunaGrade, Dialogo, FaixaKpi, Grade, Notificacao, Painel,
};
use cardeal_ui::tokens::{Espaco, TemaUi};
use eframe::egui;
use mod_clientes::ItemPessoa;
use mod_compras::pedido::ResumoPedido;
use mod_compras::{
    ACaminho, AprovarPedidoCompra, CancelarPedidoCompra, EncerrarPedidoCompra, EstadoPedido,
    ItemDoPedido, PedidoCompra, PedidoDeCompra, PedidosDeCompra, SalvarPedidoCompra,
    UltimosPrecosPagos,
};
use mod_estoque::{ItemAbaixoDoPontoPedido, ItemProdutoComSaldo, ProdutosAbaixoDoPontoPedido};

/// Uma linha do formulário.
#[derive(Default, Clone)]
struct LinhaPedido {
    produto: Option<Id>,
    busca: String,
    qtd: String,
    preco: String,
    /// O produto para o qual o preço já foi sugerido (não sugere de novo por cima do digitado).
    sugerido_para: Option<Id>,
}

#[derive(Default)]
struct FormPedido {
    editar: Option<Id>,
    fornecedor: Option<Id>,
    previsao: String,
    condicao: String,
    frete: String,
    observacao: String,
    linhas: Vec<LinhaPedido>,
}

impl FormPedido {
    fn novo() -> Self {
        Self {
            frete: "0,00".to_owned(),
            linhas: vec![LinhaPedido::default()],
            ..Self::default()
        }
    }

    fn de_pedido(p: &PedidoCompra) -> Self {
        Self {
            editar: Some(p.id),
            fornecedor: Some(p.fornecedor),
            previsao: p.previsao_entrega.map(Data::formatar).unwrap_or_default(),
            condicao: p.condicao_pagamento.clone(),
            frete: p.frete.formatar(),
            observacao: p.observacao.clone(),
            linhas: p
                .itens
                .iter()
                .map(|i| LinhaPedido {
                    produto: Some(i.produto),
                    busca: String::new(),
                    qtd: i.quantidade.formatar(0),
                    preco: i.preco_unitario.formatar(),
                    sugerido_para: Some(i.produto),
                })
                .collect(),
        }
    }

    /// O comando, ou o que falta.
    fn comando(&self) -> Result<SalvarPedidoCompra, String> {
        let fornecedor = self.fornecedor.ok_or("Escolha o fornecedor.")?;
        let previsao = match self.previsao.trim() {
            "" => None,
            t => Some(
                t.parse::<Data>()
                    .map_err(|_| "A entrega combinada não é uma data (dd/mm/aaaa).")?,
            ),
        };
        let frete = match self.frete.trim() {
            "" => Dinheiro::ZERO,
            t => Dinheiro::de_str(t).map_err(|_| "O frete não é um valor.")?,
        };
        let mut itens = Vec::new();
        for (n, l) in self.linhas.iter().enumerate() {
            let Some(produto) = l.produto else {
                if l.qtd.trim().is_empty() || l.qtd.trim() == "0" {
                    continue;
                }
                return Err(format!("Linha {}: escolha o produto.", n + 1));
            };
            let quantidade = l
                .qtd
                .trim()
                .parse::<Quantidade>()
                .map_err(|_| format!("Linha {}: a quantidade não é um número.", n + 1))?;
            let preco_unitario = l
                .preco
                .trim()
                .parse::<Preco>()
                .map_err(|_| format!("Linha {}: informe o preço combinado.", n + 1))?;
            itens.push(ItemDoPedido {
                produto,
                quantidade,
                preco_unitario,
            });
        }
        Ok(SalvarPedidoCompra {
            pedido: self.editar,
            fornecedor,
            previsao_entrega: previsao,
            condicao_pagamento: self.condicao.clone(),
            frete,
            observacao: self.observacao.clone(),
            itens,
        })
    }

    fn total(&self) -> Dinheiro {
        let itens: Dinheiro = self
            .linhas
            .iter()
            .filter_map(|l| {
                let q = l.qtd.trim().parse::<Quantidade>().ok()?;
                let p = l.preco.trim().parse::<Preco>().ok()?;
                Some(Dinheiro::de_total(
                    q,
                    p,
                    cardeal_kernel::Arredondamento::MeioAcima,
                ))
            })
            .sum();
        itens + Dinheiro::de_str(self.frete.trim()).unwrap_or(Dinheiro::ZERO)
    }
}

#[derive(Default)]
enum Dlg {
    #[default]
    Fechado,
    Form(Box<FormPedido>),
    Ver(Box<PedidoCompra>),
    Motivo {
        pedido: Box<PedidoCompra>,
        encerrar: bool,
        motivo: String,
    },
}

/// Estado da aba.
#[derive(Default)]
pub struct EstadoPedidos {
    lista: Vec<ResumoPedido>,
    /// Mostrar também os terminados (recebidos, encerrados, cancelados).
    todos: bool,
    busca: String,
    fornecedores: Vec<ItemPessoa>,
    erro: Option<String>,
    dlg: Dlg,
    identidade: Option<IdentidadeVisual>,
}

impl EstadoPedidos {
    /// Recarrega a lista. `fornecedores` vem da tela (o mesmo catálogo das notas).
    pub fn carregar(&mut self, motor: &Motor, sessao: &Sessao, fornecedores: Vec<ItemPessoa>) {
        self.fornecedores = fornecedores;
        match motor.consultar(
            sessao,
            "compras.pedidos.v1",
            &PedidosDeCompra {
                so_em_andamento: !self.todos,
                fornecedor: None,
            },
        ) {
            Ok(v) => {
                self.lista = v;
                self.erro = None;
            }
            Err(e) => self.erro = Some(e.mensagem),
        }
        if self.identidade.is_none() {
            self.identidade = motor.identidade_visual().ok();
        }
    }

    fn recarregar(&mut self, motor: &Motor, sessao: &Sessao) {
        let f = std::mem::take(&mut self.fornecedores);
        self.carregar(motor, sessao, f);
    }

    fn abrir(&mut self, motor: &Motor, sessao: &Sessao, pedido: Id) {
        match motor.consultar(sessao, "compras.pedido.v1", &PedidoDeCompra { pedido }) {
            Ok(Some(p)) => self.dlg = Dlg::Ver(Box::new(p)),
            Ok(None) => self.erro = Some("Pedido não encontrado.".to_owned()),
            Err(e) => self.erro = Some(e.mensagem),
        }
    }

    fn telefone(&self, fornecedor: Id) -> Option<String> {
        self.fornecedores
            .iter()
            .find(|f| f.pessoa == fornecedor)
            .and_then(|f| f.telefone.clone())
    }
}

/// Abre o formulário de pedido novo (o botão do cabeçalho).
pub fn abrir_novo(estado: &mut EstadoPedidos) {
    estado.dlg = Dlg::Form(Box::new(FormPedido::novo()));
}

/// Abre o pedido (vindo de outra aba, como a cotação decidida).
pub fn abrir_pedido(estado: &mut EstadoPedidos, motor: &Motor, sessao: &Sessao, pedido: Id) {
    estado.abrir(motor, sessao, pedido);
}

/// O rótulo e o tom do estado.
pub fn etiqueta_estado(e: EstadoPedido, atrasado: bool) -> Etiqueta {
    let tom = match e {
        EstadoPedido::Rascunho => Tom::Neutro,
        EstadoPedido::Enviado | EstadoPedido::ParcialmenteRecebido if atrasado => Tom::Negativo,
        EstadoPedido::Enviado => Tom::Info,
        EstadoPedido::ParcialmenteRecebido => Tom::Atencao,
        EstadoPedido::Recebido => Tom::Positivo,
        EstadoPedido::Encerrado | EstadoPedido::Cancelado => Tom::Neutro,
    };
    let texto = if atrasado {
        format!("{} · atrasado", e.nome())
    } else {
        e.nome().to_owned()
    };
    Etiqueta::nova(texto, tom)
}

/// O miolo da aba.
pub fn corpo(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoPedidos) {
    if let Some(erro) = &estado.erro {
        ui.add(
            Rotulo::interface(erro.clone())
                .quebravel()
                .cor(ui.cores().negativo),
        );
        ui.add_space(Espaco::E12);
    }
    let hoje = Data::hoje(Fuso::BRASILIA);
    let aguardando: Vec<&ResumoPedido> = estado
        .lista
        .iter()
        .filter(|p| p.estado.aguardando())
        .collect();
    let a_receber: Dinheiro = aguardando.iter().map(|p| p.total - p.recebido).sum();
    let atrasados = aguardando.iter().filter(|p| p.atrasado(hoje)).count();
    let rascunhos = estado
        .lista
        .iter()
        .filter(|p| p.estado == EstadoPedido::Rascunho)
        .count();
    FaixaKpi::nova(vec![
        CartaoKpi::contagem("Aguardando entrega", aguardando.len())
            .variacao(a_receber.formatar_com_simbolo())
            .compacto()
            .tom(Tom::Info),
        CartaoKpi::contagem("Atrasados", atrasados)
            .variacao("passaram da entrega combinada")
            .compacto()
            .tom(if atrasados > 0 {
                Tom::Negativo
            } else {
                Tom::Neutro
            }),
        CartaoKpi::contagem("Rascunhos", rascunhos)
            .variacao("esperam aprovação")
            .compacto()
            .tom(Tom::Neutro),
    ])
    .mostrar(ui);
    ui.add_space(Espaco::E12);

    ui.horizontal(|ui| {
        let rot = if estado.todos {
            "Só os em andamento"
        } else {
            "Mostrar também os terminados"
        };
        if ui.add(Botao::fantasma(rot).pequeno()).clicked() {
            estado.todos = !estado.todos;
            estado.recarregar(motor, sessao);
        }
    });
    ui.add_space(Espaco::E8);

    if estado.lista.is_empty() {
        if estado.erro.is_none()
            && EstadoVazio::novo(Icone::Carrinho, "Nenhum pedido de compra em andamento.")
                .acao("Fazer um pedido")
                .mostrar(ui)
        {
            abrir_novo(estado);
        }
        return;
    }
    BarraFiltros::nova(&mut estado.busca)
        .marcador("Buscar por fornecedor ou número")
        .mostrar(ui);
    let termo = estado.busca.trim().to_lowercase();
    let indices: Vec<usize> = (0..estado.lista.len())
        .filter(|&i| {
            let p = &estado.lista[i];
            termo.is_empty()
                || p.fornecedor_nome.to_lowercase().contains(&termo)
                || p.numero.to_string().contains(&termo)
        })
        .collect();
    let colunas = vec![
        ColunaGrade::nova("Nº").largura(60.0).numero(),
        ColunaGrade::nova("Fornecedor"),
        ColunaGrade::nova("Data").largura(96.0),
        ColunaGrade::nova("Entrega").largura(96.0),
        ColunaGrade::nova("Total").largura(120.0).numero(),
        ColunaGrade::nova("Recebido").largura(120.0).numero(),
        ColunaGrade::nova("Situação").largura(190.0),
    ];
    let resposta = Grade::nova(colunas)
        .id_salt("pedidos")
        .selecionavel(None)
        .vazio("Nenhum pedido para essa busca.")
        .mostrar(ui, indices.len(), |i, row| {
            let p = &estado.lista[indices[i]];
            row.col(|ui| {
                ui.add(Rotulo::interface(format!("{:04}", p.numero)));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(p.fornecedor_nome.clone()));
            });
            row.col(|ui| {
                ui.add(Rotulo::campo(p.data.formatar_curta()));
            });
            row.col(|ui| {
                ui.add(Rotulo::campo(
                    p.previsao_entrega
                        .map_or_else(|| "—".to_owned(), Data::formatar_curta),
                ));
            });
            row.col(|ui| {
                ui.add(ValorDinheiro::novo(p.total));
            });
            row.col(|ui| {
                ui.add(ValorDinheiro::novo(p.recebido));
            });
            row.col(|ui| {
                ui.add(etiqueta_estado(p.estado, p.atrasado(hoje)));
            });
        });
    if let Some(i) = resposta.linha_clicada {
        let id = estado.lista[indices[i]].pedido;
        estado.abrir(motor, sessao, id);
    }
}

/// Os diálogos da aba.
pub fn dialogos(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    produtos: &[ItemProdutoComSaldo],
    estado: &mut EstadoPedidos,
) {
    match estado.dlg {
        Dlg::Fechado => {}
        Dlg::Form(_) => dialogo_form(ctx, motor, sessao, produtos, estado),
        Dlg::Ver(_) => dialogo_ver(ctx, motor, sessao, estado),
        Dlg::Motivo { .. } => dialogo_motivo(ctx, motor, sessao, estado),
    }
}

fn opcoes_produtos(produtos: &[ItemProdutoComSaldo]) -> Vec<OpcaoBusca<Id>> {
    produtos
        .iter()
        .map(|p| {
            OpcaoBusca::nova(p.produto, p.nome.clone())
                .subtitulo(format!("disponível {}", p.disponivel.formatar(0)))
        })
        .collect()
}

/// O último preço pago de cada produto.
fn ultimos_precos(motor: &Motor, sessao: &Sessao, produtos: Vec<Id>) -> HashMap<Id, Preco> {
    motor
        .consultar(
            sessao,
            "compras.ultimos_precos_pagos.v1",
            &UltimosPrecosPagos { produtos },
        )
        .unwrap_or_default()
        .into_iter()
        .map(|p| (p.produto, p.preco))
        .collect()
}

/// As linhas que o radar de reposição sugere: o que está abaixo do ponto de pedido, menos o que
/// já vem a caminho, com o último preço pago.
fn linhas_do_radar(motor: &Motor, sessao: &Sessao) -> Result<Vec<LinhaPedido>, String> {
    let radar: Vec<ItemAbaixoDoPontoPedido> = motor
        .consultar(
            sessao,
            "estoque.produtos_abaixo_do_ponto_pedido.v1",
            &ProdutosAbaixoDoPontoPedido,
        )
        .map_err(|e| e.mensagem)?;
    let a_caminho: HashMap<Id, Quantidade> = motor
        .consultar(sessao, "compras.a_caminho.v1", &ACaminho)
        .unwrap_or_default()
        .into_iter()
        .collect();
    let faltam: Vec<(Id, Quantidade)> = radar
        .iter()
        .filter_map(|r| {
            let vem = a_caminho
                .get(&r.produto)
                .copied()
                .unwrap_or(Quantidade::ZERO);
            let falta = r.ponto_pedido - r.disponivel - vem;
            falta.e_positiva().then_some((r.produto, falta))
        })
        .collect();
    let precos = ultimos_precos(motor, sessao, faltam.iter().map(|(p, _)| *p).collect());
    Ok(faltam
        .into_iter()
        .map(|(produto, q)| LinhaPedido {
            produto: Some(produto),
            busca: String::new(),
            qtd: q.formatar(0),
            preco: precos
                .get(&produto)
                .map(|p| p.formatar())
                .unwrap_or_default(),
            sugerido_para: Some(produto),
        })
        .collect())
}

fn dialogo_form(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    produtos: &[ItemProdutoComSaldo],
    estado: &mut EstadoPedidos,
) {
    let ops_forn: Vec<(Id, String)> = estado
        .fornecedores
        .iter()
        .map(|f| (f.pessoa, f.nome.clone()))
        .collect();
    let opcoes = opcoes_produtos(produtos);
    let nomes: HashMap<Id, String> = produtos
        .iter()
        .map(|p| (p.produto, p.nome.clone()))
        .collect();
    let titulo = match &estado.dlg {
        Dlg::Form(f) if f.editar.is_some() => "Editar pedido de compra",
        _ => "Novo pedido de compra",
    };
    let fechar = Dialogo::nova(titulo).largura(820.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            let Dlg::Form(f) = &mut estado.dlg else {
                return;
            };
            if f.editar.is_some() {
                ui.add(
                    Rotulo::apoio(
                        "Salvar a edição devolve o pedido ao rascunho: ele precisa ser aprovado \
                         de novo.",
                    )
                    .quebravel(),
                );
                ui.add_space(Espaco::E8);
            }
            SeletorOpcao::novo("Fornecedor", &mut f.fornecedor)
                .opcoes(ops_forn)
                .placeholder("Escolha o fornecedor…")
                .mostrar(ui);
            ui.add_space(Espaco::E8);
            ui.columns(3, |c| {
                c[0].add(
                    Campo::novo("Entrega combinada (opcional)", &mut f.previsao)
                        .mascara(Mascara::Data)
                        .marcador("dd/mm/aaaa"),
                );
                c[1].add(
                    Campo::novo("Condição de pagamento", &mut f.condicao).marcador("ex.: 28 dias"),
                );
                c[2].add(Campo::novo("Frete combinado", &mut f.frete));
            });
            ui.add_space(Espaco::E8);
            ui.add(Campo::novo(
                "Observação para o fornecedor (opcional)",
                &mut f.observacao,
            ));
            ui.add_space(Espaco::E16);
            ui.horizontal(|ui| {
                ui.add(Rotulo::titulo_secao("Itens"));
                ui.add_space(Espaco::E12);
                if sessao.concede("estoque.compra_sugerida.ver")
                    && ui
                        .add(Botao::secundario("Puxar do radar de reposição").pequeno())
                        .clicked()
                {
                    match linhas_do_radar(motor, sessao) {
                        Ok(v) if v.is_empty() => notificar(
                            ui.ctx(),
                            Notificacao::info(
                                "Nada abaixo do ponto de pedido (descontado o que está a caminho).",
                            ),
                        ),
                        Ok(v) => {
                            f.linhas.retain(|l| l.produto.is_some());
                            for l in v {
                                if !f.linhas.iter().any(|x| x.produto == l.produto) {
                                    f.linhas.push(l);
                                }
                            }
                        }
                        Err(e) => notificar(ui.ctx(), Notificacao::erro(e)),
                    }
                }
            });
            ui.add_space(Espaco::E8);
            editor_linhas(ui, motor, sessao, f, &opcoes, &nomes);
            ui.add_space(Espaco::E12);
            ui.horizontal(|ui| {
                ui.add(Rotulo::campo("Total com frete"));
                ui.add(ValorDinheiro::novo(f.total()).destaque());
            });
        },
        |ui, estado| {
            if ui.add(Botao::primario("Salvar")).clicked() {
                salvar(ui.ctx(), motor, sessao, estado);
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

fn editor_linhas(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    f: &mut FormPedido,
    opcoes: &[OpcaoBusca<Id>],
    nomes: &HashMap<Id, String>,
) {
    let mut remover = None;
    for (i, l) in f.linhas.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.allocate_ui(egui::vec2(380.0, 40.0), |ui| match l.produto {
                Some(p) => {
                    ui.horizontal(|ui| {
                        ui.add(Rotulo::interface(
                            nomes.get(&p).cloned().unwrap_or_else(|| "—".to_owned()),
                        ));
                        if ui.add(Botao::fantasma("Trocar").pequeno()).clicked() {
                            l.produto = None;
                        }
                    });
                }
                None => {
                    SeletorBusca::novo(
                        if i == 0 { "Produto" } else { "" },
                        &mut l.busca,
                        &mut l.produto,
                    )
                    .opcoes(opcoes.to_vec())
                    .marcador("Buscar produto…")
                    .mostrar(ui);
                }
            });
            ui.allocate_ui(egui::vec2(80.0, 40.0), |ui| {
                ui.add(Campo::novo(if i == 0 { "Qtd" } else { "" }, &mut l.qtd));
            });
            ui.allocate_ui(egui::vec2(110.0, 40.0), |ui| {
                ui.add(Campo::novo(
                    if i == 0 { "Preço unit." } else { "" },
                    &mut l.preco,
                ));
            });
            if ui.add(Botao::fantasma("✕").pequeno()).clicked() {
                remover = Some(i);
            }
        });
        // O produto escolhido agora ganha o último preço pago (sem apagar o digitado).
        if let Some(p) = l.produto {
            if l.sugerido_para != Some(p) {
                l.sugerido_para = Some(p);
                if l.preco.trim().is_empty() {
                    if let Some(preco) = ultimos_precos(motor, sessao, vec![p]).get(&p) {
                        l.preco = preco.formatar();
                    }
                }
            }
        }
        ui.add_space(Espaco::E4);
    }
    if let Some(i) = remover {
        f.linhas.remove(i);
        if f.linhas.is_empty() {
            f.linhas.push(LinhaPedido::default());
        }
    }
    if ui
        .add(Botao::secundario("+ Adicionar item").pequeno())
        .clicked()
    {
        f.linhas.push(LinhaPedido::default());
    }
}

fn salvar(ctx: &egui::Context, motor: &Motor, sessao: &Sessao, estado: &mut EstadoPedidos) {
    let Dlg::Form(f) = &estado.dlg else {
        return;
    };
    let cmd = match f.comando() {
        Ok(c) => c,
        Err(e) => {
            notificar(ctx, Notificacao::aviso(e));
            return;
        }
    };
    match motor.executar(sessao, "compras.salvar_pedido.v1", &cmd) {
        Ok(s) => {
            notificar(
                ctx,
                Notificacao::sucesso(format!("Pedido {:04} salvo em rascunho", s.numero)),
            );
            estado.recarregar(motor, sessao);
            estado.abrir(motor, sessao, s.pedido);
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

/// O que o usuário pediu no pedido aberto.
enum Acao {
    Aprovar,
    Editar,
    Copiar,
    WhatsApp(String),
    Pdf,
    Cancelar,
    Encerrar,
}

fn dialogo_ver(ctx: &egui::Context, motor: &Motor, sessao: &Sessao, estado: &mut EstadoPedidos) {
    let Dlg::Ver(p) = &estado.dlg else {
        return;
    };
    let p = (**p).clone();
    let hoje = Data::hoje(Fuso::BRASILIA);
    let atrasado = p.estado.aguardando() && p.previsao_entrega.is_some_and(|d| d < hoje);
    let telefone = estado.telefone(p.fornecedor);
    let empresa = estado
        .identidade
        .as_ref()
        .map(|i| i.nome_fantasia.clone())
        .unwrap_or_default();
    let mut acao = None;
    let fechar = Dialogo::nova(format!(
        "Pedido de compra {:04} · {}",
        p.numero, p.fornecedor_nome
    ))
    .largura(820.0)
    .mostrar(
        ctx,
        estado,
        |ui, _| {
            ui.horizontal(|ui| {
                ui.add(etiqueta_estado(p.estado, atrasado));
                if p.cotacao.is_some() {
                    ui.add(Etiqueta::neutra("saiu de uma cotação"));
                }
            });
            ui.add_space(Espaco::E8);
            ui.columns(3, |c| {
                dado(&mut c[0], "Data", &p.data.formatar());
                dado(
                    &mut c[1],
                    "Entrega combinada",
                    &p.previsao_entrega
                        .map_or_else(|| "—".to_owned(), Data::formatar),
                );
                dado(
                    &mut c[2],
                    "Pagamento",
                    if p.condicao_pagamento.is_empty() {
                        "—"
                    } else {
                        &p.condicao_pagamento
                    },
                );
            });
            if let Some(m) = &p.motivo {
                ui.add_space(Espaco::E8);
                Painel::novo().realce(Tom::Neutro).mostrar(ui, |ui| {
                    ui.add(Rotulo::interface(format!("Motivo: {m}")).quebravel());
                });
            }
            ui.add_space(Espaco::E12);
            let colunas = vec![
                ColunaGrade::nova("Produto"),
                ColunaGrade::nova("Pedido").largura(80.0).numero(),
                ColunaGrade::nova("Chegou").largura(80.0).numero(),
                ColunaGrade::nova("Falta").largura(80.0).numero(),
                ColunaGrade::nova("Preço").largura(100.0).numero(),
                ColunaGrade::nova("Total").largura(110.0).numero(),
            ];
            Grade::nova(colunas)
                .id_salt("itens_pedido")
                .mostrar(ui, p.itens.len(), |i, row| {
                    let it = &p.itens[i];
                    row.col(|ui| {
                        ui.add(Rotulo::interface(it.descricao.clone()));
                    });
                    row.col(|ui| {
                        ui.add(Rotulo::interface(format!(
                            "{} {}",
                            it.quantidade.formatar(0),
                            it.unidade
                        )));
                    });
                    row.col(|ui| {
                        ui.add(Rotulo::interface(it.recebida.formatar(0)));
                    });
                    row.col(|ui| {
                        let s = it.saldo();
                        let r = Rotulo::interface(s.formatar(0));
                        ui.add(if s.e_positiva() && p.estado.aguardando() {
                            r.cor(ui.cores().atencao)
                        } else {
                            r
                        });
                    });
                    row.col(|ui| {
                        ui.add(Rotulo::interface(it.preco_unitario.formatar()));
                    });
                    row.col(|ui| {
                        ui.add(ValorDinheiro::novo(it.total()));
                    });
                });
            ui.add_space(Espaco::E8);
            ui.horizontal(|ui| {
                ui.add(Rotulo::campo("Itens"));
                ui.add(ValorDinheiro::novo(p.total_dos_itens()));
                ui.add_space(Espaco::E12);
                ui.add(Rotulo::campo("Frete"));
                ui.add(ValorDinheiro::novo(p.frete));
                ui.add_space(Espaco::E12);
                ui.add(Rotulo::campo("Total"));
                ui.add(ValorDinheiro::novo(p.total()).destaque());
            });
            if !p.notas.is_empty() {
                ui.add_space(Espaco::E12);
                ui.add(Rotulo::titulo_secao("Notas de entrada"));
                ui.add_space(Espaco::E4);
                for n in &p.notas {
                    ui.horizontal(|ui| {
                        ui.add(Rotulo::interface(format!(
                            "NF {} / {} — {} — {}",
                            n.numero,
                            n.serie,
                            n.emissao.formatar(),
                            n.total.formatar_com_simbolo()
                        )));
                        ui.add(crate::tela_compras::etiqueta_da_nota(n.estado));
                    });
                }
            }
            if !p.observacao.is_empty() {
                ui.add_space(Espaco::E8);
                ui.add(Rotulo::apoio(format!("Obs.: {}", p.observacao)).quebravel());
            }
            ui.add_space(Espaco::E16);
            ui.horizontal_wrapped(|ui| {
                if p.estado == EstadoPedido::Rascunho
                    && sessao.concede("compras.pedido.aprovar")
                    && ui.add(Botao::primario("Aprovar pedido")).clicked()
                {
                    acao = Some(Acao::Aprovar);
                }
                if p.editavel()
                    && sessao.concede("compras.pedido.criar")
                    && ui.add(Botao::secundario("Editar")).clicked()
                {
                    acao = Some(Acao::Editar);
                }
                if !matches!(p.estado, EstadoPedido::Cancelado) {
                    if ui.add(Botao::secundario("Copiar texto")).clicked() {
                        acao = Some(Acao::Copiar);
                    }
                    if let Some(link) = telefone.as_deref().and_then(|t| {
                        crate::telefone::link_whatsapp(
                            t,
                            &p.texto_para_o_fornecedor(&empresa, &p.fornecedor_nome),
                        )
                    }) {
                        if ui.add(Botao::secundario("WhatsApp")).clicked() {
                            acao = Some(Acao::WhatsApp(link));
                        }
                    }
                    if ui.add(Botao::secundario("Salvar PDF…")).clicked() {
                        acao = Some(Acao::Pdf);
                    }
                }
                if sessao.concede("compras.pedido.aprovar") {
                    if matches!(p.estado, EstadoPedido::Rascunho | EstadoPedido::Enviado)
                        && !p.tem_recebimento()
                        && ui.add(Botao::destrutivo("Cancelar pedido")).clicked()
                    {
                        acao = Some(Acao::Cancelar);
                    }
                    if p.estado == EstadoPedido::ParcialmenteRecebido
                        && ui.add(Botao::destrutivo("Encerrar o saldo")).clicked()
                    {
                        acao = Some(Acao::Encerrar);
                    }
                }
            });
            if p.estado == EstadoPedido::Rascunho {
                ui.add_space(Espaco::E4);
                ui.add(
                    Rotulo::apoio(
                        "Em rascunho o pedido ainda não espera mercadoria: aprove para as \
                             notas do fornecedor se ligarem a ele.",
                    )
                    .quebravel(),
                );
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
        return;
    }
    match acao {
        None => {}
        Some(Acao::Aprovar) => {
            match motor.executar(
                sessao,
                "compras.aprovar_pedido.v1",
                &AprovarPedidoCompra { pedido: p.id },
            ) {
                Ok(()) => {
                    notificar(
                        ctx,
                        Notificacao::sucesso("Pedido aprovado — aguardando entrega"),
                    );
                    estado.recarregar(motor, sessao);
                    estado.abrir(motor, sessao, p.id);
                }
                Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
            }
        }
        Some(Acao::Editar) => estado.dlg = Dlg::Form(Box::new(FormPedido::de_pedido(&p))),
        Some(Acao::Copiar) => {
            ctx.copy_text(p.texto_para_o_fornecedor(&empresa, &p.fornecedor_nome));
            notificar(ctx, Notificacao::sucesso("Texto do pedido copiado"));
        }
        Some(Acao::WhatsApp(link)) => {
            if let Err(e) = open::that_detached(&link) {
                notificar(ctx, Notificacao::erro(format!("Não abriu o WhatsApp: {e}")));
            }
        }
        Some(Acao::Pdf) => salvar_pdf(ctx, estado, &p),
        Some(Acao::Cancelar | Acao::Encerrar) => {
            let encerrar = matches!(acao, Some(Acao::Encerrar));
            estado.dlg = Dlg::Motivo {
                pedido: Box::new(p),
                encerrar,
                motivo: String::new(),
            };
        }
    }
}

fn dialogo_motivo(ctx: &egui::Context, motor: &Motor, sessao: &Sessao, estado: &mut EstadoPedidos) {
    let Dlg::Motivo {
        pedido, encerrar, ..
    } = &estado.dlg
    else {
        return;
    };
    let (id, numero, encerrar) = (pedido.id, pedido.numero, *encerrar);
    let titulo = if encerrar {
        format!("Encerrar o saldo do pedido {numero:04}")
    } else {
        format!("Cancelar o pedido {numero:04}")
    };
    let mut confirmar = false;
    let fechar = Dialogo::nova(titulo).largura(520.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            let Dlg::Motivo { motivo, .. } = &mut estado.dlg else {
                return;
            };
            ui.add(
                Rotulo::interface(if encerrar {
                    "O que falta chegar não vem mais: o pedido sai de \"a caminho\" e o radar de \
                     reposição volta a contar com estes itens."
                } else {
                    "O pedido deixa de valer. As notas deste fornecedor que ainda não entraram \
                     deixam de apontar para ele."
                })
                .quebravel(),
            );
            ui.add_space(Espaco::E8);
            ui.add(Campo::novo("Motivo (ao menos 10 caracteres)", motivo));
        },
        |ui, _| {
            if ui
                .add(Botao::destrutivo(if encerrar {
                    "Encerrar o saldo"
                } else {
                    "Cancelar o pedido"
                }))
                .clicked()
            {
                confirmar = true;
            }
        },
    );
    if fechar {
        estado.dlg = Dlg::Fechado;
        return;
    }
    if !confirmar {
        return;
    }
    let Dlg::Motivo { motivo, .. } = &estado.dlg else {
        return;
    };
    let motivo = motivo.clone();
    let r: Result<(), _> = if encerrar {
        motor.executar(
            sessao,
            "compras.encerrar_pedido.v1",
            &EncerrarPedidoCompra { pedido: id, motivo },
        )
    } else {
        motor.executar(
            sessao,
            "compras.cancelar_pedido.v1",
            &CancelarPedidoCompra { pedido: id, motivo },
        )
    };
    match r {
        Ok(()) => {
            notificar(
                ctx,
                Notificacao::sucesso(if encerrar {
                    "Saldo do pedido encerrado"
                } else {
                    "Pedido cancelado"
                }),
            );
            estado.recarregar(motor, sessao);
            estado.abrir(motor, sessao, id);
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

fn salvar_pdf(ctx: &egui::Context, estado: &EstadoPedidos, p: &PedidoCompra) {
    let ident = estado.identidade.clone().unwrap_or_default();
    let empresa = IdentidadeEmpresa {
        nome_fantasia: ident.nome_fantasia,
        razao_social: ident.razao_social,
        cnpj: ident.cnpj,
        endereco: ident.endereco,
        telefone: ident.telefone,
        email: ident.email,
        site: ident.site,
        logo_png: ident.logo_png,
    };
    let forn = estado
        .fornecedores
        .iter()
        .find(|f| f.pessoa == p.fornecedor);
    let doc = PedidoCompraPdf {
        numero: u64::from(p.numero),
        fornecedor_nome: p.fornecedor_nome.clone(),
        fornecedor_documento: forn.and_then(|f| f.documento.clone()).unwrap_or_default(),
        fornecedor_contato: forn
            .and_then(|f| f.telefone.as_deref().map(crate::telefone::formatar))
            .unwrap_or_default(),
        data: p.data,
        previsao_entrega: p.previsao_entrega,
        condicao_pagamento: p.condicao_pagamento.clone(),
        observacoes: p.observacao.clone(),
        itens: p
            .itens
            .iter()
            .map(|i| ItemPdf {
                descricao: i.descricao.clone(),
                quantidade: i.quantidade,
                unidade: i.unidade.clone(),
                preco_unitario: i.preco_unitario,
                desconto_pct: cardeal_kernel::Percentual::ZERO,
                total: i.total(),
            })
            .collect(),
        subtotal: p.total_dos_itens(),
        frete: p.frete,
        total: p.total(),
        responsavel: String::new(),
    };
    let bytes = match gerar_pedido_compra(&empresa, &doc, Instante::agora()) {
        Ok(b) => b,
        Err(e) => {
            notificar(ctx, Notificacao::erro(format!("PDF: {e}")));
            return;
        }
    };
    let Some(caminho) = rfd::FileDialog::new()
        .set_file_name(format!("Pedido de compra {:04}.pdf", p.numero))
        .add_filter("PDF", &["pdf"])
        .save_file()
    else {
        return;
    };
    match std::fs::write(&caminho, &bytes) {
        Ok(()) => notificar(ctx, Notificacao::sucesso("PDF do pedido salvo")),
        Err(e) => notificar(ctx, Notificacao::erro(format!("Não salvou o PDF: {e}"))),
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::tela_compras::apoio_testes::{base, contexto};
    use crate::testes_comum::motor_de_teste;

    #[test]
    fn o_pedido_se_faz_se_aprova_e_se_desenha_pela_tela() {
        let t = motor_de_teste();
        let b = base(&t.motor, &t.sessao);
        let ctx = contexto();
        let mut estado = EstadoPedidos::default();
        estado.carregar(&t.motor, &t.sessao, b.fornecedores.clone());
        assert!(estado.lista.is_empty());
        let quadro = |estado: &mut EstadoPedidos| {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    corpo(ui, &t.motor, &t.sessao, estado);
                });
                dialogos(ctx, &t.motor, &t.sessao, &b.produtos, estado);
            });
        };
        quadro(&mut estado);

        // O formulário: o fornecedor, o produto e o preço; desenha e salva em rascunho.
        abrir_novo(&mut estado);
        if let Dlg::Form(f) = &mut estado.dlg {
            f.fornecedor = Some(b.fornecedor);
            f.linhas[0].produto = Some(b.produto);
            f.linhas[0].qtd = "12".into();
            f.linhas[0].preco = "4,00".into();
            f.frete = "6,00".into();
        }
        quadro(&mut estado);
        salvar(&ctx, &t.motor, &t.sessao, &mut estado);
        let Dlg::Ver(p) = &estado.dlg else {
            panic!("depois de salvar, o pedido abre");
        };
        let pedido = p.id;
        assert_eq!((p.numero, p.estado), (1, EstadoPedido::Rascunho));
        assert_eq!(p.total(), Dinheiro::reais(54));
        assert_eq!(p.itens[0].unidade, "GF");
        quadro(&mut estado);

        // Aprovado, o pedido espera a mercadoria e vai ao fornecedor pelo WhatsApp.
        let (): () = t
            .motor
            .executar(
                &t.sessao,
                "compras.aprovar_pedido.v1",
                &AprovarPedidoCompra { pedido },
            )
            .unwrap();
        estado.recarregar(&t.motor, &t.sessao);
        estado.abrir(&t.motor, &t.sessao, pedido);
        quadro(&mut estado);
        assert_eq!(estado.lista[0].estado, EstadoPedido::Enviado);
        assert!(estado.telefone(b.fornecedor).is_some());

        // Editar o aprovado volta ao rascunho.
        let Dlg::Ver(p) = &estado.dlg else {
            panic!("aberto");
        };
        estado.dlg = Dlg::Form(Box::new(FormPedido::de_pedido(p)));
        if let Dlg::Form(f) = &mut estado.dlg {
            f.linhas[0].qtd = "24".into();
        }
        salvar(&ctx, &t.motor, &t.sessao, &mut estado);
        let Dlg::Ver(p) = &estado.dlg else {
            panic!("aberto");
        };
        assert_eq!(p.estado, EstadoPedido::Rascunho);
        assert_eq!(p.itens[0].quantidade, Quantidade::unidades(24));

        // O diálogo do motivo desenha.
        estado.dlg = Dlg::Motivo {
            pedido: p.clone(),
            encerrar: false,
            motivo: "Comprei de outro fornecedor".into(),
        };
        quadro(&mut estado);
    }

    #[test]
    fn o_formulario_pede_fornecedor_produto_e_preco() {
        let mut f = FormPedido::novo();
        assert!(f.comando().unwrap_err().contains("fornecedor"));
        f.fornecedor = Some(Id::novo());
        f.linhas[0].qtd = "3".into();
        assert!(f.comando().unwrap_err().contains("produto"));
        f.linhas[0].produto = Some(Id::novo());
        assert!(f.comando().unwrap_err().contains("preço"));
        f.linhas[0].preco = "4,50".into();
        f.frete = "10,00".into();
        f.previsao = "05/11/2026".into();
        let c = f.comando().unwrap();
        assert_eq!(c.itens.len(), 1);
        assert_eq!(c.itens[0].preco_unitario, Preco::centavos(450));
        assert_eq!(c.previsao_entrega, Some(Data::de_ymd(2026, 11, 5).unwrap()));
        assert_eq!(f.total(), Dinheiro::centavos(2_350));
        // A linha vazia que sobrou no fim não atrapalha.
        f.linhas.push(LinhaPedido::default());
        assert_eq!(f.comando().unwrap().itens.len(), 1);
    }
}
