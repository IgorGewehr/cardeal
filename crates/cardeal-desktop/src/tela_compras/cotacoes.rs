//! A aba **Cotações** de Compras (`mod_compras::cotacao`): pedir preço dos mesmos itens a vários
//! fornecedores, registrar o que cada um respondeu, ver as propostas lado a lado — com o menor
//! preço de cada item, o total de cada fornecedor com o frete e a escolha sugerida — e decidir,
//! item a item, de quem comprar: a cotação gera um pedido por fornecedor.

use std::collections::HashMap;

use cardeal_cliente::{Motor, Sessao};
use cardeal_kernel::{Data, Dinheiro, Id, Preco, Quantidade};
use cardeal_modkit::Icone;
use cardeal_ui::atoms::{Botao, Caixa, Etiqueta, Rotulo, Tom, ValorDinheiro};
use cardeal_ui::molecules::{
    dado, Campo, EstadoVazio, Mascara, OpcaoBusca, SeletorBusca, SeletorOpcao,
};
use cardeal_ui::organisms::{notificar, ColunaGrade, Dialogo, Grade, Notificacao, Painel};
use cardeal_ui::tokens::{Espaco, TemaUi};
use eframe::egui;
use mod_clientes::ItemPessoa;
use mod_compras::cotacao::{Comparativo, CotacaoSalva, ResumoCotacao};
use mod_compras::{
    CancelarCotacao, ComparativoDaCotacao, Cotacao, CotacaoCompleta, Cotacoes, DecidirCotacao,
    EscolhaDaCotacao, EstadoCotacao, ItemDaCotacao, PedidoSalvo, RegistrarPropostaFornecedor,
    SalvarCotacao,
};
use mod_estoque::ItemProdutoComSaldo;

#[derive(Default, Clone)]
struct LinhaCotacao {
    produto: Option<Id>,
    busca: String,
    qtd: String,
}

#[derive(Default)]
struct FormCotacao {
    editar: Option<Id>,
    descricao: String,
    prazo: String,
    linhas: Vec<LinhaCotacao>,
    fornecedores: Vec<Id>,
    novo_fornecedor: Option<Id>,
}

impl FormCotacao {
    fn nova() -> Self {
        Self {
            linhas: vec![LinhaCotacao::default()],
            ..Self::default()
        }
    }

    fn de_cotacao(c: &Cotacao) -> Self {
        Self {
            editar: Some(c.id),
            descricao: c.descricao.clone(),
            prazo: c.prazo_resposta.map(Data::formatar).unwrap_or_default(),
            linhas: c
                .itens
                .iter()
                .map(|i| LinhaCotacao {
                    produto: Some(i.produto),
                    busca: String::new(),
                    qtd: i.quantidade.formatar(0),
                })
                .collect(),
            fornecedores: c.propostas.iter().map(|p| p.fornecedor).collect(),
            novo_fornecedor: None,
        }
    }

    fn comando(&self) -> Result<SalvarCotacao, String> {
        let prazo = match self.prazo.trim() {
            "" => None,
            t => Some(
                t.parse::<Data>()
                    .map_err(|_| "O prazo das respostas não é uma data (dd/mm/aaaa).")?,
            ),
        };
        let mut itens = Vec::new();
        for (n, l) in self.linhas.iter().enumerate() {
            let Some(produto) = l.produto else {
                continue;
            };
            let quantidade = l
                .qtd
                .trim()
                .parse::<Quantidade>()
                .map_err(|_| format!("Linha {}: a quantidade não é um número.", n + 1))?;
            itens.push(ItemDaCotacao {
                produto,
                quantidade,
            });
        }
        if itens.is_empty() {
            return Err("Escolha ao menos um produto.".to_owned());
        }
        if self.fornecedores.is_empty() {
            return Err("Convide ao menos um fornecedor.".to_owned());
        }
        Ok(SalvarCotacao {
            cotacao: self.editar,
            descricao: self.descricao.clone(),
            prazo_resposta: prazo,
            itens,
            fornecedores: self.fornecedores.clone(),
        })
    }
}

#[derive(Default)]
struct FormProposta {
    fornecedor: Option<Id>,
    /// O preço digitado de cada item, na ordem da cotação: (item, texto).
    precos: Vec<(Id, String)>,
    prazo: String,
    condicao: String,
    frete: String,
    observacao: String,
}

impl FormProposta {
    /// O formulário do fornecedor, com a proposta que ele já mandou (se mandou).
    fn de(cot: &Cotacao, fornecedor: Option<Id>) -> Self {
        let proposta = fornecedor.and_then(|f| cot.propostas.iter().find(|p| p.fornecedor == f));
        Self {
            fornecedor,
            precos: cot
                .itens
                .iter()
                .map(|i| {
                    (
                        i.id,
                        proposta
                            .and_then(|p| p.preco(i.id))
                            .map(Preco::formatar)
                            .unwrap_or_default(),
                    )
                })
                .collect(),
            prazo: proposta
                .and_then(|p| p.prazo_entrega_dias)
                .map(|d| d.to_string())
                .unwrap_or_default(),
            condicao: proposta
                .map(|p| p.condicao_pagamento.clone())
                .unwrap_or_default(),
            frete: proposta.map_or_else(|| "0,00".to_owned(), |p| p.frete.formatar()),
            observacao: proposta.map(|p| p.observacao.clone()).unwrap_or_default(),
        }
    }

    fn comando(&self, cotacao: Id) -> Result<RegistrarPropostaFornecedor, String> {
        let fornecedor = self.fornecedor.ok_or("Escolha o fornecedor.")?;
        let mut precos = Vec::new();
        for (item, texto) in &self.precos {
            if texto.trim().is_empty() {
                continue;
            }
            let p = texto
                .trim()
                .parse::<Preco>()
                .map_err(|_| format!("\"{texto}\" não é um preço."))?;
            precos.push((*item, p));
        }
        let prazo = match self.prazo.trim() {
            "" => None,
            t => Some(
                t.parse::<u16>()
                    .map_err(|_| "O prazo de entrega é em dias (um número).")?,
            ),
        };
        let frete = match self.frete.trim() {
            "" => Dinheiro::ZERO,
            t => Dinheiro::de_str(t).map_err(|_| "O frete não é um valor.")?,
        };
        Ok(RegistrarPropostaFornecedor {
            cotacao,
            fornecedor,
            precos,
            prazo_entrega_dias: prazo,
            condicao_pagamento: self.condicao.clone(),
            frete,
            observacao: self.observacao.clone(),
        })
    }
}

/// A cotação aberta no diálogo.
struct Aberta {
    cot: Cotacao,
    comp: Comparativo,
    /// De quem comprar cada item (a decisão sendo montada).
    escolhas: HashMap<Id, Option<Id>>,
    aprovar: bool,
}

impl Aberta {
    /// O total da escolha atual, com o frete de cada fornecedor que entra.
    fn total_da_escolha(&self) -> Dinheiro {
        let mut usados: Vec<usize> = Vec::new();
        let mut total = Dinheiro::ZERO;
        for l in &self.comp.linhas {
            let Some(Some(f)) = self.escolhas.get(&l.item) else {
                continue;
            };
            let Some(col) = self.comp.colunas.iter().position(|c| c.fornecedor == *f) else {
                continue;
            };
            total += l.celulas[col].total.unwrap_or_default();
            if !usados.contains(&col) {
                usados.push(col);
                total += self.comp.colunas[col].frete;
            }
        }
        total
    }
}

#[derive(Default)]
enum Dlg {
    #[default]
    Fechado,
    Form(Box<FormCotacao>),
    Ver(Box<Aberta>),
    Proposta {
        cot: Box<Cotacao>,
        form: FormProposta,
    },
    Cancelar {
        cotacao: Id,
        numero: u32,
        motivo: String,
    },
}

/// Estado da aba.
#[derive(Default)]
pub struct EstadoCotacoes {
    lista: Vec<ResumoCotacao>,
    /// Mostrar também as decididas e as canceladas.
    todas: bool,
    fornecedores: Vec<ItemPessoa>,
    empresa: String,
    erro: Option<String>,
    dlg: Dlg,
    /// O pedido que o usuário pediu para abrir (a tela troca para a aba Pedidos).
    pub abrir_pedido: Option<Id>,
}

impl EstadoCotacoes {
    /// Recarrega a lista.
    pub fn carregar(&mut self, motor: &Motor, sessao: &Sessao, fornecedores: Vec<ItemPessoa>) {
        self.fornecedores = fornecedores;
        match motor.consultar(
            sessao,
            "compras.cotacoes.v1",
            &Cotacoes {
                so_abertas: !self.todas,
            },
        ) {
            Ok(v) => {
                self.lista = v;
                self.erro = None;
            }
            Err(e) => self.erro = Some(e.mensagem),
        }
        if self.empresa.is_empty() {
            self.empresa = motor
                .identidade_visual()
                .map(|i| i.nome_fantasia)
                .unwrap_or_default();
        }
    }

    fn recarregar(&mut self, motor: &Motor, sessao: &Sessao) {
        let f = std::mem::take(&mut self.fornecedores);
        self.carregar(motor, sessao, f);
    }

    fn abrir(&mut self, motor: &Motor, sessao: &Sessao, cotacao: Id) {
        let cot = motor.consultar(sessao, "compras.cotacao.v1", &CotacaoCompleta { cotacao });
        let comp = motor.consultar(
            sessao,
            "compras.comparativo_da_cotacao.v1",
            &ComparativoDaCotacao { cotacao },
        );
        match (cot, comp) {
            (Ok(Some(cot)), Ok(Some(comp))) => {
                let mut escolhas: HashMap<Id, Option<Id>> =
                    cot.itens.iter().map(|i| (i.id, i.escolhido)).collect();
                if cot.estado == EstadoCotacao::Aberta {
                    for (item, f) in &comp.sugerida {
                        escolhas.insert(*item, Some(*f));
                    }
                }
                self.dlg = Dlg::Ver(Box::new(Aberta {
                    cot,
                    comp,
                    escolhas,
                    aprovar: sessao.concede("compras.pedido.aprovar"),
                }));
            }
            (Err(e), _) | (_, Err(e)) => self.erro = Some(e.mensagem),
            _ => self.erro = Some("Cotação não encontrada.".to_owned()),
        }
    }

    fn nome(&self, fornecedor: Id) -> String {
        self.fornecedores
            .iter()
            .find(|f| f.pessoa == fornecedor)
            .map_or_else(|| "—".to_owned(), |f| f.nome.clone())
    }
}

/// Abre o formulário de cotação nova.
pub fn abrir_nova(estado: &mut EstadoCotacoes) {
    estado.dlg = Dlg::Form(Box::new(FormCotacao::nova()));
}

fn etiqueta(e: EstadoCotacao) -> Etiqueta {
    match e {
        EstadoCotacao::Aberta => Etiqueta::info("Aberta"),
        EstadoCotacao::Decidida => Etiqueta::positiva("Decidida"),
        EstadoCotacao::Cancelada => Etiqueta::neutra("Cancelada"),
    }
}

/// O miolo da aba.
pub fn corpo(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoCotacoes) {
    if let Some(erro) = &estado.erro {
        ui.add(
            Rotulo::interface(erro.clone())
                .quebravel()
                .cor(ui.cores().negativo),
        );
        ui.add_space(Espaco::E12);
    }
    ui.horizontal(|ui| {
        let rot = if estado.todas {
            "Só as abertas"
        } else {
            "Mostrar também as decididas e canceladas"
        };
        if ui.add(Botao::fantasma(rot).pequeno()).clicked() {
            estado.todas = !estado.todas;
            estado.recarregar(motor, sessao);
        }
    });
    ui.add_space(Espaco::E8);
    if estado.lista.is_empty() {
        if estado.erro.is_none()
            && EstadoVazio::novo(
                Icone::Nota,
                "Nenhuma cotação aberta. Pergunte o preço a vários fornecedores e compare.",
            )
            .acao("Fazer uma cotação")
            .mostrar(ui)
        {
            abrir_nova(estado);
        }
        return;
    }
    let colunas = vec![
        ColunaGrade::nova("Nº").largura(60.0).numero(),
        ColunaGrade::nova("Descrição"),
        ColunaGrade::nova("Data").largura(96.0),
        ColunaGrade::nova("Responder até").largura(110.0),
        ColunaGrade::nova("Itens").largura(60.0).numero(),
        ColunaGrade::nova("Respostas").largura(100.0).numero(),
        ColunaGrade::nova("Estado").largura(110.0),
    ];
    let resposta = Grade::nova(colunas)
        .id_salt("cotacoes")
        .selecionavel(None)
        .mostrar(ui, estado.lista.len(), |i, row| {
            let c = &estado.lista[i];
            row.col(|ui| {
                ui.add(Rotulo::interface(format!("{:04}", c.numero)));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(if c.descricao.is_empty() {
                    "—".to_owned()
                } else {
                    c.descricao.clone()
                }));
            });
            row.col(|ui| {
                ui.add(Rotulo::campo(c.data.formatar_curta()));
            });
            row.col(|ui| {
                ui.add(Rotulo::campo(
                    c.prazo_resposta
                        .map_or_else(|| "—".to_owned(), Data::formatar_curta),
                ));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(c.itens.to_string()));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(format!(
                    "{} de {}",
                    c.respostas, c.fornecedores
                )));
            });
            row.col(|ui| {
                ui.add(etiqueta(c.estado));
            });
        });
    if let Some(i) = resposta.linha_clicada {
        let id = estado.lista[i].cotacao;
        estado.abrir(motor, sessao, id);
    }
}

/// Os diálogos da aba.
pub fn dialogos(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    produtos: &[ItemProdutoComSaldo],
    estado: &mut EstadoCotacoes,
) {
    match estado.dlg {
        Dlg::Fechado => {}
        Dlg::Form(_) => dialogo_form(ctx, motor, sessao, produtos, estado),
        Dlg::Ver(_) => dialogo_ver(ctx, motor, sessao, estado),
        Dlg::Proposta { .. } => dialogo_proposta(ctx, motor, sessao, estado),
        Dlg::Cancelar { .. } => dialogo_cancelar(ctx, motor, sessao, estado),
    }
}

fn dialogo_form(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    produtos: &[ItemProdutoComSaldo],
    estado: &mut EstadoCotacoes,
) {
    let opcoes: Vec<OpcaoBusca<Id>> = produtos
        .iter()
        .map(|p| OpcaoBusca::nova(p.produto, p.nome.clone()))
        .collect();
    let nomes_produto: HashMap<Id, String> = produtos
        .iter()
        .map(|p| (p.produto, p.nome.clone()))
        .collect();
    let ops_forn: Vec<(Id, String)> = estado
        .fornecedores
        .iter()
        .map(|f| (f.pessoa, f.nome.clone()))
        .collect();
    let nomes_forn: HashMap<Id, String> = ops_forn.iter().cloned().collect();
    let titulo = match &estado.dlg {
        Dlg::Form(f) if f.editar.is_some() => "Editar cotação",
        _ => "Nova cotação",
    };
    let fechar = Dialogo::nova(titulo).largura(760.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            let Dlg::Form(f) = &mut estado.dlg else {
                return;
            };
            ui.columns(2, |c| {
                c[0].add(
                    Campo::novo("Descrição", &mut f.descricao)
                        .marcador("ex.: Reposição de bebidas — novembro"),
                );
                c[1].add(
                    Campo::novo("Responder até (opcional)", &mut f.prazo)
                        .mascara(Mascara::Data)
                        .marcador("dd/mm/aaaa"),
                );
            });
            ui.add_space(Espaco::E16);
            ui.add(Rotulo::titulo_secao("Itens"));
            ui.add_space(Espaco::E8);
            let mut remover = None;
            for (i, l) in f.linhas.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.allocate_ui(egui::vec2(440.0, 40.0), |ui| match l.produto {
                        Some(p) => {
                            ui.horizontal(|ui| {
                                ui.add(Rotulo::interface(
                                    nomes_produto.get(&p).cloned().unwrap_or_default(),
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
                            .opcoes(opcoes.clone())
                            .marcador("Buscar produto…")
                            .mostrar(ui);
                        }
                    });
                    ui.allocate_ui(egui::vec2(90.0, 40.0), |ui| {
                        ui.add(Campo::novo(if i == 0 { "Qtd" } else { "" }, &mut l.qtd));
                    });
                    if ui.add(Botao::fantasma("✕").pequeno()).clicked() {
                        remover = Some(i);
                    }
                });
                ui.add_space(Espaco::E4);
            }
            if let Some(i) = remover {
                f.linhas.remove(i);
                if f.linhas.is_empty() {
                    f.linhas.push(LinhaCotacao::default());
                }
            }
            if ui
                .add(Botao::secundario("+ Adicionar item").pequeno())
                .clicked()
            {
                f.linhas.push(LinhaCotacao::default());
            }
            ui.add_space(Espaco::E16);
            ui.add(Rotulo::titulo_secao("Fornecedores convidados"));
            ui.add_space(Espaco::E8);
            let mut tirar = None;
            for (n, forn) in f.fornecedores.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.add(Rotulo::interface(
                        nomes_forn.get(forn).cloned().unwrap_or_default(),
                    ));
                    if ui.add(Botao::fantasma("✕").pequeno()).clicked() {
                        tirar = Some(n);
                    }
                });
            }
            if let Some(n) = tirar {
                f.fornecedores.remove(n);
            }
            ui.horizontal(|ui| {
                ui.allocate_ui(egui::vec2(360.0, 40.0), |ui| {
                    SeletorOpcao::novo("Convidar", &mut f.novo_fornecedor)
                        .opcoes(
                            ops_forn
                                .iter()
                                .filter(|(id, _)| !f.fornecedores.contains(id))
                                .cloned(),
                        )
                        .placeholder("Escolha um fornecedor…")
                        .mostrar(ui);
                });
                if let Some(novo) = f.novo_fornecedor {
                    if ui.add(Botao::secundario("Adicionar").pequeno()).clicked() {
                        f.fornecedores.push(novo);
                        f.novo_fornecedor = None;
                    }
                }
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

fn salvar(ctx: &egui::Context, motor: &Motor, sessao: &Sessao, estado: &mut EstadoCotacoes) {
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
    match motor.executar(sessao, "compras.salvar_cotacao.v1", &cmd) {
        Ok(CotacaoSalva { cotacao, numero }) => {
            notificar(
                ctx,
                Notificacao::sucesso(format!("Cotação {numero:04} salva")),
            );
            estado.recarregar(motor, sessao);
            estado.abrir(motor, sessao, cotacao);
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

enum Acao {
    Proposta(Option<Id>),
    Copiar,
    Editar,
    Cancelar,
    Decidir,
    AbrirPedido(Id),
}

#[allow(clippy::too_many_lines)] // o comparativo, a decisão e as ações, num diálogo só
fn dialogo_ver(ctx: &egui::Context, motor: &Motor, sessao: &Sessao, estado: &mut EstadoCotacoes) {
    let Dlg::Ver(a) = &estado.dlg else {
        return;
    };
    let (numero, aberta_ok) = (a.cot.numero, a.cot.estado == EstadoCotacao::Aberta);
    let decide = aberta_ok && sessao.concede("compras.cotacao.decidir");
    let pode_aprovar = sessao.concede("compras.pedido.aprovar");
    let mut acao = None;
    let fechar = Dialogo::nova(format!("Cotação {numero:04}"))
        .largura(900.0)
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                let Dlg::Ver(a) = &mut estado.dlg else {
                    return;
                };
                ui.horizontal(|ui| {
                    ui.add(etiqueta(a.cot.estado));
                    if !a.cot.descricao.is_empty() {
                        ui.add(Rotulo::interface(a.cot.descricao.clone()));
                    }
                });
                ui.add_space(Espaco::E8);
                ui.columns(3, |c| {
                    dado(&mut c[0], "Data", &a.cot.data.formatar());
                    dado(
                        &mut c[1],
                        "Responder até",
                        &a.cot
                            .prazo_resposta
                            .map_or_else(|| "—".to_owned(), Data::formatar),
                    );
                    dado(
                        &mut c[2],
                        "Respostas",
                        &format!(
                            "{} de {}",
                            a.cot.propostas.iter().filter(|p| p.respondeu).count(),
                            a.cot.propostas.len()
                        ),
                    );
                });
                if let Some(m) = &a.cot.motivo {
                    ui.add(Rotulo::apoio(format!("Cancelada: {m}")).quebravel());
                }

                // Os fornecedores: o total de cada um, com o frete.
                ui.add_space(Espaco::E12);
                ui.add(Rotulo::titulo_secao("Propostas"));
                ui.add_space(Espaco::E4);
                let colunas = vec![
                    ColunaGrade::nova("Fornecedor"),
                    ColunaGrade::nova("Cotou").largura(70.0).numero(),
                    ColunaGrade::nova("Itens").largura(110.0).numero(),
                    ColunaGrade::nova("Frete").largura(90.0).numero(),
                    ColunaGrade::nova("Total").largura(110.0).numero(),
                    ColunaGrade::nova("Prazo").largura(80.0),
                    ColunaGrade::nova("Pagamento").largura(130.0),
                ];
                let n_itens = a.comp.linhas.len();
                let r = Grade::nova(colunas)
                    .id_salt("propostas")
                    .selecionavel(None)
                    .mostrar(ui, a.comp.colunas.len(), |i, row| {
                        let c = &a.comp.colunas[i];
                        row.col(|ui| {
                            ui.horizontal(|ui| {
                                ui.add(Rotulo::interface(c.nome.clone()));
                                if a.comp.melhor_unico == Some(i) {
                                    ui.add(Etiqueta::positiva("melhor total"));
                                }
                            });
                        });
                        row.col(|ui| {
                            ui.add(Rotulo::interface(if c.respondeu {
                                format!("{} de {n_itens}", c.cotados)
                            } else {
                                "—".to_owned()
                            }));
                        });
                        row.col(|ui| {
                            ui.add(ValorDinheiro::novo(c.itens));
                        });
                        row.col(|ui| {
                            ui.add(ValorDinheiro::novo(c.frete));
                        });
                        row.col(|ui| {
                            if c.respondeu {
                                ui.add(ValorDinheiro::novo(c.total));
                            } else {
                                ui.add(Etiqueta::atencao("sem resposta"));
                            }
                        });
                        row.col(|ui| {
                            ui.add(Rotulo::campo(
                                c.prazo_entrega_dias
                                    .map_or_else(|| "—".to_owned(), |d| format!("{d} dias")),
                            ));
                        });
                        row.col(|ui| {
                            ui.add(Rotulo::campo(if c.condicao_pagamento.is_empty() {
                                "—".to_owned()
                            } else {
                                c.condicao_pagamento.clone()
                            }));
                        });
                    });
                if aberta_ok {
                    if let Some(i) = r.linha_clicada {
                        acao = Some(Acao::Proposta(Some(a.comp.colunas[i].fornecedor)));
                    }
                    ui.add(
                        Rotulo::apoio(
                            "Clique num fornecedor para registrar ou corrigir a proposta.",
                        )
                        .quebravel(),
                    );
                }

                // Os itens: os preços de cada fornecedor e o último pago.
                ui.add_space(Espaco::E12);
                ui.add(Rotulo::titulo_secao("Itens"));
                ui.add_space(Espaco::E4);
                let colunas = vec![
                    ColunaGrade::nova("Item"),
                    ColunaGrade::nova("Qtd").largura(70.0).numero(),
                    ColunaGrade::nova("Último pago").largura(100.0).numero(),
                    ColunaGrade::nova("Preços cotados").largura(330.0),
                ];
                Grade::nova(colunas).id_salt("itens_cotacao").mostrar(
                    ui,
                    a.comp.linhas.len(),
                    |i, row| {
                        let l = &a.comp.linhas[i];
                        row.col(|ui| {
                            ui.add(Rotulo::interface(l.descricao.clone()));
                        });
                        row.col(|ui| {
                            ui.add(Rotulo::interface(format!(
                                "{} {}",
                                l.quantidade.formatar(0),
                                l.unidade
                            )));
                        });
                        row.col(|ui| {
                            ui.add(Rotulo::campo(
                                l.ultimo_pago
                                    .map_or_else(|| "—".to_owned(), Preco::formatar),
                            ));
                        });
                        row.col(|ui| {
                            ui.horizontal_wrapped(|ui| {
                                let mut algum = false;
                                for (n, c) in l.celulas.iter().enumerate() {
                                    let Some(p) = c.preco else { continue };
                                    algum = true;
                                    let texto =
                                        format!("{} {}", a.comp.colunas[n].nome, p.formatar());
                                    ui.add(if c.melhor {
                                        Etiqueta::positiva(texto)
                                    } else {
                                        Etiqueta::neutra(texto)
                                    });
                                }
                                if !algum {
                                    ui.add(Rotulo::apoio("ninguém cotou"));
                                }
                            });
                        });
                    },
                );

                // A decisão.
                if decide && !a.comp.colunas.is_empty() {
                    ui.add_space(Espaco::E16);
                    Painel::novo().realce(Tom::Info).mostrar(ui, |ui| {
                        ui.add(Rotulo::titulo_secao("De quem comprar"));
                        ui.add_space(Espaco::E4);
                        ui.add(
                            Rotulo::apoio(format!(
                                "Sugestão: {} — o mais barato com os fretes somados \
                                 (dividir entre fornecedores só compensa quando a economia paga o \
                                 frete a mais). Deixe um item sem fornecedor para não comprá-lo.",
                                a.comp.total_sugerido.formatar_com_simbolo()
                            ))
                            .quebravel(),
                        );
                        ui.add_space(Espaco::E8);
                        for l in &a.comp.linhas {
                            let ops: Vec<(Id, String)> = l
                                .celulas
                                .iter()
                                .enumerate()
                                .filter_map(|(n, c)| {
                                    let col = &a.comp.colunas[n];
                                    c.total.map(|t| {
                                        (
                                            col.fornecedor,
                                            format!("{} — {}", col.nome, t.formatar_com_simbolo()),
                                        )
                                    })
                                })
                                .collect();
                            let sel = a.escolhas.entry(l.item).or_insert(None);
                            ui.allocate_ui(egui::vec2(560.0, 44.0), |ui| {
                                SeletorOpcao::novo(l.descricao.clone(), sel)
                                    .opcoes(ops)
                                    .placeholder("Não comprar")
                                    .mostrar(ui);
                            });
                        }
                        ui.add_space(Espaco::E8);
                        ui.horizontal(|ui| {
                            ui.add(Rotulo::campo("Total da escolha, com frete"));
                            ui.add(ValorDinheiro::novo(a.total_da_escolha()).destaque());
                            if ui
                                .add(Botao::fantasma("Usar a sugestão").pequeno())
                                .clicked()
                            {
                                for v in a.escolhas.values_mut() {
                                    *v = None;
                                }
                                for (item, f) in &a.comp.sugerida {
                                    a.escolhas.insert(*item, Some(*f));
                                }
                            }
                        });
                        if pode_aprovar {
                            ui.add(Caixa::nova(
                                &mut a.aprovar,
                                "Gerar os pedidos já aprovados (prontos para mandar)",
                            ));
                        }
                        ui.add_space(Espaco::E8);
                        if ui
                            .add(Botao::primario("Decidir e gerar os pedidos"))
                            .clicked()
                        {
                            acao = Some(Acao::Decidir);
                        }
                    });
                }

                if !a.cot.pedidos.is_empty() {
                    ui.add_space(Espaco::E12);
                    ui.add(Rotulo::titulo_secao("Pedidos gerados"));
                    for (pedido, n) in &a.cot.pedidos {
                        if ui
                            .add(Botao::fantasma(format!("Pedido {n:04}")).pequeno())
                            .clicked()
                        {
                            acao = Some(Acao::AbrirPedido(*pedido));
                        }
                    }
                }

                ui.add_space(Espaco::E16);
                ui.horizontal_wrapped(|ui| {
                    if aberta_ok {
                        if sessao.concede("compras.cotacao.editar")
                            && ui.add(Botao::secundario("Registrar proposta")).clicked()
                        {
                            acao = Some(Acao::Proposta(None));
                        }
                        if ui.add(Botao::secundario("Copiar a solicitação")).clicked() {
                            acao = Some(Acao::Copiar);
                        }
                        if sessao.concede("compras.cotacao.criar")
                            && ui.add(Botao::secundario("Editar")).clicked()
                        {
                            acao = Some(Acao::Editar);
                        }
                        if decide && ui.add(Botao::destrutivo("Cancelar cotação")).clicked() {
                            acao = Some(Acao::Cancelar);
                        }
                    }
                });
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
    let Some(acao) = acao else { return };
    let Dlg::Ver(a) = &estado.dlg else { return };
    match acao {
        Acao::Proposta(f) => {
            let cot = a.cot.clone();
            let form = FormProposta::de(&cot, f);
            estado.dlg = Dlg::Proposta {
                cot: Box::new(cot),
                form,
            };
        }
        Acao::Copiar => {
            ctx.copy_text(a.cot.texto_da_solicitacao(&estado.empresa));
            notificar(ctx, Notificacao::sucesso("Solicitação de cotação copiada"));
        }
        Acao::Editar => estado.dlg = Dlg::Form(Box::new(FormCotacao::de_cotacao(&a.cot))),
        Acao::Cancelar => {
            estado.dlg = Dlg::Cancelar {
                cotacao: a.cot.id,
                numero: a.cot.numero,
                motivo: String::new(),
            };
        }
        Acao::AbrirPedido(p) => {
            estado.abrir_pedido = Some(p);
            estado.dlg = Dlg::Fechado;
        }
        Acao::Decidir => {
            let escolhas: Vec<EscolhaDaCotacao> =
                a.cot
                    .itens
                    .iter()
                    .filter_map(|i| {
                        a.escolhas.get(&i.id).copied().flatten().map(|fornecedor| {
                            EscolhaDaCotacao {
                                item: i.id,
                                fornecedor,
                            }
                        })
                    })
                    .collect();
            let cmd = DecidirCotacao {
                cotacao: a.cot.id,
                escolhas,
                aprovar_pedidos: a.aprovar && pode_aprovar,
            };
            let cotacao = a.cot.id;
            match motor.executar(sessao, "compras.decidir_cotacao.v1", &cmd) {
                Ok(pedidos) => {
                    let pedidos: Vec<PedidoSalvo> = pedidos;
                    let quem: Vec<String> =
                        pedidos.iter().map(|p| format!("{:04}", p.numero)).collect();
                    notificar(
                        ctx,
                        Notificacao::sucesso(format!(
                            "Cotação decidida — pedido(s) {}",
                            quem.join(", ")
                        )),
                    );
                    estado.recarregar(motor, sessao);
                    estado.abrir(motor, sessao, cotacao);
                }
                Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
            }
        }
    }
}

fn dialogo_proposta(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoCotacoes,
) {
    let Dlg::Proposta { cot, .. } = &estado.dlg else {
        return;
    };
    let cot = (**cot).clone();
    let ops: Vec<(Id, String)> = estado
        .fornecedores
        .iter()
        .map(|f| (f.pessoa, f.nome.clone()))
        .collect();
    let mut salvar = false;
    let fechar = Dialogo::nova(format!("Proposta — cotação {:04}", cot.numero))
        .largura(640.0)
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                let Dlg::Proposta { form, .. } = &mut estado.dlg else {
                    return;
                };
                let antes = form.fornecedor;
                SeletorOpcao::novo("Fornecedor", &mut form.fornecedor)
                    .opcoes(ops)
                    .placeholder("Quem mandou a proposta…")
                    .mostrar(ui);
                if form.fornecedor != antes {
                    *form = FormProposta::de(&cot, form.fornecedor);
                }
                ui.add_space(Espaco::E8);
                ui.add(
                    Rotulo::apoio("Preço por unidade. Deixe em branco o que o fornecedor não tem.")
                        .quebravel(),
                );
                ui.add_space(Espaco::E4);
                for (item, texto) in &mut form.precos {
                    let Some(i) = cot.itens.iter().find(|x| x.id == *item) else {
                        continue;
                    };
                    ui.horizontal(|ui| {
                        ui.allocate_ui(egui::vec2(380.0, 40.0), |ui| {
                            ui.add(Rotulo::interface(format!(
                                "{} — {} {}",
                                i.descricao,
                                i.quantidade.formatar(0),
                                i.unidade
                            )));
                        });
                        ui.allocate_ui(egui::vec2(140.0, 40.0), |ui| {
                            ui.add(Campo::novo("", texto).marcador("preço unit."));
                        });
                    });
                }
                ui.add_space(Espaco::E8);
                ui.columns(3, |c| {
                    c[0].add(Campo::novo("Entrega (dias)", &mut form.prazo));
                    c[1].add(Campo::novo("Pagamento", &mut form.condicao));
                    c[2].add(Campo::novo("Frete", &mut form.frete));
                });
                ui.add_space(Espaco::E8);
                ui.add(Campo::novo("Observação (opcional)", &mut form.observacao));
            },
            |ui, _| {
                if ui.add(Botao::primario("Registrar")).clicked() {
                    salvar = true;
                }
            },
        );
    if fechar {
        estado.abrir(motor, sessao, cot.id);
        return;
    }
    if !salvar {
        return;
    }
    let Dlg::Proposta { form, .. } = &estado.dlg else {
        return;
    };
    let cmd = match form.comando(cot.id) {
        Ok(c) => c,
        Err(e) => {
            notificar(ctx, Notificacao::aviso(e));
            return;
        }
    };
    let quem = estado.nome(cmd.fornecedor);
    match motor.executar(sessao, "compras.registrar_proposta.v1", &cmd) {
        Ok(()) => {
            notificar(
                ctx,
                Notificacao::sucesso(format!("Proposta de {quem} registrada")),
            );
            estado.recarregar(motor, sessao);
            estado.abrir(motor, sessao, cot.id);
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

fn dialogo_cancelar(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoCotacoes,
) {
    let Dlg::Cancelar {
        cotacao, numero, ..
    } = &estado.dlg
    else {
        return;
    };
    let (cotacao, numero) = (*cotacao, *numero);
    let mut confirmar = false;
    let fechar = Dialogo::nova(format!("Cancelar a cotação {numero:04}"))
        .largura(480.0)
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                let Dlg::Cancelar { motivo, .. } = &mut estado.dlg else {
                    return;
                };
                ui.add(Campo::novo("Motivo (ao menos 10 caracteres)", motivo));
            },
            |ui, _| {
                if ui.add(Botao::destrutivo("Cancelar a cotação")).clicked() {
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
    let Dlg::Cancelar { motivo, .. } = &estado.dlg else {
        return;
    };
    let cmd = CancelarCotacao {
        cotacao,
        motivo: motivo.clone(),
    };
    match motor.executar(sessao, "compras.cancelar_cotacao.v1", &cmd) {
        Ok(()) => {
            notificar(ctx, Notificacao::sucesso("Cotação cancelada"));
            estado.recarregar(motor, sessao);
            estado.dlg = Dlg::Fechado;
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::tela_compras::apoio_testes::{base, contexto};
    use crate::testes_comum::motor_de_teste;

    #[test]
    fn a_cotacao_compara_as_propostas_e_gera_o_pedido_pela_tela() {
        let t = motor_de_teste();
        let b = base(&t.motor, &t.sessao);
        let ctx = contexto();
        let mut estado = EstadoCotacoes::default();
        estado.carregar(&t.motor, &t.sessao, b.fornecedores.clone());
        let quadro = |estado: &mut EstadoCotacoes| {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    corpo(ui, &t.motor, &t.sessao, estado);
                });
                dialogos(ctx, &t.motor, &t.sessao, &b.produtos, estado);
            });
        };
        quadro(&mut estado);

        abrir_nova(&mut estado);
        if let Dlg::Form(f) = &mut estado.dlg {
            f.descricao = "Reposição de bebidas".into();
            f.linhas[0].produto = Some(b.produto);
            f.linhas[0].qtd = "10".into();
            f.fornecedores = vec![b.fornecedor, b.outro_fornecedor];
        }
        quadro(&mut estado);
        salvar(&ctx, &t.motor, &t.sessao, &mut estado);
        let Dlg::Ver(a) = &estado.dlg else {
            panic!("salva, a cotação abre");
        };
        let cot = a.cot.clone();
        assert_eq!(cot.propostas.len(), 2);
        quadro(&mut estado);

        // As duas propostas, pelo formulário.
        for (fornecedor, preco) in [(b.fornecedor, "4,00"), (b.outro_fornecedor, "3,80")] {
            let mut form = FormProposta::de(&cot, Some(fornecedor));
            form.precos[0].1 = preco.into();
            form.frete = "0,00".into();
            let (): () = t
                .motor
                .executar(
                    &t.sessao,
                    "compras.registrar_proposta.v1",
                    &form.comando(cot.id).unwrap(),
                )
                .unwrap();
        }
        estado.abrir(&t.motor, &t.sessao, cot.id);
        let Dlg::Ver(a) = &estado.dlg else {
            panic!("aberta");
        };
        // A sugestão é a Beta (3,80), e a escolha já vem com ela.
        assert_eq!(a.comp.sugerida, vec![(cot.itens[0].id, b.outro_fornecedor)]);
        assert_eq!(a.total_da_escolha(), Dinheiro::reais(38));
        quadro(&mut estado);
        estado.dlg = Dlg::Proposta {
            cot: Box::new(cot.clone()),
            form: FormProposta::de(&cot, Some(b.fornecedor)),
        };
        quadro(&mut estado);

        let pedidos: Vec<PedidoSalvo> = t
            .motor
            .executar(
                &t.sessao,
                "compras.decidir_cotacao.v1",
                &DecidirCotacao {
                    cotacao: cot.id,
                    escolhas: vec![EscolhaDaCotacao {
                        item: cot.itens[0].id,
                        fornecedor: b.outro_fornecedor,
                    }],
                    aprovar_pedidos: true,
                },
            )
            .unwrap();
        assert_eq!(pedidos.len(), 1);
        estado.abrir(&t.motor, &t.sessao, cot.id);
        quadro(&mut estado);
        let Dlg::Ver(a) = &estado.dlg else {
            panic!("aberta");
        };
        assert_eq!(a.cot.estado, EstadoCotacao::Decidida);
        assert_eq!(a.cot.pedidos.len(), 1);
    }

    #[test]
    fn a_proposta_ignora_o_item_em_branco_e_le_o_prazo_em_dias() {
        let mut f = FormProposta {
            fornecedor: Some(Id::novo()),
            precos: vec![(Id::novo(), "10,50".into()), (Id::novo(), String::new())],
            prazo: "5".into(),
            condicao: "28 dias".into(),
            frete: "12,00".into(),
            observacao: String::new(),
        };
        let c = f.comando(Id::novo()).unwrap();
        assert_eq!(c.precos.len(), 1);
        assert_eq!(c.precos[0].1, Preco::centavos(1_050));
        assert_eq!(c.prazo_entrega_dias, Some(5));
        assert_eq!(c.frete, Dinheiro::reais(12));
        f.prazo = "uma semana".into();
        assert!(f.comando(Id::novo()).is_err());
    }

    #[test]
    fn a_cotacao_pede_item_e_fornecedor() {
        let mut f = FormCotacao::nova();
        assert!(f.comando().unwrap_err().contains("produto"));
        f.linhas[0].produto = Some(Id::novo());
        f.linhas[0].qtd = "12".into();
        assert!(f.comando().unwrap_err().contains("fornecedor"));
        f.fornecedores.push(Id::novo());
        assert_eq!(
            f.comando().unwrap().itens[0].quantidade,
            Quantidade::unidades(12)
        );
    }
}
