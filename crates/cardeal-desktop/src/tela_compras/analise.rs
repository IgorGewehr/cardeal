//! A aba "Análise" das compras: **quanto comprei**, no período, em cada nível da estrutura
//! mercadológica (Grupo › Subgrupo › Categoria › Subcategoria) e os produtos que mais pesaram —
//! das notas de entrada confirmadas (`estoque.entradas_por_produto_e_origem.v1`, origem
//! `compras`). O filtro da estrutura vale para os indicadores e a lista.

use super::*;
use cardeal_kernel::{Data, Dinheiro, Periodo, Quantidade};
use cardeal_ui::atoms::ValorDinheiro;
use cardeal_ui::molecules::CartaoKpi;
use cardeal_ui::organisms::FaixaKpi;
use mod_estoque::{EntradaDoProduto, EntradasPorProdutoEOrigem};

/// A janela de tempo.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum Janela {
    #[default]
    MesAtual,
    Ultimos90,
    Ano,
}

impl Janela {
    fn periodo(self, hoje: Data) -> Periodo {
        match self {
            Self::MesAtual => Periodo::novo(hoje.inicio_do_mes(), hoje),
            Self::Ultimos90 => Periodo::ultimos_dias(hoje, 90),
            Self::Ano => Periodo::novo(hoje.inicio_do_ano(), hoje),
        }
    }
}

/// O estado da aba.
#[derive(Default)]
pub(super) struct EstadoAnaliseCompras {
    janela: Janela,
    entradas: Vec<EntradaDoProduto>,
    estrutura: crate::recorte_estrutura::EstadoRecorte,
    erro: Option<String>,
}

impl EstadoAnaliseCompras {
    pub(super) fn carregar(&mut self, motor: &Motor, sessao: &Sessao) {
        let periodo = self.janela.periodo(Data::hoje(FUSO));
        match motor.consultar(
            sessao,
            "estoque.entradas_por_produto_e_origem.v1",
            &EntradasPorProdutoEOrigem {
                periodo,
                origem: Some("compras".to_owned()),
            },
        ) {
            Ok(e) => {
                self.entradas = e;
                self.erro = None;
            }
            Err(e) => self.erro = Some(e.mensagem),
        }
        self.estrutura.carregar(motor, sessao);
    }

    /// A soma do comprado pela estrutura (todo o período, sem o filtro).
    pub(super) fn soma_da_estrutura(&self) -> cardeal_analytics::SomaPorEstrutura<Dinheiro> {
        self.estrutura
            .recorte()
            .somar(self.entradas.iter().map(|x| (x.produto, x.valor)))
    }

    /// As linhas da lista (dentro do filtro).
    #[cfg(test)]
    pub(super) fn linhas(&self) -> Vec<&EntradaDoProduto> {
        self.filtradas()
    }

    /// Põe o filtro da estrutura.
    #[cfg(test)]
    pub(super) fn filtrar(&mut self, no: Option<Id>) {
        self.estrutura.filtro = no;
    }

    /// As entradas dentro do filtro da estrutura.
    fn filtradas(&self) -> Vec<&EntradaDoProduto> {
        self.entradas
            .iter()
            .filter(|e| self.estrutura.contem(e.produto))
            .collect()
    }
}

pub(super) fn corpo(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    produtos: &[ItemProdutoComSaldo],
    e: &mut EstadoAnaliseCompras,
) {
    ui.horizontal_wrapped(|ui| {
        if let Some(j) = Abas::nova(&[
            (Janela::MesAtual, "Este mês"),
            (Janela::Ultimos90, "90 dias"),
            (Janela::Ano, "Este ano"),
        ])
        .selecionada(e.janela)
        .id_salt("compras-analise-janela")
        .mostrar(ui)
        {
            e.janela = j;
            e.carregar(motor, sessao);
        }
        ui.add_space(Espaco::E16);
        e.estrutura.mostrar_filtro(ui, "Estrutura das compras");
    });
    ui.add_space(Espaco::E12);
    if let Some(erro) = &e.erro {
        ui.add(
            Rotulo::interface(erro.clone())
                .quebravel()
                .cor(ui.cores().negativo),
        );
        return;
    }

    let mut ordenadas: Vec<EntradaDoProduto> = e.filtradas().into_iter().cloned().collect();
    ordenadas.sort_by_key(|l| std::cmp::Reverse(l.valor));
    let total: Dinheiro = ordenadas.iter().map(|l| l.valor).sum();
    let quantidade: Quantidade = ordenadas.iter().map(|l| l.quantidade).sum();
    FaixaKpi::nova(vec![
        CartaoKpi::novo("Comprado no período", total),
        CartaoKpi::contagem("Produtos comprados", ordenadas.len())
            .variacao(format!("{} unidades", quantidade.formatar(0))),
    ])
    .mostrar(ui);
    ui.add_space(Espaco::E12);

    let caminhos: Vec<String> = ordenadas
        .iter()
        .map(|l| e.estrutura.recorte().caminho(l.produto))
        .collect();
    let nome = |p: Id| {
        produtos
            .iter()
            .find(|x| x.produto == p)
            .map_or_else(|| "Produto".to_owned(), |x| x.nome.clone())
    };
    let lista = |ui: &mut egui::Ui| {
        Grade::nova(vec![
            ColunaGrade::nova("Produto"),
            ColunaGrade::nova("Estrutura").largura(220.0),
            ColunaGrade::nova("Qtd").largura(80.0).numero(),
            ColunaGrade::nova("Total").largura(120.0).numero(),
        ])
        .id_salt("compras-analise-lista")
        .vazio("Nenhuma compra confirmada neste recorte.")
        .mostrar(ui, ordenadas.len(), |i, row| {
            let l = &ordenadas[i];
            row.col(|ui| {
                ui.add(Rotulo::interface(nome(l.produto)));
            });
            row.col(|ui| {
                ui.add(Rotulo::apoio(caminhos[i].clone()));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(l.quantidade.formatar(0)));
            });
            row.col(|ui| {
                ui.add(ValorDinheiro::novo(l.valor).neutro());
            });
        });
    };
    let arvore = |ui: &mut egui::Ui, e: &mut EstadoAnaliseCompras| {
        Painel::novo().titulo("Por estrutura").mostrar(ui, |ui| {
            let soma = e.soma_da_estrutura();
            e.estrutura.mostrar_soma(ui, &soma);
        });
    };
    if !e.estrutura.tem_arvore() {
        lista(ui);
    } else if ui.available_width() > 900.0 {
        ui.columns(2, |c| {
            arvore(&mut c[0], e);
            lista(&mut c[1]);
        });
    } else {
        arvore(ui, e);
        ui.add_space(Espaco::E12);
        lista(ui);
    }
    ui.add_space(Espaco::E8);
    ui.add(Rotulo::apoio(
        "Das notas de entrada confirmadas, ao custo de cada nota (com frete e impostos rateados). \
         Vale a classificação de hoje.",
    ));
}
