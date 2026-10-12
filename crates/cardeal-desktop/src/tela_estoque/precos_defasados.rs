//! A seção "Preços defasados" da aba Preços: os produtos de uma tabela de preço cujo preço
//! praticado ficou para trás do que o estoque calcula hoje (o custo subiu, a margem caiu), com
//! o botão que leva o preço sugerido aos defasados.

use super::precos::aplicar_em;
use super::*;
use cardeal_kernel::{Dinheiro, Percentual};
use cardeal_ui::atoms::ValorDinheiro;
use mod_estoque::precificacao::ArredondamentoDePreco;
use mod_vendas::{PrecoDefasado, PrecosDefasados, SituacaoDoPreco};

/// Qual recorte da lista aparece.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum Recorte {
    /// Prejuízo e abaixo do alvo.
    #[default]
    Defasados,
    /// Margem bem acima da desejada.
    AcimaDoAlvo,
    /// Tudo.
    Todos,
}

impl Recorte {
    fn aceita(self, s: &SituacaoDoPreco) -> bool {
        match self {
            Self::Defasados => s.e_defasado(),
            Self::AcimaDoAlvo => *s == SituacaoDoPreco::AcimaDoAlvo,
            Self::Todos => true,
        }
    }
}

/// O estado da seção.
#[derive(Default)]
pub(super) struct EstadoDefasados {
    pub(super) tabelas: Vec<(Id, String)>,
    pub(super) tabela: Option<Id>,
    pub(super) itens: Vec<PrecoDefasado>,
    recorte: Recorte,
}

impl EstadoDefasados {
    /// Recarrega a lista da tabela escolhida (a primeira, se nenhuma). Sem permissão ou sem
    /// tabela, a seção só fica vazia.
    pub(super) fn carregar(&mut self, motor: &Motor, sessao: &Sessao) {
        if self
            .tabela
            .is_none_or(|id| !self.tabelas.iter().any(|(t, _)| *t == id))
        {
            self.tabela = self.tabelas.first().map(|(id, _)| *id);
        }
        self.itens = self
            .tabela
            .and_then(|tabela_preco| {
                motor
                    .consultar(
                        sessao,
                        "vendas.precos_defasados.v1",
                        &PrecosDefasados { tabela_preco },
                    )
                    .ok()
            })
            .unwrap_or_default();
    }

    fn visiveis(&self) -> Vec<&PrecoDefasado> {
        self.itens
            .iter()
            .filter(|d| self.recorte.aceita(&d.situacao))
            .collect()
    }

    /// Os produtos que a ação em lote atinge: defasados com preço sugerido calculado.
    pub(super) fn a_corrigir(&self) -> Vec<Id> {
        self.itens
            .iter()
            .filter(|d| d.situacao.e_defasado() && d.preco_sugerido.is_some())
            .map(|d| d.produto)
            .collect()
    }
}

fn pct(p: Option<Percentual>) -> String {
    p.map_or_else(|| "—".to_owned(), |p| format!("{}%", p.formatar(1)))
}

fn dinheiro(ui: &mut egui::Ui, v: Option<Dinheiro>) {
    match v {
        Some(v) => {
            ui.add(ValorDinheiro::novo(v).neutro());
        }
        None => {
            ui.add(Rotulo::interface("—").cor(ui.cores().texto_fraco));
        }
    }
}

fn etiqueta(ui: &mut egui::Ui, s: &SituacaoDoPreco) {
    match s {
        SituacaoDoPreco::Prejuizo => ui.add(Etiqueta::negativa("prejuízo")),
        SituacaoDoPreco::AbaixoDoAlvo => ui.add(Etiqueta::atencao("abaixo do alvo")),
        SituacaoDoPreco::NoAlvo => ui.add(Etiqueta::positiva("no alvo")),
        SituacaoDoPreco::AcimaDoAlvo => ui.add(Etiqueta::info("acima do alvo")),
        SituacaoDoPreco::SemCalculo(m) => ui.add(Etiqueta::neutra(format!("sem cálculo: {m}"))),
    };
}

pub(super) fn secao(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
) {
    ui.add(Rotulo::titulo_secao("Preços defasados"));
    ui.add(Rotulo::apoio(
        "Compara o preço da tabela com o que o estoque calcula hoje. \"Defasado\" é prejuízo ou \
         margem mais de 1 ponto abaixo da desejada.",
    ));
    ui.add_space(Espaco::E8);
    if estado.prc.def.tabelas.is_empty() {
        ui.add(Rotulo::interface(
            "Não há tabela de preço de venda ou atacado. Crie uma em Vendas › Tabelas de preço.",
        ));
        return;
    }

    let antes = estado.prc.def.tabela;
    let ops = estado.prc.def.tabelas.clone();
    SeletorOpcao::novo("Tabela de preço", &mut estado.prc.def.tabela)
        .opcoes(ops)
        .mostrar(ui);
    if antes != estado.prc.def.tabela {
        estado.prc.def.carregar(motor, sessao);
    }
    ui.add_space(Espaco::E8);
    if let Some(r) = Abas::nova(&[
        (Recorte::Defasados, "Defasados"),
        (Recorte::AcimaDoAlvo, "Acima do alvo"),
        (Recorte::Todos, "Todos"),
    ])
    .selecionada(estado.prc.def.recorte)
    .id_salt("prc-def-recorte")
    .mostrar(ui)
    {
        estado.prc.def.recorte = r;
    }
    ui.add_space(Espaco::E8);

    let linhas = estado.prc.def.visiveis();
    Grade::nova(vec![
        ColunaGrade::nova("Produto").largura(220.0),
        ColunaGrade::nova("Situação").largura(220.0),
        ColunaGrade::nova("Margem atual × desejada").largura(180.0),
        ColunaGrade::nova("Preço atual").largura(110.0).numero(),
        ColunaGrade::nova("Preço sugerido").largura(120.0).numero(),
    ])
    .id_salt("precos-defasados")
    .vazio("Nenhum produto neste recorte.")
    .mostrar(ui, linhas.len(), |i, row| {
        let d = linhas[i];
        row.col(|ui| {
            ui.add(Rotulo::interface(d.nome.clone()));
        });
        row.col(|ui| etiqueta(ui, &d.situacao));
        row.col(|ui| {
            ui.add(Rotulo::interface(format!(
                "{} × {}",
                pct(d.margem_atual),
                pct(d.margem_desejada)
            )));
        });
        row.col(|ui| dinheiro(ui, Some(d.preco_atual)));
        row.col(|ui| dinheiro(ui, d.preco_sugerido));
    });
    ui.add_space(Espaco::E12);

    ui.add(Rotulo::campo("Preço de prateleira ao aplicar"));
    if let Some(a) = Abas::nova(&[
        (ArredondamentoDePreco::Nenhum, "Como calculado"),
        (ArredondamentoDePreco::FinalNoventa, "Terminar em ,90"),
        (ArredondamentoDePreco::InteiroAcima, "Real inteiro"),
    ])
    .selecionada(estado.prc.arredondamento)
    .id_salt("prc-def-arred")
    .mostrar(ui)
    {
        estado.prc.arredondamento = a;
    }
    ui.add_space(Espaco::E8);
    let produtos = estado.prc.def.a_corrigir();
    let texto = format!(
        "Aplicar o preço sugerido aos defasados ({})",
        produtos.len()
    );
    if ui
        .add_enabled(!produtos.is_empty(), Botao::primario(texto))
        .clicked()
    {
        if let Some(tabela) = estado.prc.def.tabela {
            aplicar_em(ui.ctx(), motor, sessao, estado, tabela, produtos);
        }
    }
}
