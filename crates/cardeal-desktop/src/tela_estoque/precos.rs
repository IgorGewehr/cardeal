//! A aba "Preços": os parâmetros de formação de preço da empresa (regime, custo operacional,
//! comissão, taxas, margens) e, produto a produto, o preço sugerido pelo markup divisor — com
//! de onde veio cada percentual e o preço de empate abaixo do qual a venda dá prejuízo.

use super::*;
use cardeal_kernel::{Dinheiro, Percentual};
use cardeal_ui::atoms::ValorDinheiro;
use cardeal_ui::molecules::Abas;
use mod_estoque::precificacao::{AnexoSimples, ArredondamentoDePreco, RegimeImposto};
use mod_estoque::{
    DefinirParametrosPreco, OrigemMargem, ParametrosDePreco, ParametrosPreco,
    PrecosSugeridosDoGrupo, SugestaoDePreco,
};
use mod_vendas::{
    AplicarPrecosSugeridos, PrecosAplicados, TabelaPreco, TabelasDePreco, TipoTabela,
};

/// Regime em edição (sem os dados que dependem dele).
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Regime {
    Mei,
    #[default]
    Simples,
    Manual,
}

/// O estado da aba.
#[derive(Default)]
pub(super) struct EstadoPrecos {
    parametros: Option<ParametrosPreco>,
    pub(super) sugestoes: Vec<SugestaoDePreco>,
    grupo: Option<Id>,
    busca: String,
    // Diálogo "aplicar à tabela de preço".
    tabelas: Vec<TabelaPreco>,
    pub(super) tabela: Option<Id>,
    pub(super) arredondamento: ArredondamentoDePreco,
    // Formulário de parâmetros.
    regime: Regime,
    anexo: u8,
    imposto_manual: String,
    opex: String,
    opex_derivado: bool,
    pub(super) comissao: String,
    taxa: String,
    margem_padrao: String,
    margem_minima: String,
    obrigatorio: bool,
}

const ANEXOS: [AnexoSimples; 5] = [
    AnexoSimples::I,
    AnexoSimples::II,
    AnexoSimples::III,
    AnexoSimples::IV,
    AnexoSimples::V,
];

fn pct(p: Percentual) -> String {
    p.formatar(2)
}

impl EstadoPrecos {
    pub(super) fn carregar(&mut self, motor: &Motor, sessao: &Sessao) {
        if let Ok(p) = motor.consultar(sessao, "estoque.parametros_de_preco.v1", &ParametrosDePreco)
        {
            self.preencher(&p);
            self.parametros = Some(p);
        }
        if let Ok(s) = motor.consultar(
            sessao,
            "estoque.precos_sugeridos_do_grupo.v1",
            &PrecosSugeridosDoGrupo { grupo: self.grupo },
        ) {
            self.sugestoes = s;
        }
    }

    fn preencher(&mut self, p: &ParametrosPreco) {
        match p.regime {
            RegimeImposto::Mei => self.regime = Regime::Mei,
            RegimeImposto::Simples(a) => {
                self.regime = Regime::Simples;
                self.anexo = ANEXOS.iter().position(|x| *x == a).unwrap_or(0) as u8;
            }
            RegimeImposto::Manual(v) => {
                self.regime = Regime::Manual;
                self.imposto_manual = pct(v);
            }
        }
        self.opex = pct(p.custo_operacional);
        self.opex_derivado = p.custo_operacional_derivado;
        self.comissao = pct(p.comissao);
        self.taxa = pct(p.taxa_pagamento);
        self.margem_padrao = p.margem_padrao.map(pct).unwrap_or_default();
        self.margem_minima = pct(p.margem_minima);
        self.obrigatorio = p.preco_obrigatorio;
    }

    /// Monta os parâmetros do formulário; erro com o nome do campo ruim.
    pub(super) fn montar(&self) -> Result<ParametrosPreco, String> {
        let le = |nome: &str, t: &str| -> Result<Percentual, String> {
            let t = t.trim();
            if t.is_empty() {
                return Ok(Percentual::ZERO);
            }
            Percentual::de_str(t).map_err(|_| format!("{nome}: digite um número, como 12,5"))
        };
        let regime = match self.regime {
            Regime::Mei => RegimeImposto::Mei,
            Regime::Simples => {
                RegimeImposto::Simples(ANEXOS[usize::from(self.anexo).min(ANEXOS.len() - 1)])
            }
            Regime::Manual => {
                RegimeImposto::Manual(le("Imposto sobre a venda", &self.imposto_manual)?)
            }
        };
        Ok(ParametrosPreco {
            regime,
            custo_operacional: le("Custo operacional", &self.opex)?,
            custo_operacional_derivado: self.opex_derivado,
            comissao: le("Comissão", &self.comissao)?,
            taxa_pagamento: le("Taxa de pagamento", &self.taxa)?,
            margem_padrao: if self.margem_padrao.trim().is_empty() {
                None
            } else {
                Some(le("Margem padrão", &self.margem_padrao)?)
            },
            margem_minima: le("Margem mínima", &self.margem_minima)?,
            preco_obrigatorio: self.obrigatorio,
        })
    }

    pub(super) fn abrir_aplicar(&mut self, motor: &Motor, sessao: &Sessao) {
        if let Ok(t) = motor.consultar(sessao, "vendas.tabelas_de_preco.v1", &TabelasDePreco) {
            self.tabelas = t
                .into_iter()
                .filter(|t| t.tipo != TipoTabela::Promocional)
                .collect();
        }
        if self
            .tabela
            .is_none_or(|id| !self.tabelas.iter().any(|t| t.id == id))
        {
            self.tabela = self.tabelas.first().map(|t| t.id);
        }
    }

    fn filtradas(&self) -> Vec<&SugestaoDePreco> {
        let termo = self.busca.trim().to_lowercase();
        self.sugestoes
            .iter()
            .filter(|s| termo.is_empty() || s.nome.to_lowercase().contains(&termo))
            .collect()
    }
}

pub(super) fn aba(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
) {
    resumo(ui, &estado.prc);
    ui.add_space(Espaco::E16);
    SecaoExpansivel::nova("Parâmetros de preço da empresa").mostrar(ui, |ui| {
        parametros(ui, motor, sessao, estado);
    });
    ui.add_space(Espaco::E16);
    tabela(ui, motor, sessao, estado);
}

fn resumo(ui: &mut egui::Ui, e: &EstadoPrecos) {
    let Some(s) = e.sugestoes.first() else {
        return;
    };
    let tom = if s.custo_operacional_derivado {
        Tom::Positivo
    } else {
        Tom::Neutro
    };
    FaixaKpi::nova(vec![
        CartaoKpi::novo("Faturamento (12 meses)", s.rbt12).tom(Tom::Neutro),
        CartaoKpi::contagem(
            "Custo operacional",
            format!("{}%", pct(s.custo_operacional)),
        )
        .tom(tom)
        .variacao(if s.custo_operacional_derivado {
            "calculado pelo financeiro"
        } else {
            "informado por você"
        }),
        CartaoKpi::contagem("Imposto sobre a venda", format!("{}%", pct(s.imposto)))
            .tom(Tom::Atencao)
            .variacao("do primeiro produto da lista"),
    ])
    .mostrar(ui);
}

fn parametros(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoTelaEstoque) {
    let e = &mut estado.prc;
    ui.add(Rotulo::apoio(
        "O preço sugerido é: custo ÷ (100% − custo operacional − margem − imposto − comissão − taxas). \
         Alterar estes parâmetros exige permissão de perfil tributário.",
    ));
    ui.add_space(Espaco::E8);
    ui.add(Rotulo::campo("Regime tributário"));
    if let Some(r) = Abas::nova(&[
        (Regime::Mei, "MEI"),
        (Regime::Simples, "Simples Nacional"),
        (Regime::Manual, "Percentual informado"),
    ])
    .selecionada(e.regime)
    .id_salt("prc-regime")
    .mostrar(ui)
    {
        e.regime = r;
    }
    ui.add_space(Espaco::E8);
    match e.regime {
        Regime::Mei => {
            ui.add(Rotulo::apoio(
                "O MEI paga um valor fixo (DAS): nenhum percentual entra no preço. Lance o DAS \
                 como despesa para ele compor o custo operacional.",
            ));
        }
        Regime::Simples => {
            ui.add(Rotulo::campo("Anexo"));
            let ids: Vec<(u8, &str)> = vec![
                (0, "I · Comércio"),
                (1, "II · Indústria"),
                (2, "III · Serviços"),
                (3, "IV · Serviços"),
                (4, "V · Serviços"),
            ];
            if let Some(a) = Abas::nova(&ids)
                .selecionada(e.anexo)
                .id_salt("prc-anexo")
                .mostrar(ui)
            {
                e.anexo = a;
            }
            ui.add(Rotulo::apoio(
                "A alíquota efetiva sai do faturamento dos últimos 12 meses lido do financeiro. \
                 Acima de R$ 3,6 milhões o ICMS/ISS deixa o DAS: use \"Percentual informado\".",
            ));
        }
        Regime::Manual => {
            ui.add(
                Campo::novo("Imposto sobre a venda (%)", &mut e.imposto_manual)
                    .marcador("ICMS + PIS + COFINS (+ ISS), ex.: 18"),
            );
        }
    }
    ui.add_space(Espaco::E12);
    ui.columns(3, |c| {
        c[0].add(Campo::novo("Custo operacional (%)", &mut e.opex));
        c[1].add(Campo::novo("Comissão média (%)", &mut e.comissao));
        c[2].add(Campo::novo("Taxa de cartão/PIX (%)", &mut e.taxa));
    });
    ui.add_space(Espaco::E8);
    ui.add(cardeal_ui::atoms::Caixa::nova(
        &mut e.opex_derivado,
        "Calcular o custo operacional pelo financeiro (despesas ÷ faturamento dos últimos 12 meses)",
    ));
    ui.add_space(Espaco::E8);
    ui.columns(2, |c| {
        c[0].add(
            Campo::novo("Margem padrão (%)", &mut e.margem_padrao)
                .marcador("para o que não tem margem própria"),
        );
        c[1].add(Campo::novo("Margem mínima (%)", &mut e.margem_minima));
    });
    ui.add_space(Espaco::E8);
    ui.add(cardeal_ui::atoms::Caixa::nova(
        &mut e.obrigatorio,
        "O preço sugerido é obrigatório no PDV (desligado, ele só orienta)",
    ));
    ui.add_space(Espaco::E12);
    if ui.add(Botao::primario("Salvar parâmetros")).clicked() {
        match e.montar() {
            Ok(p) => match motor.executar(
                sessao,
                "estoque.definir_parametros_preco.v1",
                &DefinirParametrosPreco { parametros: p },
            ) {
                Ok(()) => {
                    estado.prc.carregar(motor, sessao);
                    notificar(ui.ctx(), Notificacao::sucesso("Parâmetros salvos"));
                }
                Err(err) => notificar(ui.ctx(), Notificacao::erro(err.mensagem)),
            },
            Err(m) => notificar(ui.ctx(), Notificacao::erro(m)),
        }
    }
}

fn tabela(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoTelaEstoque) {
    let mut recarregar = false;
    let mut abrir_aplicar = false;
    let ops: Vec<(Id, String)> = estado.est.nos_para_seletor().into_iter().collect();
    BarraFiltros::nova(&mut estado.prc.busca)
        .marcador("Buscar produto")
        .filtro(|ui| {
            let antes = estado.prc.grupo;
            SeletorOpcao::novo("Grupo", &mut estado.prc.grupo)
                .opcoes(ops.clone())
                .placeholder("Todos os grupos")
                .sem_rotulo()
                .mostrar(ui);
            if antes != estado.prc.grupo {
                recarregar = true;
            }
        })
        .acao(|ui| {
            if ui
                .add(Botao::primario("Aplicar à tabela de preço…"))
                .clicked()
            {
                abrir_aplicar = true;
            }
        })
        .mostrar(ui);
    if abrir_aplicar {
        estado.prc.abrir_aplicar(motor, sessao);
        estado.dlg = Dlg::AplicarPrecos;
    }
    if recarregar {
        estado.prc.carregar(motor, sessao);
    }
    ui.add_space(Espaco::E8);

    let linhas = estado.prc.filtradas();
    Grade::nova(vec![
        ColunaGrade::nova("Produto").largura(240.0),
        ColunaGrade::nova("Custo").largura(100.0).numero(),
        ColunaGrade::nova("Margem").largura(150.0),
        ColunaGrade::nova("Imposto").largura(80.0).numero(),
        ColunaGrade::nova("Preço mínimo").largura(120.0).numero(),
        ColunaGrade::nova("Preço sugerido").largura(130.0).numero(),
        ColunaGrade::nova("Situação").largura(260.0),
    ])
    .id_salt("precos-sugeridos")
    .vazio("Nenhum produto neste recorte.")
    .mostrar(ui, linhas.len(), |i, row| {
        let s = linhas[i];
        row.col(|ui| {
            ui.add(Rotulo::interface(s.nome.clone()));
        });
        row.col(|ui| celula(ui, s.custo));
        row.col(|ui| match (&s.margem, &s.origem_margem) {
            (Some(m), origem) => {
                let de = match origem {
                    OrigemMargem::Produto => "do produto".to_owned(),
                    OrigemMargem::Grupo(g) => format!("de {g}"),
                    OrigemMargem::Padrao => "padrão".to_owned(),
                    OrigemMargem::Nenhuma => String::new(),
                };
                ui.add(Rotulo::interface(format!("{}% {de}", pct(*m))));
            }
            _ => {
                ui.add(Etiqueta::atencao("sem margem"));
            }
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(format!("{}%", pct(s.imposto))));
        });
        row.col(|ui| celula(ui, s.minimo.map(|m| m.preco)));
        row.col(|ui| celula(ui, s.sugerido.map(|m| m.preco)));
        row.col(|ui| match &s.problema {
            Some(p) => {
                ui.add(Rotulo::apoio(p.clone()).cor(ui.cores().atencao));
            }
            None => {
                ui.add(Etiqueta::positiva("pronto"));
            }
        });
    });
}

fn celula(ui: &mut egui::Ui, v: Option<Dinheiro>) {
    match v {
        Some(v) => {
            ui.add(ValorDinheiro::novo(v).neutro());
        }
        None => {
            ui.add(Rotulo::interface("—").cor(ui.cores().texto_fraco));
        }
    }
}

/// O diálogo que leva os preços sugeridos da lista (já filtrada) para uma tabela de preço.
pub(super) fn dialogo_aplicar(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
) {
    let prontos: Vec<Id> = estado
        .prc
        .filtradas()
        .iter()
        .filter(|s| s.sugerido.is_some())
        .map(|s| s.produto)
        .collect();
    let sem_preco = estado.prc.filtradas().len() - prontos.len();

    let fechar = Dialogo::nova("Aplicar preços sugeridos")
        .largura(560.0)
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                ui.add(Rotulo::apoio(format!(
                    "{} produto(s) da lista atual receberão o preço sugerido{}. Quem já tem preço \
                     nesta tabela é atualizado; pedidos já feitos não mudam (o preço é \
                     congelado no item).",
                    prontos.len(),
                    if sem_preco > 0 {
                        format!(" ({sem_preco} ficam de fora por falta de custo ou margem)")
                    } else {
                        String::new()
                    }
                )));
                ui.add_space(Espaco::E12);
                if estado.prc.tabelas.is_empty() {
                    ui.add(Rotulo::interface(
                        "Não há tabela de preço de venda ou atacado. Crie uma em Vendas › Tabelas de preço.",
                    ));
                    return;
                }
                let ops: Vec<(Id, String)> = estado
                    .prc
                    .tabelas
                    .iter()
                    .map(|t| (t.id, t.nome.clone()))
                    .collect();
                SeletorOpcao::novo("Tabela de preço", &mut estado.prc.tabela)
                    .opcoes(ops)
                    .mostrar(ui);
                ui.add_space(Espaco::E12);
                ui.add(Rotulo::campo("Preço de prateleira"));
                if let Some(a) = Abas::nova(&[
                    (ArredondamentoDePreco::Nenhum, "Como calculado"),
                    (ArredondamentoDePreco::FinalNoventa, "Terminar em ,90"),
                    (ArredondamentoDePreco::InteiroAcima, "Real inteiro"),
                ])
                .selecionada(estado.prc.arredondamento)
                .id_salt("prc-arred")
                .mostrar(ui)
                {
                    estado.prc.arredondamento = a;
                }
                ui.add(Rotulo::apoio(
                    "O arredondamento sempre sobe, para a margem nunca ficar abaixo da calculada.",
                ));
            },
            |ui, estado| {
                if ui.add(Botao::secundario("Cancelar")).clicked() {
                    estado.dlg = Dlg::Fechado;
                }
                let pode = estado.prc.tabela.is_some() && !prontos.is_empty();
                if ui
                    .add_enabled(pode, Botao::primario("Aplicar preços"))
                    .clicked()
                {
                    aplicar(ui.ctx(), motor, sessao, estado, prontos.clone());
                }
            },
        );
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

pub(super) fn aplicar(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
    produtos: Vec<Id>,
) {
    let Some(tabela) = estado.prc.tabela else {
        return;
    };
    match motor.executar(
        sessao,
        "vendas.aplicar_precos_sugeridos.v1",
        &AplicarPrecosSugeridos {
            tabela_preco: tabela,
            produtos,
            arredondamento: estado.prc.arredondamento,
        },
    ) {
        Ok(r) => {
            let r: PrecosAplicados = r;
            estado.dlg = Dlg::Fechado;
            let ignorados = if r.ignorados.is_empty() {
                String::new()
            } else {
                format!(", {} de fora", r.ignorados.len())
            };
            notificar(
                ctx,
                Notificacao::sucesso(format!(
                    "{} preço(s) criado(s), {} atualizado(s){ignorados}",
                    r.criadas, r.atualizadas
                )),
            );
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}
