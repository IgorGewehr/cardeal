//! A aba "Rentabilidade": quanto cada Grupo/Subgrupo/Categoria/Subcategoria vendeu, quanto
//! custou e quanto deixou de margem num período, somando o que saiu por venda (pedido
//! faturado), PDV (cupom finalizado) e OS (peças de OS faturada).
//!
//! A tela só orquestra: chama a consulta pública de cada módulo, **ignora** o módulo que falta
//! ou que o usuário não pode consultar (ele simplesmente não soma — a tela diz quem entrou) e
//! entrega tudo ao domínio puro de `cardeal-analytics`. A margem líquida é **estimada**: usa os
//! encargos de hoje (operação, imposto, comissão e taxas).

use std::collections::HashSet;

use super::*;
use cardeal_analytics::{
    calcular, CustoDoProduto, NoDaEstrutura, NoDeRentabilidade, ProdutoDaEstrutura,
    ProdutoRentavel, ReceitaDoProduto, Rentabilidade,
};
use cardeal_kernel::{Data, Percentual, Periodo};
use cardeal_ui::atoms::ValorDinheiro;
use cardeal_ui::organisms::{ArvoreHierarquia, NoArvore, Painel};
use mod_estoque::hierarquia::{nome_do_nivel, Arvore, NoGrupo};
use mod_estoque::{
    ArvoreGruposProduto, EncargosAtuais, ProdutosParaLote, SaidaDoProduto, SaidasPorProdutoEOrigem,
};

/// A janela de tempo escolhida.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum Janela {
    #[default]
    MesAtual,
    Ultimos30,
    Ultimos90,
    Ano,
}

impl Janela {
    /// O período que a janela cobre, até `hoje`.
    pub(super) fn periodo(self, hoje: Data) -> Periodo {
        match self {
            Self::MesAtual => Periodo::novo(hoje.inicio_do_mes(), hoje),
            Self::Ultimos30 => Periodo::ultimos_dias(hoje, 30),
            Self::Ultimos90 => Periodo::ultimos_dias(hoje, 90),
            Self::Ano => Periodo::novo(
                Data::de_ymd(hoje.ano(), 1, 1).unwrap_or(hoje.inicio_do_mes()),
                hoje,
            ),
        }
    }
}

/// O estado da aba.
#[derive(Default)]
pub(super) struct EstadoRentabilidade {
    janela: Janela,
    pub(super) resultado: Rentabilidade,
    /// As origens (módulos) cuja receita entrou na conta.
    pub(super) origens: Vec<&'static str>,
    /// As que não entraram (módulo ausente ou sem permissão).
    pub(super) ausentes: Vec<&'static str>,
    /// Os encargos de hoje e a margem líquida mínima.
    encargos: Percentual,
    margem_minima: Percentual,
    /// Se o custo (estoque) foi consultado; sem ele, nada tem margem.
    custo_disponivel: bool,
    recolhidos: HashSet<Id>,
    pub(super) selecionado: Option<Id>,
    /// Descendentes do nó selecionado (inclusive ele), para filtrar os produtos.
    subarvore: HashSet<Id>,
}

/// Uma origem de receita: o nome que aparece na tela, o `origem_modulo` do estoque e o que ela devolve.
struct Origem {
    rotulo: &'static str,
    modulo: &'static str,
    receita: Option<Vec<ReceitaDoProduto>>,
}

impl EstadoRentabilidade {
    pub(super) fn carregar(&mut self, motor: &Motor, sessao: &Sessao) {
        let periodo = self.janela.periodo(Data::hoje(Fuso::BRASILIA));
        let arvore: Vec<_> = motor
            .consultar(
                sessao,
                "estoque.arvore_grupos_produto.v1",
                &ArvoreGruposProduto,
            )
            .unwrap_or_default();
        let nos: Vec<NoDaEstrutura> = arvore
            .iter()
            .map(|n| NoDaEstrutura {
                id: n.id,
                pai: n.pai,
                nome: n.nome.clone(),
                codigo: n.codigo.clone(),
                nivel: n.nivel,
            })
            .collect();
        let produtos: Vec<ProdutoDaEstrutura> = motor
            .consultar(
                sessao,
                "estoque.produtos_para_lote.v1",
                &ProdutosParaLote::default(),
            )
            .unwrap_or_default()
            .into_iter()
            .map(|p| ProdutoDaEstrutura {
                produto: p.produto,
                nome: p.nome,
                grupo: p.grupo,
            })
            .collect();

        let origens = [
            Origem {
                rotulo: "Vendas",
                modulo: "vendas",
                receita: motor
                    .consultar(
                        sessao,
                        "vendas.receita_por_produto.v1",
                        &mod_vendas::ReceitaPorProduto { periodo },
                    )
                    .ok(),
            },
            Origem {
                rotulo: "PDV",
                modulo: "pdv",
                receita: motor
                    .consultar(
                        sessao,
                        "pdv.receita_por_produto.v1",
                        &mod_pdv::ReceitaPorProduto { periodo },
                    )
                    .ok(),
            },
            Origem {
                rotulo: "OS",
                modulo: "os",
                receita: motor
                    .consultar(
                        sessao,
                        "os.receita_por_produto.v1",
                        &mod_os::ReceitaPorProduto { periodo },
                    )
                    .ok(),
            },
        ];
        let saidas: Option<Vec<SaidaDoProduto>> = motor
            .consultar(
                sessao,
                "estoque.saidas_por_produto_e_origem.v1",
                &SaidasPorProdutoEOrigem { periodo },
            )
            .ok();
        if let Ok(e) = motor.consultar(sessao, "estoque.encargos_atuais.v1", &EncargosAtuais) {
            self.encargos = e.total();
            self.margem_minima = e.margem_minima;
        }

        self.custo_disponivel = saidas.is_some();
        self.origens = origens
            .iter()
            .filter(|o| o.receita.is_some())
            .map(|o| o.rotulo)
            .collect();
        self.ausentes = origens
            .iter()
            .filter(|o| o.receita.is_none())
            .map(|o| o.rotulo)
            .collect();
        // O custo só vale para as origens cuja receita entrou.
        let custos: Vec<CustoDoProduto> = saidas
            .unwrap_or_default()
            .into_iter()
            .filter(|s| {
                origens
                    .iter()
                    .any(|o| o.receita.is_some() && o.modulo == s.origem_modulo)
            })
            .map(|s| CustoDoProduto {
                produto: s.produto,
                quantidade: s.quantidade,
                custo: s.custo,
            })
            .collect();
        let receitas: Vec<ReceitaDoProduto> = origens
            .into_iter()
            .filter_map(|o| o.receita)
            .flatten()
            .collect();

        self.resultado = calcular(&nos, &produtos, &receitas, &custos, self.encargos);
        if self
            .selecionado
            .is_some_and(|s| !nos.iter().any(|n| n.id == s))
        {
            self.selecionado = None;
        }
        self.atualizar_subarvore();
    }

    fn atualizar_subarvore(&mut self) {
        self.subarvore.clear();
        let Some(id) = self.selecionado else {
            return;
        };
        let arvore = Arvore::de_nos(
            &self
                .resultado
                .nos
                .iter()
                .map(|n| NoGrupo {
                    id: n.id,
                    pai: n.pai,
                })
                .collect::<Vec<_>>(),
        );
        self.subarvore.extend(arvore.descendentes(id));
        self.subarvore.insert(id);
    }

    pub(super) fn selecionar(&mut self, id: Id) {
        self.selecionado = if self.selecionado == Some(id) {
            None
        } else {
            Some(id)
        };
        self.atualizar_subarvore();
    }

    /// Os produtos que a grade mostra: os do nó selecionado (e abaixo) ou todos.
    pub(super) fn produtos_visiveis(&self) -> Vec<&ProdutoRentavel> {
        self.resultado
            .produtos
            .iter()
            .filter(|p| {
                self.selecionado.is_none() || p.grupo.is_some_and(|g| self.subarvore.contains(&g))
            })
            .collect()
    }
}

fn pct(p: Option<Percentual>) -> String {
    p.map_or_else(|| "—".to_owned(), |p| format!("{}%", p.formatar(1)))
}

/// A etiqueta da margem líquida estimada: verde na meta, âmbar abaixo dela, vermelho no prejuízo.
fn etiqueta_de_margem(no: &NoDeRentabilidade, minima: Percentual) -> Etiqueta {
    match no.margem_liquida_pct {
        None => Etiqueta::neutra("sem custo"),
        Some(p) => {
            let texto = format!("líq. {}", pct(Some(p)));
            if p < Percentual::ZERO {
                Etiqueta::negativa(texto)
            } else if p < minima {
                Etiqueta::atencao(texto)
            } else {
                Etiqueta::positiva(texto)
            }
        }
    }
}

pub(super) fn aba(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
) {
    ui.horizontal_wrapped(|ui| {
        ui.add(Rotulo::sobrelinha("Período"));
        ui.add_space(Espaco::E8);
        if let Some(j) = Abas::nova(&[
            (Janela::MesAtual, "Mês atual"),
            (Janela::Ultimos30, "Últimos 30 dias"),
            (Janela::Ultimos90, "Últimos 90 dias"),
            (Janela::Ano, "Ano"),
        ])
        .selecionada(estado.rent.janela)
        .id_salt("rentabilidade-janela")
        .mostrar(ui)
        {
            estado.rent.janela = j;
            estado.rent.carregar(motor, sessao);
        }
    });
    ui.add_space(Espaco::E12);

    let r = &estado.rent;
    let t = &r.resultado.total;
    FaixaKpi::nova(vec![
        CartaoKpi::novo("Receita", t.receita),
        CartaoKpi::novo("Custo", t.custo).variacao("do que tem custo conhecido"),
        CartaoKpi::novo("Margem bruta", t.margem_bruta)
            .variacao(format!("{} da receita com custo", pct(t.margem_bruta_pct))),
        CartaoKpi::novo("Margem líquida estimada", t.margem_liquida_estimada)
            .variacao(format!("{} · estimada", pct(t.margem_liquida_pct)))
            .tom(match t.margem_liquida_pct {
                Some(p) if p < Percentual::ZERO => Tom::Negativo,
                Some(p) if p < r.margem_minima => Tom::Atencao,
                Some(_) => Tom::Positivo,
                None => Tom::Neutro,
            }),
        CartaoKpi::contagem("Itens sem custo", t.itens_sem_custo)
            .tom(if t.itens_sem_custo > 0 {
                Tom::Atencao
            } else {
                Tom::Neutro
            })
            .variacao("fora da margem"),
    ])
    .mostrar(ui);
    ui.add_space(Espaco::E8);

    ui.add(Rotulo::apoio(origens_e_aviso(r)).quebravel());
    ui.add_space(Espaco::E12);

    if r.resultado.nos.is_empty() && r.resultado.produtos.is_empty() {
        EstadoVazio::novo(
            Icone::Estoque,
            "Nada vendido neste período. Quando houver pedido faturado, cupom finalizado ou OS \
             faturada, a rentabilidade aparece aqui.",
        )
        .mostrar(ui);
        return;
    }
    ui.columns(2, |c| {
        arvore(&mut c[0], estado);
        produtos(&mut c[1], estado);
    });
}

fn origens_e_aviso(r: &EstadoRentabilidade) -> String {
    let mut texto = if r.origens.is_empty() {
        "Nenhuma origem de venda disponível para o seu acesso.".to_owned()
    } else {
        format!("Entraram na conta: {}.", r.origens.join(", "))
    };
    if !r.ausentes.is_empty() {
        texto.push_str(&format!(
            " Fora (módulo ausente ou sem permissão): {}.",
            r.ausentes.join(", ")
        ));
    }
    if !r.custo_disponivel {
        texto.push_str(" Sem acesso ao custo do estoque: nada tem margem.");
    }
    texto.push_str(&format!(
        " A margem líquida é estimada: desconta {}% de encargos de hoje (operação, imposto, \
         comissão e taxas). Produto sem custo conhecido conta na receita, mas fica fora da margem.",
        r.encargos.formatar(1)
    ));
    texto
}

fn arvore(ui: &mut egui::Ui, estado: &mut EstadoTelaEstoque) {
    let e = &mut estado.rent;
    let nos: Vec<NoArvore> = e
        .resultado
        .nos
        .iter()
        .map(|n| NoArvore {
            id: n.id,
            nivel: n.nivel,
            rotulo: n.nome.clone(),
            apoio: Some(format!(
                "{} · {} · {} da receita · {} produto(s) vendido(s)",
                n.codigo,
                nome_do_nivel(n.nivel),
                pct(Some(n.participacao)),
                n.produtos
            )),
            tem_filhos: e.resultado.nos.iter().any(|f| f.pai == Some(n.id)),
        })
        .collect();
    if nos.is_empty() {
        EstadoVazio::novo(
            Icone::Estoque,
            "Sem estrutura mercadológica: crie os grupos na aba Estrutura.",
        )
        .mostrar(ui);
        return;
    }
    let minima = e.margem_minima;
    let dados = e.resultado.nos.clone();
    let resposta = ArvoreHierarquia::nova(&nos, &mut e.recolhidos)
        .selecionado(e.selecionado)
        .mostrar(ui, |ui, no| {
            let Some(n) = dados.iter().find(|n| n.id == no.id) else {
                return;
            };
            ui.add(ValorDinheiro::novo(n.receita).neutro());
            if n.produtos > 0 {
                ui.add(etiqueta_de_margem(n, minima));
            }
        });
    if let Some(id) = resposta.clicado {
        e.selecionar(id);
    }
}

fn produtos(ui: &mut egui::Ui, estado: &mut EstadoTelaEstoque) {
    let e = &estado.rent;
    let titulo = e
        .selecionado
        .and_then(|s| e.resultado.nos.iter().find(|n| n.id == s))
        .map_or_else(
            || "Todos os produtos vendidos".to_owned(),
            |n| format!("{}: produtos da subárvore", n.nome),
        );
    Painel::novo().plano().mostrar(ui, |ui| {
        ui.add(Rotulo::titulo_secao(titulo));
        ui.add(Rotulo::apoio(
            "Clique num nível da árvore para ver só os produtos dele (e dos níveis abaixo).",
        ));
        ui.add_space(Espaco::E8);
        let linhas = e.produtos_visiveis();
        Grade::nova(vec![
            ColunaGrade::nova("Produto").largura(190.0),
            ColunaGrade::nova("Qtd").largura(60.0).numero(),
            ColunaGrade::nova("Receita").largura(100.0).numero(),
            ColunaGrade::nova("Custo").largura(100.0).numero(),
            ColunaGrade::nova("Margem").largura(100.0).numero(),
            ColunaGrade::nova("Margem %").largura(90.0).numero(),
        ])
        .id_salt("rentabilidade-produtos")
        .vazio("Nenhum produto vendido neste recorte.")
        .mostrar(ui, linhas.len(), |i, row| {
            let p = linhas[i];
            row.col(|ui| {
                ui.add(Rotulo::interface(if p.nome.is_empty() {
                    "(produto fora do cadastro)".to_owned()
                } else {
                    p.nome.clone()
                }));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(p.quantidade.to_string()));
            });
            row.col(|ui| {
                ui.add(ValorDinheiro::novo(p.receita).neutro());
            });
            row.col(|ui| match p.custo {
                Some(c) => {
                    ui.add(ValorDinheiro::novo(c).neutro());
                }
                None => {
                    ui.add(Etiqueta::atencao("sem custo"));
                }
            });
            row.col(|ui| match p.margem_bruta {
                Some(m) => {
                    ui.add(ValorDinheiro::novo(m).neutro());
                }
                None => {
                    ui.add(Rotulo::interface("—").cor(ui.cores().texto_fraco));
                }
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(pct(p.margem_bruta_pct)));
            });
        });
    });
}
