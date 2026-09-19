//! Tela de Financeiro — Contas a Receber / a Pagar. `docs/modulos/financeiro.md`.
//!
//! Duas abas (a receber / a pagar), cada uma a lista inteira de parcelas em aberto.
//! "+ Lançar título" e o clique numa linha abrem o `Dialogo` — ver a parcela e **dar baixa**.

use std::collections::HashMap;

use cardeal_cliente::{MotorLocal, SessaoLocal};
use cardeal_kernel::{Competencia, Data, Dinheiro, Fuso, Id, Periodo};
use cardeal_ledger::Contraparte;
use cardeal_modkit::Icone;
use cardeal_ui::atoms::{Botao, Etiqueta, Rotulo, ValorDinheiro};
use cardeal_ui::molecules::{
    Abas, Campo, CartaoKpi, EstadoVazio, Mascara, SecaoExpansivel, SeletorOpcao,
};
use cardeal_ui::organisms::{
    notificar, ColunaGrade, Dialogo, Direcao, FaixaKpi, Grade, GraficoBarras,
    GraficoBarrasHorizontais, LayoutTela, Notificacao, SerieBarras,
};
use cardeal_ui::tokens::{perseguir, sombra_cartao, Espaco, Mov, Raio, Rubro, TemaUi};
use eframe::egui;
use mod_clientes::{CriarPessoa, ItemPessoa, Papel, PessoaCadastrada, PessoasPorPapel, TipoPessoa};
use mod_financeiro::{
    BaixarPagamento, BaixarRecebimento, BaixasDaParcela, CategoriaFinanceira, Categorias,
    ContaBancariaCriada, ContasDeResultado, ContasDisponiveis, CriarCategoria, CriarContaBancaria,
    CriarRecorrencia, EspecieTitulo, EstadoParcela, EstornarBaixa, ExtratoDisponivel, ItemBaixa,
    ItemContaDisponivel, ItemContaResultado, ItemMovimentoDisponivel, ItemTituloEmAberto,
    ItemTotalPorCategoria, LancarTituloAPagar, LancarTituloAReceber, MeioPagamento,
    ParcelasAPagarNoPeriodo, ParcelasAReceberNoPeriodo, Periodicidade, Recorrencia, Recorrencias,
    TipoValor, TitulosAPagarEmAberto, TitulosAReceberEmAberto, TotalPagoNoPeriodo,
    TotalPorCategoriaNoPeriodo, TotalRecebidoNoPeriodo,
};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Aba {
    #[default]
    Visao,
    Receber,
    Pagar,
    Fluxo,
    Bancos,
}

impl Aba {
    const fn a_receber(self) -> bool {
        matches!(self, Self::Receber)
    }
}

/// Um mês da série de receita × custo.
#[derive(Clone, Default)]
struct MesFluxo {
    rotulo: String,
    receita: f64,
    custo: f64,
}

#[derive(Default)]
enum Dlg {
    #[default]
    Fechado,
    Lancar(FormLancar),
    Baixar {
        indice: usize,
        valor: String,
        data: String,
        /// Dinheiro/Pix/Cartão — decide se a baixa cai no Caixa ou na conta bancária
        /// (`MeioPagamento::papel`). `None` só antes do primeiro quadro; vira `Some` já na
        /// abertura do diálogo, igual ao `filtro_status` de `tela_os`.
        meio_pagamento: Option<MeioPagamento>,
        /// O histórico de baixas da parcela — carregado uma vez na abertura do diálogo
        /// (`baixas_carregadas` marca isso, já que uma parcela recém-lançada legitimamente
        /// tem histórico vazio).
        baixas: Vec<ItemBaixa>,
        baixas_carregadas: bool,
        /// O motivo do estorno, digitado antes de clicar em "Estornar" numa baixa do
        /// histórico — `EstornarBaixa` exige um motivo não vazio.
        motivo_estorno: String,
    },
    Analise,
    Recorrencias,
    NovaRecorrencia(FormRecorrencia),
    NovaContaBancaria {
        nome: String,
    },
    NovaCategoria {
        nome: String,
        /// `None` = serve para as duas espécies.
        especie: Option<EspecieTitulo>,
    },
}

/// Para qual formulário o cadastro rápido ([`DlgRapido`]) devolve o `Id` recém-criado —
/// `dlg_rapido` é independente do `dlg` principal (um dialog pequeno empilhado por cima),
/// então precisa saber em qual campo, de qual `Dlg`, escrever o resultado.
#[derive(Clone, Copy, PartialEq, Eq)]
enum AlvoRapido {
    Lancar,
    Recorrencia,
}

/// Cadastro rápido de cliente/fornecedor ou categoria, sem sair do dialog de lançamento —
/// pedido explícito do usuário: "deve ser prático de criar um cliente por ali rapidinho".
/// Reaproveita os comandos de cadastro já existentes (`CriarPessoa`, já pensado para um
/// "cadastro de balcão" só com nome; `CriarCategoria`), só oferecendo um atalho de UI: um
/// segundo `Dialogo` menor, empilhado por cima do formulário principal (que continua aberto
/// por trás).
enum DlgRapido {
    Pessoa {
        alvo: AlvoRapido,
        papel: Papel,
        tipo: TipoPessoa,
        nome: String,
    },
    Categoria {
        alvo: AlvoRapido,
        nome: String,
        especie: Option<EspecieTitulo>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PeriodoRec {
    Mensal,
    Semanal,
    Anual,
}

impl PeriodoRec {
    const fn dominio(self) -> Periodicidade {
        match self {
            Self::Mensal => Periodicidade::Mensal,
            Self::Semanal => Periodicidade::Semanal,
            Self::Anual => Periodicidade::Anual,
        }
    }
}

/// O período pré-definido do filtro de Contas a Receber/Pagar (`FiltroPeriodo`) — pedido
/// explícito do usuário: dia, semana, mês, trimestre, semestre, ano, ou uma faixa digitada
/// à mão.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum PresetPeriodo {
    Dia,
    Semana,
    #[default]
    Mes,
    Trimestre,
    Semestre,
    Ano,
    Personalizado,
}

/// O filtro de período das abas "A receber"/"A pagar" — um preset de calendário (o mais
/// comum) ou uma faixa `De`/`Até` digitada, quando `preset == Personalizado`.
#[derive(Default)]
struct FiltroPeriodo {
    preset: PresetPeriodo,
    de_personalizado: String,
    ate_personalizado: String,
}

impl FiltroPeriodo {
    /// Resolve o preset (ou a faixa personalizada) num [`Periodo`] concreto, sempre a
    /// unidade de calendário inteira — não só "até hoje": um preset de "Mês" cobre o mês
    /// inteiro (passado e futuro), porque a projeção precisa enxergar até o fim dele.
    fn resolver(&self, hoje: Data) -> Periodo {
        match self.preset {
            PresetPeriodo::Dia => Periodo::novo(hoje, hoje),
            PresetPeriodo::Semana => {
                let inicio = hoje.mais_dias(-(hoje.dia_da_semana() as i32));
                Periodo::novo(inicio, inicio.mais_dias(6))
            }
            PresetPeriodo::Mes => Periodo::novo(hoje.inicio_do_mes(), hoje.fim_do_mes()),
            PresetPeriodo::Trimestre => {
                let mes_inicio = ((hoje.mes() - 1) / 3) * 3 + 1;
                let inicio = Data::de_ymd(hoje.ano(), mes_inicio, 1)
                    .unwrap_or(hoje)
                    .inicio_do_mes();
                Periodo::novo(inicio, inicio.mais_meses(2).fim_do_mes())
            }
            PresetPeriodo::Semestre => {
                let mes_inicio = if hoje.mes() <= 6 { 1 } else { 7 };
                let inicio = Data::de_ymd(hoje.ano(), mes_inicio, 1)
                    .unwrap_or(hoje)
                    .inicio_do_mes();
                Periodo::novo(inicio, inicio.mais_meses(5).fim_do_mes())
            }
            PresetPeriodo::Ano => {
                let inicio = hoje.inicio_do_ano();
                Periodo::novo(inicio, inicio.mais_meses(11).fim_do_mes())
            }
            PresetPeriodo::Personalizado => {
                let de = self.de_personalizado.parse::<Data>().unwrap_or(hoje);
                let ate = self.ate_personalizado.parse::<Data>().unwrap_or(hoje);
                Periodo::novo(de.min(ate), de.max(ate))
            }
        }
    }
}

struct FormRecorrencia {
    a_receber: bool,
    descricao: String,
    contraparte: Option<Id>,
    conta: Option<Id>,
    categoria: Option<Id>,
    valor: String,
    periodicidade: PeriodoRec,
    dia_referencia: String,
    inicio: String,
    fim: String,
    antecedencia: String,
}

impl Default for FormRecorrencia {
    fn default() -> Self {
        Self {
            a_receber: false,
            descricao: String::new(),
            contraparte: None,
            conta: None,
            categoria: None,
            valor: String::new(),
            periodicidade: PeriodoRec::Mensal,
            dia_referencia: "10".to_owned(),
            inicio: Data::hoje(Fuso::BRASILIA).to_string(),
            fim: String::new(),
            antecedencia: "5".to_owned(),
        }
    }
}

struct FormLancar {
    contraparte: Option<Id>,
    valor: String,
    emissao: String,
    parcelas: String,
    primeiro_vencimento: String,
    intervalo: String,
    observacao: String,
    categoria: Option<Id>,
}

impl Default for FormLancar {
    fn default() -> Self {
        let hoje = Data::hoje(Fuso::BRASILIA).to_string();
        Self {
            contraparte: None,
            valor: String::new(),
            emissao: hoje.clone(),
            parcelas: "1".to_owned(),
            primeiro_vencimento: hoje,
            intervalo: "30".to_owned(),
            observacao: String::new(),
            categoria: None,
        }
    }
}

/// Estado local da tela.
#[derive(Default)]
pub struct EstadoTelaFinanceiro {
    aba: Aba,
    /// As parcelas da aba ativa (Receber/Pagar) com vencimento em `periodo` — **qualquer
    /// estado**, não só em aberto (pedido explícito do usuário: a aba deve mostrar também o
    /// que já foi pago/recebido, igual ao histórico do fluxo de caixa).
    parcelas: Vec<ItemTituloEmAberto>,
    /// O filtro de período das abas Receber/Pagar — compartilhado entre as duas (mesmo
    /// padrão de `parcelas`: um só conjunto de campos, recarregado a cada troca de aba).
    periodo: FiltroPeriodo,
    /// Quanto foi efetivamente recebido/pago dentro de `periodo` (soma de baixas, não de
    /// vencimento) — o card "Recebido/Pago no período".
    baixado_periodo: Dinheiro,
    /// Saldo em aberto (qualquer vencimento) que já vence até o fim de `periodo` — o card
    /// de projeção: "até `periodo.ate` você vai receber/pagar X, baseado no que já está em
    /// aberto hoje".
    projecao_periodo: Dinheiro,
    busca: String,
    ordenacao: Option<(usize, Direcao)>,
    nomes: HashMap<Id, String>,
    clientes: Vec<ItemPessoa>,
    fornecedores: Vec<ItemPessoa>,
    categorias: Vec<CategoriaFinanceira>,
    analise: Vec<ItemTotalPorCategoria>,
    analise_meses: u8,
    recorrencias: Vec<Recorrencia>,
    contas: Vec<ItemContaResultado>,
    contas_receber: bool,
    // Dashboard ("Visão geral").
    dash_receber: Dinheiro,
    dash_pagar: Dinheiro,
    dash_venc_receber: (usize, Dinheiro),
    dash_venc_pagar: (usize, Dinheiro),
    dash_recebido_mes: Dinheiro,
    dash_pago_mes: Dinheiro,
    serie: Vec<MesFluxo>,
    // Fluxo de caixa / bancos.
    fluxo_dias: i64,
    extrato: Vec<ItemMovimentoDisponivel>,
    /// Rótulo amigável por `origem_id` ("OS #123 — João Silva"), resolvido uma vez ao
    /// carregar o extrato — pedido explícito do usuário: o fluxo de caixa deve dizer de onde
    /// veio o dinheiro. Resolvido aqui na UI, nunca no financeiro (`docs/contratos-internos.md`
    /// §7 regra 2 — o financeiro só sabe `origem_modulo`/`origem_id`, não o que é uma OS).
    origem_labels: HashMap<Id, String>,
    contas_disp: Vec<ItemContaDisponivel>,
    erro: Option<String>,
    dlg: Dlg,
    /// Cadastro rápido (cliente/fornecedor/categoria) aberto por cima do `dlg` principal —
    /// ver [`DlgRapido`].
    dlg_rapido: Option<DlgRapido>,
}

impl EstadoTelaFinanceiro {
    /// Recarrega a lista da aba ativa, o painel de visão geral e o índice de nomes.
    pub fn carregar(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        self.erro = None;
        if matches!(self.aba, Aba::Receber | Aba::Pagar) {
            self.carregar_parcelas_periodo(motor, sessao);
        }
        self.carregar_dashboard(motor, sessao);
        if matches!(self.aba, Aba::Fluxo | Aba::Bancos) {
            self.carregar_fluxo(motor, sessao);
        }

        if let Ok(c) = motor.consultar(
            sessao,
            "clientes.pessoas_por_papel.v1",
            &PessoasPorPapel {
                papel: Papel::Cliente,
                busca: None,
            },
        ) {
            self.clientes = c;
        }
        if let Ok(f) = motor.consultar(
            sessao,
            "clientes.pessoas_por_papel.v1",
            &PessoasPorPapel {
                papel: Papel::Fornecedor,
                busca: None,
            },
        ) {
            self.fornecedores = f;
        }
        self.carregar_categorias(motor, sessao);
        self.nomes = self
            .clientes
            .iter()
            .chain(self.fornecedores.iter())
            .map(|p| (p.pessoa, p.nome.clone()))
            .collect();
    }

    /// Recarrega a listagem (qualquer estado, dentro do período selecionado) e os dois
    /// cards da aba ativa (Receber/Pagar): quanto foi baixado no período, e a projeção do
    /// saldo em aberto que já vence até o fim do período.
    fn carregar_parcelas_periodo(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        let hoje = Data::hoje(Fuso::BRASILIA);
        let periodo = self.periodo.resolver(hoje);
        let a_receber = self.aba.a_receber();

        let r = if a_receber {
            motor.consultar(
                sessao,
                "financeiro.parcelas_a_receber_no_periodo.v1",
                &ParcelasAReceberNoPeriodo { periodo },
            )
        } else {
            motor.consultar(
                sessao,
                "financeiro.parcelas_a_pagar_no_periodo.v1",
                &ParcelasAPagarNoPeriodo { periodo },
            )
        };
        match r {
            Ok(p) => self.parcelas = p,
            Err(e) => self.erro = Some(e.mensagem),
        }

        self.baixado_periodo = if a_receber {
            motor
                .consultar(
                    sessao,
                    "financeiro.total_recebido_no_periodo.v1",
                    &TotalRecebidoNoPeriodo { periodo },
                )
                .unwrap_or(Dinheiro::ZERO)
        } else {
            motor
                .consultar(
                    sessao,
                    "financeiro.total_pago_no_periodo.v1",
                    &TotalPagoNoPeriodo { periodo },
                )
                .unwrap_or(Dinheiro::ZERO)
        };

        // Projeção: soma do saldo em aberto (qualquer vencimento, inclusive já vencido) que
        // já vence até o fim do período — usa a consulta "em aberto" de sempre (sem filtro
        // de período nenhum), só filtrada aqui pela data-fim.
        let abertos: Vec<ItemTituloEmAberto> = if a_receber {
            motor
                .consultar(
                    sessao,
                    "financeiro.titulos_a_receber_em_aberto.v1",
                    &TitulosAReceberEmAberto,
                )
                .unwrap_or_default()
        } else {
            motor
                .consultar(
                    sessao,
                    "financeiro.titulos_a_pagar_em_aberto.v1",
                    &TitulosAPagarEmAberto,
                )
                .unwrap_or_default()
        };
        self.projecao_periodo = abertos
            .iter()
            .filter(|p| p.vencimento <= periodo.ate)
            .fold(Dinheiro::ZERO, |acc, p| acc + p.saldo());
    }

    /// Alimenta o painel de visão geral: saldos em aberto, o que vence em 7 dias e a série
    /// de recebido × pago dos últimos 6 meses.
    fn carregar_dashboard(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        let hoje = Data::hoje(Fuso::BRASILIA);
        let em7 = hoje.mais_dias(7);

        let receber = motor
            .consultar(
                sessao,
                "financeiro.titulos_a_receber_em_aberto.v1",
                &TitulosAReceberEmAberto,
            )
            .unwrap_or_default();
        let pagar = motor
            .consultar(
                sessao,
                "financeiro.titulos_a_pagar_em_aberto.v1",
                &TitulosAPagarEmAberto,
            )
            .unwrap_or_default();

        let soma = |v: &[ItemTituloEmAberto]| {
            v.iter()
                .map(ItemTituloEmAberto::saldo)
                .fold(Dinheiro::ZERO, |a, b| a + b)
        };
        let venc = |v: &[ItemTituloEmAberto]| {
            let atrasa_ou_vence: Vec<&ItemTituloEmAberto> =
                v.iter().filter(|p| p.vencimento <= em7).collect();
            let total = atrasa_ou_vence
                .iter()
                .map(|p| p.saldo())
                .fold(Dinheiro::ZERO, |a, b| a + b);
            (atrasa_ou_vence.len(), total)
        };
        self.dash_receber = soma(&receber);
        self.dash_pagar = soma(&pagar);
        self.dash_venc_receber = venc(&receber);
        self.dash_venc_pagar = venc(&pagar);

        let periodo = Periodo::novo(hoje.mais_meses(-5).inicio_do_mes(), hoje);
        let itens = motor
            .consultar(
                sessao,
                "financeiro.total_por_categoria_no_periodo.v1",
                &TotalPorCategoriaNoPeriodo { periodo },
            )
            .unwrap_or_default();

        let mut comp = hoje.mais_meses(-5).competencia();
        let mut serie = Vec::with_capacity(6);
        for _ in 0..6 {
            let centavos = |a_receber: bool| -> i64 {
                itens
                    .iter()
                    .filter(|it| {
                        it.competencia == comp
                            && matches!(it.especie, EspecieTitulo::Receber) == a_receber
                    })
                    .map(|it| it.total_baixado.em_centavos())
                    .sum()
            };
            serie.push(MesFluxo {
                rotulo: nome_mes(comp),
                receita: centavos(true) as f64 / 100.0,
                custo: centavos(false) as f64 / 100.0,
            });
            comp = comp.proxima();
        }
        self.dash_recebido_mes =
            Dinheiro::centavos((serie.last().map_or(0.0, |m| m.receita) * 100.0).round() as i64);
        self.dash_pago_mes =
            Dinheiro::centavos((serie.last().map_or(0.0, |m| m.custo) * 100.0).round() as i64);
        self.serie = serie;

        // Também alimenta o recorte "por categoria" já visível na Visão Geral (antes só
        // aparecia depois de clicar em "Ver por categoria") — mesma janela de 6 meses.
        self.carregar_analise(motor, sessao);
    }

    /// As 5 categorias de maior movimento (receita − despesa, em módulo) nos últimos
    /// [`Self::analise_meses`] meses — o recorte "Por categoria" da Visão Geral.
    fn top_categorias(&self) -> Vec<cardeal_ui::organisms::ItemBarraHorizontal> {
        let mut por_cat: HashMap<Option<Id>, f64> = HashMap::new();
        for it in &self.analise {
            let sinal = match it.especie {
                EspecieTitulo::Receber => 1.0,
                EspecieTitulo::Pagar => -1.0,
            };
            #[allow(clippy::cast_precision_loss)]
            let valor = it.total_baixado.em_centavos() as f64 / 100.0 * sinal;
            *por_cat.entry(it.categoria).or_insert(0.0) += valor;
        }
        let mut itens: Vec<cardeal_ui::organisms::ItemBarraHorizontal> = por_cat
            .into_iter()
            .map(|(cat, valor)| cardeal_ui::organisms::ItemBarraHorizontal {
                rotulo: self.nome_categoria(cat),
                valor,
            })
            .collect();
        itens.sort_by(|a, b| b.valor.abs().total_cmp(&a.valor.abs()));
        itens.truncate(5);
        itens
    }

    fn carregar_fluxo(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        if self.fluxo_dias == 0 {
            self.fluxo_dias = 30;
        }
        let hoje = Data::hoje(Fuso::BRASILIA);
        let periodo = Periodo::novo(
            hoje.mais_dias(-i32::try_from(self.fluxo_dias).unwrap_or(30)),
            hoje,
        );
        match motor.consultar(
            sessao,
            "financeiro.extrato_disponivel.v1",
            &ExtratoDisponivel {
                periodo,
                conta: None,
            },
        ) {
            Ok(v) => {
                self.extrato = v;
                self.carregar_origens_do_extrato(motor, sessao);
            }
            Err(e) => self.erro = Some(e.mensagem),
        }
        match motor.consultar(
            sessao,
            "financeiro.contas_disponiveis.v1",
            &ContasDisponiveis,
        ) {
            Ok(v) => self.contas_disp = v,
            Err(e) => self.erro = Some(e.mensagem),
        }
    }

    /// Resolve um rótulo amigável para cada origem distinta do extrato recém-carregado
    /// (`self.origem_labels`, consumido por [`rotulo_origem`]). Só `"os"` por enquanto — o
    /// mesmo padrão serve para `"vendas"`/`"compras"` quando a tela ganhar consulta
    /// equivalente para eles.
    fn carregar_origens_do_extrato(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        let ids: Vec<Id> = self
            .extrato
            .iter()
            .filter(|m| m.origem_modulo.as_deref() == Some("os"))
            .filter_map(|m| m.origem_id)
            .filter(|id| !self.origem_labels.contains_key(id))
            .collect();
        for id in ids {
            let resultado: Result<Option<mod_os::DetalheOrdem>, _> = motor.consultar(
                sessao,
                "os.buscar_detalhe_ordem.v1",
                &mod_os::BuscarDetalheOrdem { ordem_servico: id },
            );
            if let Ok(Some(d)) = resultado {
                let cliente = self.nomes.get(&d.ordem.cliente).cloned();
                let rotulo = match cliente {
                    Some(nome) => format!("OS #{} — {nome}", d.ordem.numero),
                    None => format!("OS #{}", d.ordem.numero),
                };
                self.origem_labels.insert(id, rotulo);
            }
        }
    }

    /// O rótulo de origem de um movimento do extrato, se resolvido (ver
    /// [`carregar_origens_do_extrato`]).
    fn rotulo_origem(&self, m: &ItemMovimentoDisponivel) -> Option<&str> {
        m.origem_id
            .and_then(|id| self.origem_labels.get(&id))
            .map(String::as_str)
    }

    fn carregar_analise(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        if self.analise_meses == 0 {
            self.analise_meses = 6;
        }
        let hoje = Data::hoje(Fuso::BRASILIA);
        let periodo = Periodo::novo(hoje.mais_meses(-i32::from(self.analise_meses)), hoje);
        match motor.consultar(
            sessao,
            "financeiro.total_por_categoria_no_periodo.v1",
            &TotalPorCategoriaNoPeriodo { periodo },
        ) {
            Ok(v) => self.analise = v,
            Err(e) => self.erro = Some(e.mensagem),
        }
    }

    fn carregar_recorrencias(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        match motor.consultar(sessao, "financeiro.recorrencias.v1", &Recorrencias) {
            Ok(v) => self.recorrencias = v,
            Err(e) => self.erro = Some(e.mensagem),
        }
    }

    /// Contas de resultado da espécie pedida — Receitas p/ recorrência a receber, Despesas
    /// p/ a pagar. O seletor de `conta_contrapartida` do formulário.
    fn carregar_contas(&mut self, motor: &MotorLocal, sessao: &SessaoLocal, a_receber: bool) {
        let especie = if a_receber {
            EspecieTitulo::Receber
        } else {
            EspecieTitulo::Pagar
        };
        match motor.consultar(
            sessao,
            "financeiro.contas_de_resultado.v1",
            &ContasDeResultado { especie },
        ) {
            Ok(v) => {
                self.contas = v;
                self.contas_receber = a_receber;
            }
            Err(e) => self.erro = Some(e.mensagem),
        }
    }

    /// Recarrega só as categorias — usado depois de criar uma nova, sem repetir o resto do
    /// que `carregar` busca.
    fn carregar_categorias(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        match motor.consultar(sessao, "financeiro.categorias.v1", &Categorias) {
            Ok(v) => self.categorias = v,
            Err(e) => self.erro = Some(e.mensagem),
        }
    }

    /// Índices de `parcelas` cujo nome de contraparte bate com `busca` — filtro client-side,
    /// mesmo racional de `tela_estoque.rs::produtos_filtrados`: a consulta já trouxe até 500
    /// linhas de uma vez, e o volume de uma assistência cabe folgado nisso.
    fn parcelas_filtradas(&self) -> Vec<usize> {
        let termo = self.busca.trim().to_lowercase();
        (0..self.parcelas.len())
            .filter(|&i| {
                termo.is_empty()
                    || self
                        .nome_contraparte(&self.parcelas[i].contraparte)
                        .to_lowercase()
                        .contains(&termo)
            })
            .collect()
    }

    fn nome_categoria(&self, id: Option<Id>) -> String {
        id.and_then(|i| self.categorias.iter().find(|c| c.id == i))
            .map_or_else(|| "Sem categoria".to_owned(), |c| c.nome.clone())
    }

    fn nome_contraparte(&self, c: &Option<Contraparte>) -> String {
        let Some(c) = c else {
            return "Sem cliente/fornecedor".to_owned();
        };
        let id = match c {
            Contraparte::Cliente(i)
            | Contraparte::Fornecedor(i)
            | Contraparte::Funcionario(i)
            | Contraparte::Socio(i)
            | Contraparte::Outro(i) => *i,
        };
        self.nomes
            .get(&id)
            .cloned()
            .unwrap_or_else(|| "—".to_owned())
    }
}

/// Desenha a tela inteira.
pub fn mostrar(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    LayoutTela::nova("Financeiro").mostrar(
        ui,
        estado,
        |ui, estado| {
            let rot = if estado.aba.a_receber() {
                "+ Lançar a receber"
            } else {
                "+ Lançar a pagar"
            };
            if ui.add(Botao::primario(rot).atalho("Ctrl+N")).clicked() {
                estado.dlg = Dlg::Lancar(FormLancar::default());
            }
            if ui.add(Botao::secundario("Por categoria")).clicked() {
                estado.carregar_analise(motor, sessao);
                estado.dlg = Dlg::Analise;
            }
            if ui.add(Botao::secundario("Recorrências")).clicked() {
                estado.carregar_recorrencias(motor, sessao);
                estado.dlg = Dlg::Recorrencias;
            }
            if ui.add(Botao::secundario("+ Categoria")).clicked() {
                estado.dlg = Dlg::NovaCategoria {
                    nome: String::new(),
                    especie: None,
                };
            }
        },
        |ui, estado| {
            abas(ui, motor, sessao, estado);
            ui.add_space(Espaco::E16);

            if let Some(erro) = &estado.erro {
                ui.add(
                    Rotulo::interface(erro.clone())
                        .quebravel()
                        .cor(ui.cores().negativo),
                );
                ui.add_space(Espaco::E12);
            }
            match estado.aba {
                Aba::Visao => painel_visao(ui, motor, sessao, estado),
                Aba::Fluxo => painel_fluxo(ui, motor, sessao, estado),
                Aba::Bancos => painel_bancos(ui, estado),
                _ => lista(ui, motor, sessao, estado),
            }
        },
    );

    match estado.dlg {
        Dlg::Fechado => {}
        Dlg::Lancar(_) => dialogo_lancar(ui.ctx(), motor, sessao, estado),
        Dlg::Baixar { .. } => dialogo_baixar(ui.ctx(), motor, sessao, estado),
        Dlg::Analise => dialogo_analise(ui.ctx(), motor, sessao, estado),
        Dlg::Recorrencias => dialogo_recorrencias(ui.ctx(), motor, sessao, estado),
        Dlg::NovaRecorrencia(_) => dialogo_nova_recorrencia(ui.ctx(), motor, sessao, estado),
        Dlg::NovaContaBancaria { .. } => dialogo_conta_bancaria(ui.ctx(), motor, sessao, estado),
        Dlg::NovaCategoria { .. } => dialogo_categoria(ui.ctx(), motor, sessao, estado),
    }
    if estado.dlg_rapido.is_some() {
        dialogo_rapido(ui.ctx(), motor, sessao, estado);
    }
}

/// Um botão pequeno e discreto — sem preenchimento nem contorno em repouso, na cor da marca
/// — para abrir um cadastro rápido ([`DlgRapido`]) logo abaixo do seletor que ele
/// complementa.
fn botao_cadastro_rapido(rotulo: &str) -> Botao {
    Botao::fantasma(rotulo).pequeno().cor(Rubro::R500)
}

/// O cadastro rápido — um segundo `Dialogo`, menor, empilhado por cima do `dlg` principal
/// (que continua aberto por trás). Ver [`DlgRapido`].
fn dialogo_rapido(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    let Some(rapido) = &estado.dlg_rapido else {
        return;
    };
    let titulo = match rapido {
        DlgRapido::Pessoa {
            papel: Papel::Fornecedor,
            ..
        } => "Novo fornecedor",
        DlgRapido::Pessoa { .. } => "Novo cliente",
        DlgRapido::Categoria { .. } => "Nova categoria",
    };

    let fechar = Dialogo::nova(titulo).largura(420.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            let Some(rapido) = &mut estado.dlg_rapido else {
                return;
            };
            match rapido {
                DlgRapido::Pessoa { tipo, nome, .. } => {
                    ui.horizontal(|ui| {
                        for (rot, t) in [
                            ("Pessoa física", TipoPessoa::Fisica),
                            ("Pessoa jurídica", TipoPessoa::Juridica),
                        ] {
                            let sel = *tipo == t;
                            let b = if sel {
                                Botao::primario(rot)
                            } else {
                                Botao::fantasma(rot)
                            };
                            if ui.add(b).clicked() {
                                *tipo = t;
                            }
                        }
                    });
                    ui.add_space(Espaco::E12);
                    ui.add(Campo::novo("Nome", nome).marcador("Nome completo ou razão social"));
                }
                DlgRapido::Categoria { nome, especie, .. } => {
                    ui.add(Campo::novo("Nome", nome).marcador("Aluguel"));
                    ui.add_space(Espaco::E12);
                    ui.horizontal(|ui| {
                        ui.add(Rotulo::campo("Serve para"));
                        for (rot, valor) in [
                            ("Ambas", None),
                            ("Só a receber", Some(EspecieTitulo::Receber)),
                            ("Só a pagar", Some(EspecieTitulo::Pagar)),
                        ] {
                            let sel = *especie == valor;
                            let b = if sel {
                                Botao::primario(rot)
                            } else {
                                Botao::fantasma(rot)
                            };
                            if ui.add(b).clicked() {
                                *especie = valor;
                            }
                        }
                    });
                }
            }
        },
        |ui, estado| {
            if ui.add(Botao::primario("Criar")).clicked() {
                criar_rapido(ui.ctx(), motor, sessao, estado);
            }
            if ui.add(Botao::secundario("Cancelar")).clicked() {
                estado.dlg_rapido = None;
            }
        },
    );
    if fechar {
        estado.dlg_rapido = None;
    }
}

/// Cria a pessoa/categoria do cadastro rápido e escreve o `Id` de volta no formulário
/// principal ([`AlvoRapido`]) que pediu o cadastro.
fn criar_rapido(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    let Some(rapido) = &estado.dlg_rapido else {
        return;
    };
    match rapido {
        DlgRapido::Pessoa {
            alvo,
            papel,
            tipo,
            nome,
        } => {
            let nome = nome.trim().to_owned();
            if nome.is_empty() {
                notificar(ctx, Notificacao::aviso("Informe o nome."));
                return;
            }
            let (alvo, papel, tipo) = (*alvo, *papel, *tipo);
            let r = motor
                .executar(
                    sessao,
                    "clientes.criar_pessoa.v1",
                    &CriarPessoa {
                        tipo,
                        nome,
                        nome_fantasia: None,
                        papel_inicial: papel,
                        documento_tipo: None,
                        documento_numero: None,
                        data_nascimento: None,
                        endereco: None,
                        contato: None,
                    },
                )
                .map(|p: PessoaCadastrada| p.pessoa);
            match r {
                Ok(id) => {
                    estado.dlg_rapido = None;
                    estado.carregar(motor, sessao);
                    match (alvo, &mut estado.dlg) {
                        (AlvoRapido::Lancar, Dlg::Lancar(f)) => f.contraparte = Some(id),
                        (AlvoRapido::Recorrencia, Dlg::NovaRecorrencia(f)) => {
                            f.contraparte = Some(id);
                        }
                        _ => {}
                    }
                    notificar(
                        ctx,
                        Notificacao::sucesso(if papel == Papel::Fornecedor {
                            "Fornecedor cadastrado"
                        } else {
                            "Cliente cadastrado"
                        }),
                    );
                }
                Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
            }
        }
        DlgRapido::Categoria {
            alvo,
            nome,
            especie,
        } => {
            let nome = nome.trim().to_owned();
            if nome.is_empty() {
                notificar(ctx, Notificacao::aviso("Informe o nome da categoria."));
                return;
            }
            let (alvo, especie) = (*alvo, *especie);
            let r = motor
                .executar(
                    sessao,
                    "financeiro.criar_categoria.v1",
                    &CriarCategoria { nome, especie },
                )
                .map(|c: mod_financeiro::CategoriaCriada| c.categoria);
            match r {
                Ok(id) => {
                    estado.dlg_rapido = None;
                    estado.carregar_categorias(motor, sessao);
                    match (alvo, &mut estado.dlg) {
                        (AlvoRapido::Lancar, Dlg::Lancar(f)) => f.categoria = Some(id),
                        (AlvoRapido::Recorrencia, Dlg::NovaRecorrencia(f)) => {
                            f.categoria = Some(id);
                        }
                        _ => {}
                    }
                    notificar(ctx, Notificacao::sucesso("Categoria criada"));
                }
                Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
            }
        }
    }
}

fn dialogo_analise(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    // Agrega o que veio (categoria × mês) em uma linha por categoria: receita, custo, saldo.
    let mut por_cat: std::collections::BTreeMap<String, (Dinheiro, Dinheiro)> =
        std::collections::BTreeMap::new();
    for it in &estado.analise {
        let nome = estado.nome_categoria(it.categoria);
        let e = por_cat
            .entry(nome)
            .or_insert((Dinheiro::ZERO, Dinheiro::ZERO));
        match it.especie {
            EspecieTitulo::Receber => e.0 += it.total_baixado,
            EspecieTitulo::Pagar => e.1 += it.total_baixado,
        }
    }
    let linhas: Vec<(String, Dinheiro, Dinheiro)> =
        por_cat.into_iter().map(|(k, (r, c))| (k, r, c)).collect();
    let meses = estado.analise_meses;

    let fechar = Dialogo::nova("Onde o dinheiro entrou e saiu")
        .largura(720.0)
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                ui.horizontal(|ui| {
                    for m in [3_u8, 6, 12] {
                        let b = if meses == m {
                            Botao::primario(format!("{m} meses"))
                        } else {
                            Botao::fantasma(format!("{m} meses"))
                        };
                        if ui.add(b).clicked() {
                            estado.analise_meses = m;
                            estado.carregar_analise(motor, sessao);
                        }
                    }
                });
                ui.add_space(Espaco::E12);

                if linhas.is_empty() {
                    ui.add(Rotulo::campo(
                        "Nada baixado com categoria no período — categorize os títulos ao lançar.",
                    ));
                    return;
                }

                let colunas = vec![
                    ColunaGrade::nova("Categoria"),
                    ColunaGrade::nova("Recebido").largura(140.0).numero(),
                    ColunaGrade::nova("Pago").largura(140.0).numero(),
                    ColunaGrade::nova("Saldo").largura(140.0).numero(),
                ];
                Grade::nova(colunas).mostrar(ui, linhas.len(), |i, row| {
                    let (nome, rec, pago) = &linhas[i];
                    row.col(|ui| {
                        ui.add(Rotulo::interface(nome.clone()));
                    });
                    row.col(|ui| {
                        ui.add(ValorDinheiro::novo(*rec));
                    });
                    row.col(|ui| {
                        ui.add(ValorDinheiro::novo(*pago));
                    });
                    row.col(|ui| {
                        ui.add(ValorDinheiro::novo(*rec - *pago).com_sinal());
                    });
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
    }
}

fn dialogo_recorrencias(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    let linhas: Vec<(String, String, String, String, bool)> = estado
        .recorrencias
        .iter()
        .map(|r| {
            let especie = if matches!(r.especie, EspecieTitulo::Receber) {
                "A receber"
            } else {
                "A pagar"
            };
            let periodo = match r.periodicidade {
                Periodicidade::Mensal => "Mensal",
                Periodicidade::Semanal => "Semanal",
                Periodicidade::Anual => "Anual",
                Periodicidade::Personalizada => "Personalizada",
            };
            let valor = r
                .valor_fixo
                .map_or_else(|| "variável".to_owned(), |v| v.formatar_com_simbolo());
            (
                r.descricao.clone(),
                especie.to_owned(),
                periodo.to_owned(),
                valor,
                r.ativa,
            )
        })
        .collect();

    let fechar = Dialogo::nova("Recorrências").largura(760.0).mostrar(
        ctx,
        estado,
        |ui, _estado| {
            ui.add(Rotulo::campo(
                "Regras que geram títulos automaticamente — aluguel, internet, assinaturas. \
                     Nenhum título é criado agora; cada ocorrência vira título real na data.",
            ));
            ui.add_space(Espaco::E12);
            if linhas.is_empty() {
                ui.add(Rotulo::campo("Nenhuma recorrência cadastrada."));
            } else {
                let colunas = vec![
                    ColunaGrade::nova("Descrição"),
                    ColunaGrade::nova("Espécie").largura(100.0),
                    ColunaGrade::nova("Periodicidade").largura(120.0),
                    ColunaGrade::nova("Valor").largura(130.0).numero(),
                    ColunaGrade::nova("Ativa").largura(70.0),
                ];
                Grade::nova(colunas).mostrar(ui, linhas.len(), |i, row| {
                    let (desc, esp, per, val, ativa) = &linhas[i];
                    row.col(|ui| {
                        ui.add(Rotulo::interface(desc.clone()));
                    });
                    row.col(|ui| {
                        ui.add(Rotulo::interface(esp.clone()));
                    });
                    row.col(|ui| {
                        ui.add(Rotulo::interface(per.clone()));
                    });
                    row.col(|ui| {
                        ui.add(Rotulo::interface(val.clone()));
                    });
                    row.col(|ui| {
                        ui.add(Rotulo::campo(if *ativa { "sim" } else { "não" }));
                    });
                });
            }
        },
        |ui, estado| {
            if ui.add(Botao::primario("+ Nova recorrência")).clicked() {
                estado.carregar_contas(motor, sessao, false);
                estado.dlg = Dlg::NovaRecorrencia(FormRecorrencia::default());
            }
            if ui.add(Botao::secundario("Fechar")).clicked() {
                estado.dlg = Dlg::Fechado;
            }
        },
    );
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

fn dialogo_nova_recorrencia(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    let a_receber = match &estado.dlg {
        Dlg::NovaRecorrencia(f) => f.a_receber,
        _ => return,
    };
    if estado.contas_receber != a_receber {
        estado.carregar_contas(motor, sessao, a_receber);
    }
    let pessoas: Vec<(Id, String)> = if a_receber {
        estado.clientes.iter()
    } else {
        estado.fornecedores.iter()
    }
    .map(|p| (p.pessoa, p.nome.clone()))
    .collect();
    let contas: Vec<(Id, String)> = estado
        .contas
        .iter()
        .map(|c| (c.conta, format!("{} · {}", c.codigo, c.nome)))
        .collect();
    let cats: Vec<(Id, String)> = estado
        .categorias
        .iter()
        .map(|c| (c.id, c.nome.clone()))
        .collect();

    let fechar = Dialogo::nova("Nova recorrência").largura(720.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            let Dlg::NovaRecorrencia(f) = &mut estado.dlg else {
                return;
            };
            ui.horizontal(|ui| {
                for (rot, receber) in [("A pagar", false), ("A receber", true)] {
                    let sel = f.a_receber == receber;
                    let b = if sel {
                        Botao::primario(rot)
                    } else {
                        Botao::fantasma(rot)
                    };
                    if ui.add(b).clicked() && !sel {
                        f.a_receber = receber;
                        f.contraparte = None;
                        f.conta = None;
                    }
                }
            });
            ui.add_space(Espaco::E12);
            ui.add(Campo::novo("Descrição", &mut f.descricao).marcador("Aluguel da loja"));
            ui.add_space(Espaco::E12);
            SeletorOpcao::novo(
                if a_receber {
                    "Cliente (opcional)"
                } else {
                    "Fornecedor (opcional)"
                },
                &mut f.contraparte,
            )
            .opcoes(pessoas.clone())
            .placeholder(if a_receber {
                "Sem cliente informado"
            } else {
                "Sem fornecedor informado"
            })
            .mostrar(ui);
            if ui
                .add(botao_cadastro_rapido(if a_receber {
                    "+ Cadastrar cliente"
                } else {
                    "+ Cadastrar fornecedor"
                }))
                .clicked()
            {
                estado.dlg_rapido = Some(DlgRapido::Pessoa {
                    alvo: AlvoRapido::Recorrencia,
                    papel: if a_receber {
                        Papel::Cliente
                    } else {
                        Papel::Fornecedor
                    },
                    tipo: TipoPessoa::Fisica,
                    nome: String::new(),
                });
            }
            ui.add_space(Espaco::E12);
            SeletorOpcao::novo(
                if a_receber {
                    "Conta de receita"
                } else {
                    "Conta de despesa"
                },
                &mut f.conta,
            )
            .opcoes(contas.clone())
            .placeholder("Escolha a conta do plano")
            .mostrar(ui);
            ui.add_space(Espaco::E12);
            ui.columns(2, |c| {
                c[0].add(Campo::novo("Valor mensal", &mut f.valor).marcador("0,00"));
                SeletorOpcao::novo("Categoria (opcional)", &mut f.categoria)
                    .opcoes(cats.clone())
                    .placeholder("Sem categoria")
                    .mostrar(&mut c[1]);
                if c[1]
                    .add(botao_cadastro_rapido("+ Nova categoria"))
                    .clicked()
                {
                    estado.dlg_rapido = Some(DlgRapido::Categoria {
                        alvo: AlvoRapido::Recorrencia,
                        nome: String::new(),
                        especie: None,
                    });
                }
            });
            ui.add_space(Espaco::E12);
            ui.horizontal(|ui| {
                ui.add(Rotulo::campo("Periodicidade"));
                for (rot, p) in [
                    ("Mensal", PeriodoRec::Mensal),
                    ("Semanal", PeriodoRec::Semanal),
                    ("Anual", PeriodoRec::Anual),
                ] {
                    let sel = f.periodicidade == p;
                    let b = if sel {
                        Botao::primario(rot)
                    } else {
                        Botao::fantasma(rot)
                    };
                    if ui.add(b).clicked() {
                        f.periodicidade = p;
                    }
                }
            });
            ui.add_space(Espaco::E12);
            ui.columns(3, |c| {
                let rot_dia = match f.periodicidade {
                    PeriodoRec::Semanal => "Dia da semana (0=dom)",
                    _ => "Dia do mês",
                };
                c[0].add(Campo::novo(rot_dia, &mut f.dia_referencia));
                c[1].add(Campo::novo("Início", &mut f.inicio).mascara(Mascara::Data));
                c[2].add(Campo::novo("Gerar com (dias)", &mut f.antecedencia));
            });
            ui.add_space(Espaco::E12);
            ui.add(Campo::novo("Fim (opcional)", &mut f.fim).mascara(Mascara::Data));
        },
        |ui, estado| {
            if ui.add(Botao::primario("Criar recorrência")).clicked() {
                criar_recorrencia(ui.ctx(), motor, sessao, estado);
            }
            if ui.add(Botao::secundario("Cancelar")).clicked() {
                estado.carregar_recorrencias(motor, sessao);
                estado.dlg = Dlg::Recorrencias;
            }
        },
    );
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

fn criar_recorrencia(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    let Dlg::NovaRecorrencia(f) = &estado.dlg else {
        return;
    };
    if f.descricao.trim().is_empty() {
        notificar(ctx, Notificacao::aviso("Informe a descrição."));
        return;
    }
    // Cliente/fornecedor é opcional (pedido explícito do usuário) — uma recorrência avulsa
    // não precisa de uma pessoa cadastrada.
    let contraparte_id = f.contraparte;
    let Some(conta) = f.conta else {
        notificar(ctx, Notificacao::aviso("Selecione a conta do plano."));
        return;
    };
    let (Ok(valor), Ok(inicio), Ok(dia), Ok(antecedencia)) = (
        f.valor.parse::<Dinheiro>(),
        f.inicio.parse::<Data>(),
        f.dia_referencia.parse::<u8>(),
        f.antecedencia.parse::<u16>(),
    ) else {
        notificar(
            ctx,
            Notificacao::aviso("Confira valor (0,00), início (dd/mm/aaaa), dia e antecedência."),
        );
        return;
    };
    let fim = if f.fim.trim().is_empty() {
        None
    } else {
        match f.fim.parse::<Data>() {
            Ok(d) => Some(d),
            Err(_) => {
                notificar(ctx, Notificacao::aviso("Fim inválido (dd/mm/aaaa)."));
                return;
            }
        }
    };
    let especie = if f.a_receber {
        EspecieTitulo::Receber
    } else {
        EspecieTitulo::Pagar
    };
    let contraparte = contraparte_id.map(|id| {
        if f.a_receber {
            Contraparte::Cliente(id)
        } else {
            Contraparte::Fornecedor(id)
        }
    });
    let cmd = CriarRecorrencia {
        descricao: f.descricao.trim().to_owned(),
        especie,
        contraparte,
        tipo_valor: TipoValor::Fixo,
        valor_fixo: Some(valor),
        indice: None,
        media_ultimos_n: None,
        periodicidade: f.periodicidade.dominio(),
        dia_referencia: Some(dia),
        expressao_cron: None,
        inicio,
        fim,
        conta_contrapartida: conta,
        centro_custo: None,
        categoria: f.categoria,
        antecedencia_geracao_dias: antecedencia,
    };
    match motor
        .executar(sessao, "financeiro.criar_recorrencia.v1", &cmd)
        .map(|_: mod_financeiro::RecorrenciaCriada| ())
    {
        Ok(()) => {
            estado.erro = None;
            estado.carregar_recorrencias(motor, sessao);
            estado.dlg = Dlg::Recorrencias;
            notificar(ctx, Notificacao::sucesso("Recorrência criada"));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

/// Um cartão de indicador: faixa de acento no topo, rótulo pequeno, **valor que sobe
/// contando** até o número final, linha de apoio opcional. A contagem só dispara quando o
/// valor muda de verdade (`perseguir` persegue o alvo a partir do último exibido) — entrar
/// na tela com os mesmos números não re-anima.
fn kpi(ui: &mut egui::Ui, rotulo: &str, valor: Dinheiro, cor: egui::Color32, apoio: Option<&str>) {
    let cores = ui.cores();
    let id = ui.id().with(("kpi", rotulo));
    #[allow(clippy::cast_precision_loss)]
    let alvo = valor.em_centavos() as f32;
    let mostrado = perseguir(ui, id, alvo, Mov::CONTAGEM);
    #[allow(clippy::cast_possible_truncation)]
    let texto = Dinheiro::centavos(mostrado.round() as i64).formatar_com_simbolo();

    let frame = egui::Frame::none()
        .fill(cores.superficie)
        .stroke(egui::Stroke::new(1.0_f32, cores.borda))
        .rounding(Raio::CARTAO)
        .shadow(sombra_cartao(ui.ctx()))
        .inner_margin(Espaco::E16)
        .show(ui, |ui| {
            // Estica pra ocupar a coluna inteira (`ui.columns` já reservou a largura certa)
            // — antes era uma largura fixa de 212px, sobrando um vão morto do lado em
            // qualquer janela mais larga que 3×212px (achado pelo usuário na Visão Geral).
            ui.set_width(ui.available_width());
            ui.vertical(|ui| {
                ui.add(Rotulo::campo(rotulo.to_uppercase()));
                ui.add_space(Espaco::E4);
                ui.add(Rotulo::titulo_secao(texto).cor(cor));
                if let Some(a) = apoio {
                    ui.add_space(Espaco::E4);
                    ui.add(Rotulo::campo(a.to_owned()));
                }
            });
        });

    // Faixa de acento colada no topo do cartão, na cor semântica do indicador.
    let r = frame.response.rect;
    let faixa = egui::Rect::from_min_max(r.min, egui::pos2(r.max.x, r.min.y + 3.0_f32));
    ui.painter().rect_filled(
        faixa,
        egui::Rounding {
            nw: Raio::CARTAO,
            ne: Raio::CARTAO,
            sw: 0.0_f32,
            se: 0.0_f32,
        },
        cor,
    );
}

/// Largura abaixo da qual o gráfico principal e o recorte "por categoria" empilham em vez
/// de ficar lado a lado — mesma ideia de `LayoutTela::mostrar_com_detalhe`, só que local a
/// este painel (não é uma tela mestre-detalhe).
const LIMIAR_COLUNAS_VISAO: f32 = 760.0;

fn painel_visao(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    let cores = ui.cores();

    // Faixa de KPIs — três cartões esticados pra ocupar a largura toda, em vez de flutuar
    // colados à esquerda com um vão morto do lado (pedido explícito do usuário: a Visão
    // Geral estava "horrível", essa era a maior causa numa janela larga).
    ui.columns(3, |col| {
        kpi(
            &mut col[0],
            "A receber em aberto",
            estado.dash_receber,
            cores.positivo,
            None,
        );
        kpi(
            &mut col[1],
            "A pagar em aberto",
            estado.dash_pagar,
            cores.negativo,
            None,
        );
        let saldo = estado.dash_recebido_mes - estado.dash_pago_mes;
        kpi(
            &mut col[2],
            "Saldo do mês",
            saldo,
            if saldo.e_negativo() {
                cores.negativo
            } else {
                cores.positivo
            },
            Some(&format!(
                "recebido {} · pago {}",
                estado.dash_recebido_mes.formatar_com_simbolo(),
                estado.dash_pago_mes.formatar_com_simbolo()
            )),
        );
    });
    ui.add_space(Espaco::E12);

    // Faixa "vence em 7 dias".
    let (nr, vr) = estado.dash_venc_receber;
    let (np, vp) = estado.dash_venc_pagar;
    egui::Frame::none()
        .fill(if np > 0 {
            cores.atencao_suave
        } else {
            cores.superficie_2
        })
        .rounding(Raio::ITEM)
        .inner_margin(egui::Margin::symmetric(Espaco::E12, Espaco::E8))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.add(Rotulo::campo("PRÓXIMOS 7 DIAS"));
                ui.add_space(Espaco::E16);
                ui.add(
                    Rotulo::interface(format!("{np} a pagar · {}", vp.formatar_com_simbolo()))
                        .cor(cores.negativo),
                );
                ui.add_space(Espaco::E16);
                ui.add(
                    Rotulo::interface(format!("{nr} a receber · {}", vr.formatar_com_simbolo()))
                        .cor(cores.positivo),
                );
            });
        });
    ui.add_space(Espaco::E24);

    let tem_dados = estado
        .serie
        .iter()
        .any(|m| m.receita > 0.0 || m.custo > 0.0);
    let tem_categorias = !estado.analise.is_empty();

    let cartao_grafico = |ui: &mut egui::Ui, titulo: &str, corpo: &dyn Fn(&mut egui::Ui)| {
        ui.add(Rotulo::titulo_secao(titulo));
        ui.add_space(Espaco::E8);
        egui::Frame::none()
            .fill(cores.superficie)
            .stroke(egui::Stroke::new(1.0_f32, cores.borda))
            .rounding(Raio::CARTAO)
            .inner_margin(Espaco::E16)
            .show(ui, corpo);
    };

    let largura = ui.available_width();
    let lado_a_lado = largura >= LIMIAR_COLUNAS_VISAO && tem_dados && tem_categorias;
    if lado_a_lado {
        ui.columns(2, |col| {
            cartao_grafico(&mut col[0], "Recebido × pago — últimos 6 meses", &|ui| {
                grafico_fluxo(ui, estado, &cores)
            });
            cartao_grafico(&mut col[1], "Por categoria — últimos 6 meses", &|ui| {
                GraficoBarrasHorizontais::novo(&estado.top_categorias()).mostrar(ui);
            });
        });
    } else {
        cartao_grafico(ui, "Recebido × pago — últimos 6 meses", &|ui| {
            grafico_fluxo(ui, estado, &cores)
        });
        if tem_categorias {
            ui.add_space(Espaco::E16);
            cartao_grafico(ui, "Por categoria — últimos 6 meses", &|ui| {
                GraficoBarrasHorizontais::novo(&estado.top_categorias()).mostrar(ui);
            });
        }
    }
    ui.add_space(Espaco::E16);
    if ui.add(Botao::secundario("Ver por categoria")).clicked() {
        estado.carregar_analise(motor, sessao);
        estado.dlg = Dlg::Analise;
    }
}

/// O corpo do cartão "Recebido × pago" — extraído de [`painel_visao`] porque aparece tanto
/// no layout lado a lado quanto no empilhado.
fn grafico_fluxo(
    ui: &mut egui::Ui,
    estado: &EstadoTelaFinanceiro,
    cores: &cardeal_ui::tokens::Cores,
) {
    let tem_dados = estado
        .serie
        .iter()
        .any(|m| m.receita > 0.0 || m.custo > 0.0);
    if !tem_dados {
        ui.add(Rotulo::campo(
            "Sem baixas nos últimos 6 meses — o gráfico aparece conforme você recebe e paga títulos.",
        ));
        return;
    }
    let eixo: Vec<String> = estado.serie.iter().map(|m| m.rotulo.clone()).collect();
    let series = [
        SerieBarras {
            rotulo: "Recebido".to_owned(),
            cor: cores.positivo,
            valores: estado.serie.iter().map(|m| m.receita).collect(),
        },
        SerieBarras {
            rotulo: "Pago".to_owned(),
            cor: cores.negativo,
            valores: estado.serie.iter().map(|m| m.custo).collect(),
        },
    ];
    GraficoBarras::novo(&eixo, &series)
        .altura(260.0)
        .mostrar(ui);
}

fn painel_fluxo(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    let cores = ui.cores();
    ui.horizontal(|ui| {
        ui.add(Rotulo::campo("PERÍODO"));
        for (d, rot) in [(30_i64, "30 dias"), (90, "90 dias"), (365, "12 meses")] {
            let b = if estado.fluxo_dias == d {
                Botao::primario(rot).pequeno()
            } else {
                Botao::fantasma(rot).pequeno()
            };
            if ui.add(b).clicked() {
                estado.fluxo_dias = d;
                estado.carregar_fluxo(motor, sessao);
            }
        }
    });
    ui.add_space(Espaco::E12);

    // Fluxo de caixa é o local de "bater o caixa": o lojista confere o físico contra o
    // sistema, então os cards aqui são só saldo em caixa e saldo em bancos — nada de
    // "entrou/saiu no período" (isso mora nos cards de Contas a Receber/Pagar agora,
    // pedido explícito do usuário).
    let saldo_caixa: Dinheiro = estado
        .contas_disp
        .iter()
        .filter(|c| c.e_caixa)
        .map(|c| c.saldo)
        .fold(Dinheiro::ZERO, |a, b| a + b);
    let saldo_bancos: Dinheiro = estado
        .contas_disp
        .iter()
        .filter(|c| !c.e_caixa)
        .map(|c| c.saldo)
        .fold(Dinheiro::ZERO, |a, b| a + b);

    ui.columns(2, |col| {
        kpi(
            &mut col[0],
            "Saldo em caixa",
            saldo_caixa,
            if saldo_caixa.e_negativo() {
                cores.negativo
            } else {
                cores.texto_forte
            },
            Some("confira contra o caixa físico"),
        );
        kpi(
            &mut col[1],
            "Saldo em conta bancária",
            saldo_bancos,
            if saldo_bancos.e_negativo() {
                cores.negativo
            } else {
                cores.texto_forte
            },
            None,
        );
    });
    ui.add_space(Espaco::E16);

    if estado.extrato.is_empty() {
        ui.add(Rotulo::campo(
            "Nenhum movimento realizado no período. Este é o livro do que efetivamente entrou e saiu — títulos ainda não baixados não aparecem aqui.",
        ));
        return;
    }

    let colunas = vec![
        ColunaGrade::nova("Data").largura(96.0),
        ColunaGrade::nova("Histórico"),
        ColunaGrade::nova("Conta").largura(140.0),
        ColunaGrade::nova("Valor").largura(130.0).numero(),
        ColunaGrade::nova("Acumulado").largura(130.0).numero(),
    ];
    let mut acc = Dinheiro::ZERO;
    let linhas: Vec<(String, String, String, Dinheiro, Dinheiro)> = estado
        .extrato
        .iter()
        .map(|m| {
            acc += m.valor;
            let hist = match estado.rotulo_origem(m) {
                Some(origem) => format!("{} — {origem}", m.historico),
                None => m.historico.clone(),
            };
            (
                m.data.formatar_curta(),
                hist,
                m.conta_nome.clone(),
                m.valor,
                acc,
            )
        })
        .collect();
    Grade::nova(colunas).mostrar(ui, linhas.len(), |i, row| {
        let (data, hist, conta, valor, acumulado) = &linhas[i];
        row.col(|ui| {
            ui.add(Rotulo::campo(data.clone()));
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(hist.clone()));
        });
        row.col(|ui| {
            ui.add(Rotulo::campo(conta.clone()));
        });
        row.col(|ui| {
            ui.add(ValorDinheiro::novo(*valor).com_sinal());
        });
        row.col(|ui| {
            ui.add(ValorDinheiro::novo(*acumulado));
        });
    });
}

fn painel_bancos(ui: &mut egui::Ui, estado: &mut EstadoTelaFinanceiro) {
    if ui.add(Botao::primario("+ Nova conta bancária")).clicked() {
        estado.dlg = Dlg::NovaContaBancaria {
            nome: String::new(),
        };
    }
    ui.add_space(Espaco::E12);

    if estado.contas_disp.is_empty() {
        ui.add(Rotulo::campo("Nenhuma conta no grupo Disponível."));
        return;
    }
    let total: Dinheiro = estado
        .contas_disp
        .iter()
        .map(|c| c.saldo)
        .fold(Dinheiro::ZERO, |a, b| a + b);

    let colunas = vec![
        ColunaGrade::nova("Conta"),
        ColunaGrade::nova("Código").largura(90.0),
        ColunaGrade::nova("Tipo").largura(90.0),
        ColunaGrade::nova("Saldo").largura(140.0).numero(),
    ];
    Grade::nova(colunas).mostrar(ui, estado.contas_disp.len(), |i, row| {
        let c = &estado.contas_disp[i];
        row.col(|ui| {
            ui.add(Rotulo::interface(c.nome.clone()));
        });
        row.col(|ui| {
            ui.add(Rotulo::campo(c.codigo.clone()));
        });
        row.col(|ui| {
            ui.add(Rotulo::campo(if c.e_caixa { "Caixa" } else { "Banco" }));
        });
        row.col(|ui| {
            ui.add(ValorDinheiro::novo(c.saldo));
        });
    });
    ui.add_space(Espaco::E12);
    ui.horizontal(|ui| {
        ui.add(Rotulo::titulo_secao("Total disponível"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(Rotulo::titulo_secao(total.formatar_com_simbolo()));
        });
    });
}

fn dialogo_conta_bancaria(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    let fechar = Dialogo::nova("Nova conta bancária").largura(460.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            let Dlg::NovaContaBancaria { nome } = &mut estado.dlg else {
                return;
            };
            ui.add(Rotulo::campo(
                "Entra no plano como conta analítica do grupo 1.1 (Disponível).",
            ));
            ui.add_space(Espaco::E8);
            ui.add(Campo::novo("Nome", nome).marcador("Nubank — cc 12345-6"));
        },
        |ui, estado| {
            if ui.add(Botao::primario("Criar")).clicked() {
                if let Dlg::NovaContaBancaria { nome } = &estado.dlg {
                    let nome = nome.trim().to_owned();
                    if nome.is_empty() {
                        notificar(ui.ctx(), Notificacao::aviso("Informe o nome da conta."));
                        return;
                    }
                    match motor
                        .executar(
                            sessao,
                            "financeiro.criar_conta_bancaria.v1",
                            &CriarContaBancaria { nome },
                        )
                        .map(|_: ContaBancariaCriada| ())
                    {
                        Ok(()) => {
                            estado.dlg = Dlg::Fechado;
                            estado.carregar_fluxo(motor, sessao);
                            notificar(ui.ctx(), Notificacao::sucesso("Conta bancária criada"));
                        }
                        Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
                    }
                }
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

fn dialogo_categoria(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    let fechar = Dialogo::nova("Nova categoria").largura(460.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            let Dlg::NovaCategoria { nome, especie } = &mut estado.dlg else {
                return;
            };
            ui.add(Rotulo::campo(
                "Rótulo livre para relatório — \"Aluguel\", \"Peças\", \"Assinatura SaaS\". \
                 Não afeta a contabilização, só agrupa os gráficos de \"Por categoria\".",
            ));
            ui.add_space(Espaco::E8);
            ui.add(Campo::novo("Nome", nome).marcador("Aluguel"));
            ui.add_space(Espaco::E12);
            ui.horizontal(|ui| {
                ui.add(Rotulo::campo("Serve para"));
                for (rot, valor) in [
                    ("Ambas", None),
                    ("Só a receber", Some(EspecieTitulo::Receber)),
                    ("Só a pagar", Some(EspecieTitulo::Pagar)),
                ] {
                    let sel = *especie == valor;
                    let b = if sel {
                        Botao::primario(rot)
                    } else {
                        Botao::fantasma(rot)
                    };
                    if ui.add(b).clicked() {
                        *especie = valor;
                    }
                }
            });
        },
        |ui, estado| {
            if ui.add(Botao::primario("Criar")).clicked() {
                if let Dlg::NovaCategoria { nome, especie } = &estado.dlg {
                    let nome = nome.trim().to_owned();
                    if nome.is_empty() {
                        notificar(ui.ctx(), Notificacao::aviso("Informe o nome da categoria."));
                        return;
                    }
                    let especie = *especie;
                    match motor
                        .executar(
                            sessao,
                            "financeiro.criar_categoria.v1",
                            &CriarCategoria { nome, especie },
                        )
                        .map(|_: mod_financeiro::CategoriaCriada| ())
                    {
                        Ok(()) => {
                            estado.dlg = Dlg::Fechado;
                            estado.carregar_categorias(motor, sessao);
                            notificar(ui.ctx(), Notificacao::sucesso("Categoria criada"));
                        }
                        Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
                    }
                }
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

fn abas(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    let nova = Abas::nova(&[
        (Aba::Visao, "Visão geral"),
        (Aba::Receber, "A receber"),
        (Aba::Pagar, "A pagar"),
        (Aba::Fluxo, "Fluxo de caixa"),
        (Aba::Bancos, "Contas bancárias"),
    ])
    .selecionada(estado.aba)
    .mostrar(ui);
    if let Some(nova) = nova {
        estado.aba = nova;
        estado.carregar(motor, sessao);
    }
}

/// O seletor de período das abas "A receber"/"A pagar" — presets de calendário + uma faixa
/// digitada (`PresetPeriodo::Personalizado`). Devolve `true` quando o período mudou e a
/// tela precisa recarregar.
fn seletor_periodo(ui: &mut egui::Ui, filtro: &mut FiltroPeriodo) -> bool {
    let mut mudou = false;
    ui.horizontal_wrapped(|ui| {
        ui.add(Rotulo::campo("PERÍODO"));
        for (preset, rot) in [
            (PresetPeriodo::Dia, "Dia"),
            (PresetPeriodo::Semana, "Semana"),
            (PresetPeriodo::Mes, "Mês"),
            (PresetPeriodo::Trimestre, "Trimestre"),
            (PresetPeriodo::Semestre, "Semestre"),
            (PresetPeriodo::Ano, "Ano"),
            (PresetPeriodo::Personalizado, "Personalizado"),
        ] {
            let sel = filtro.preset == preset;
            let b = if sel {
                Botao::primario(rot).pequeno()
            } else {
                Botao::fantasma(rot).pequeno()
            };
            if ui.add(b).clicked() && !sel {
                filtro.preset = preset;
                if preset == PresetPeriodo::Personalizado {
                    let hoje = Data::hoje(Fuso::BRASILIA).to_string();
                    if filtro.de_personalizado.is_empty() {
                        filtro.de_personalizado = hoje.clone();
                    }
                    if filtro.ate_personalizado.is_empty() {
                        filtro.ate_personalizado = hoje;
                    }
                } else {
                    mudou = true;
                }
            }
        }
    });
    if filtro.preset == PresetPeriodo::Personalizado {
        ui.add_space(Espaco::E8);
        ui.horizontal(|ui| {
            ui.add(Campo::novo("De", &mut filtro.de_personalizado).mascara(Mascara::Data));
            ui.add_space(Espaco::E8);
            ui.add(Campo::novo("Até", &mut filtro.ate_personalizado).mascara(Mascara::Data));
            ui.add_space(Espaco::E8);
            if ui.add(Botao::secundario("Aplicar")).clicked() {
                mudou = true;
            }
        });
    }
    mudou
}

fn lista(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    if seletor_periodo(ui, &mut estado.periodo) {
        estado.carregar_parcelas_periodo(motor, sessao);
    }
    ui.add_space(Espaco::E12);

    let hoje = Data::hoje(Fuso::BRASILIA);
    let periodo = estado.periodo.resolver(hoje);
    let a_receber = estado.aba.a_receber();

    // Os dois cards pedidos explicitamente pelo usuário: quanto foi de fato recebido/pago
    // no período (baixas, não vencimento) e a projeção do que ainda vai vencer — nunca
    // some da tela mesmo com a lista vazia, pra sempre dar pra ver "nada aconteceu nesse
    // período" com números, não um vazio mudo.
    let vencidas = estado
        .parcelas
        .iter()
        .filter(|p| p.estado.aceita_baixa() && p.vencimento < hoje)
        .count();
    FaixaKpi::nova(vec![
        CartaoKpi::novo(
            if a_receber {
                "Recebido no período"
            } else {
                "Pago no período"
            },
            estado.baixado_periodo,
        ),
        CartaoKpi::contagem("Vencidas", vencidas),
        CartaoKpi::novo(
            format!("Projeção até {}", periodo.ate.formatar_curta()),
            estado.projecao_periodo,
        ),
    ])
    .mostrar(ui);
    ui.add_space(Espaco::E16);

    if estado.parcelas.is_empty() {
        let msg = if a_receber {
            "Nada a receber nesse período."
        } else {
            "Nada a pagar nesse período."
        };
        EstadoVazio::novo(Icone::Dinheiro, msg).mostrar(ui);
        return;
    }

    ui.horizontal(|ui| {
        ui.set_max_width(360.0);
        ui.add(Campo::novo("", &mut estado.busca).marcador("Buscar por cliente/fornecedor"));
    });
    ui.add_space(Espaco::E12);

    let indices = estado.parcelas_filtradas();
    if indices.is_empty() {
        ui.add(Rotulo::interface("Nenhuma parcela para essa busca.").cor(ui.cores().texto_medio));
        return;
    }

    let colunas = vec![
        ColunaGrade::nova("Contraparte"),
        ColunaGrade::nova("Parc.").largura(60.0).numero(),
        ColunaGrade::nova("Vencimento").largura(120.0),
        ColunaGrade::nova("Saldo").largura(130.0).numero(),
        ColunaGrade::nova("Estado").largura(110.0),
    ];
    let resposta = Grade::nova(colunas)
        .selecionavel(None)
        .ordenacao(estado.ordenacao)
        .mostrar(ui, indices.len(), |i, row| {
            let p = &estado.parcelas[indices[i]];
            row.col(|ui| {
                ui.add(Rotulo::interface(estado.nome_contraparte(&p.contraparte)));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(p.numero.to_string()));
            });
            row.col(|ui| {
                let venceu = p.vencimento < hoje;
                let r = Rotulo::interface(p.vencimento.to_string());
                ui.add(if venceu {
                    r.cor(ui.cores().negativo)
                } else {
                    r
                });
            });
            row.col(|ui| {
                ui.add(ValorDinheiro::novo(p.saldo()));
            });
            row.col(|ui| {
                ui.add(etiqueta_estado_parcela(p.estado, p.vencimento < hoje));
            });
        });

    if let Some(coluna) = resposta.coluna_clicada {
        let direcao = match estado.ordenacao {
            Some((atual, direcao)) if atual == coluna => direcao.invertida(),
            _ => Direcao::Ascendente,
        };
        estado.ordenacao = Some((coluna, direcao));
        let nomes = estado.nomes.clone();
        ordenar_parcelas(&mut estado.parcelas, &nomes, coluna, direcao);
    }
    if let Some(i) = resposta.linha_clicada {
        let indice = indices[i];
        if estado.parcelas[indice].estado.aceita_baixa() {
            estado.dlg = Dlg::Baixar {
                indice,
                valor: estado.parcelas[indice].saldo().formatar(),
                data: Data::hoje(Fuso::BRASILIA).to_string(),
                meio_pagamento: Some(MeioPagamento::Pix),
                baixas: Vec::new(),
                baixas_carregadas: false,
                motivo_estorno: String::new(),
            };
        }
        // Uma parcela já quitada/cancelada/renegociada não abre "dar baixa" — só está na
        // lista como histórico (pedido explícito do usuário).
    }
}

/// Etiqueta colorida para o estado de uma parcela — a lista mostra qualquer estado agora
/// (`parcelas_no_periodo` não filtra), então além de `Aberta`/`Parcial` também chega
/// `Quitada`/`Cancelada`/`Renegociada` (rótulo neutro, caso padrão). Vencida pesa mais que
/// o estado em si, por isso entra primeiro na escolha do tom.
fn etiqueta_estado_parcela(estado: EstadoParcela, vencida: bool) -> Etiqueta {
    match (estado, vencida) {
        (EstadoParcela::Parcial, true) => Etiqueta::negativa("Parcial · vencida"),
        (EstadoParcela::Parcial, false) => Etiqueta::info("Parcial"),
        (EstadoParcela::Aberta, true) => Etiqueta::negativa("Vencida"),
        (EstadoParcela::Aberta, false) => Etiqueta::neutra("Aberta"),
        (outro, _) => Etiqueta::neutra(outro.rotulo()),
    }
}

/// Ordena `parcelas` pela coluna clicada no cabeçalho da [`Grade`] de `lista` (mesma ordem
/// das colunas: Contraparte, Parc., Vencimento, Saldo, Estado). `nomes` resolve o nome de
/// exibição da contraparte — a própria coluna 0 ordena por ele, não pelo id.
fn ordenar_parcelas(
    parcelas: &mut [ItemTituloEmAberto],
    nomes: &HashMap<Id, String>,
    coluna: usize,
    direcao: Direcao,
) {
    let nome_de = |p: &ItemTituloEmAberto| -> String {
        let Some(c) = p.contraparte else {
            return String::new();
        };
        let id = match c {
            Contraparte::Cliente(i)
            | Contraparte::Fornecedor(i)
            | Contraparte::Funcionario(i)
            | Contraparte::Socio(i)
            | Contraparte::Outro(i) => i,
        };
        nomes.get(&id).cloned().unwrap_or_default()
    };
    parcelas.sort_by(|a, b| {
        let ordem = match coluna {
            0 => nome_de(a).cmp(&nome_de(b)),
            1 => a.numero.cmp(&b.numero),
            2 => a.vencimento.cmp(&b.vencimento),
            3 => a.saldo().cmp(&b.saldo()),
            4 => a.estado.rotulo().cmp(b.estado.rotulo()),
            _ => std::cmp::Ordering::Equal,
        };
        match direcao {
            Direcao::Ascendente => ordem,
            Direcao::Descendente => ordem.reverse(),
        }
    });
}

fn dialogo_lancar(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    let a_receber = estado.aba.a_receber();
    let titulo = if a_receber {
        "Lançar título a receber"
    } else {
        "Lançar título a pagar"
    };
    let ops: Vec<(Id, String)> = if a_receber {
        estado
            .clientes
            .iter()
            .map(|p| (p.pessoa, p.nome.clone()))
            .collect()
    } else {
        estado
            .fornecedores
            .iter()
            .map(|p| (p.pessoa, p.nome.clone()))
            .collect()
    };
    let cats: Vec<(Id, String)> = estado
        .categorias
        .iter()
        .map(|c| (c.id, c.nome.clone()))
        .collect();

    let fechar = Dialogo::nova(titulo).largura(680.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            let Dlg::Lancar(f) = &mut estado.dlg else {
                return;
            };
            SeletorOpcao::novo(
                if a_receber {
                    "Cliente (opcional)"
                } else {
                    "Fornecedor (opcional)"
                },
                &mut f.contraparte,
            )
            .opcoes(ops.clone())
            .placeholder(if a_receber {
                "Sem cliente informado"
            } else {
                "Sem fornecedor informado"
            })
            .mostrar(ui);
            if ui
                .add(botao_cadastro_rapido(if a_receber {
                    "+ Cadastrar cliente"
                } else {
                    "+ Cadastrar fornecedor"
                }))
                .clicked()
            {
                estado.dlg_rapido = Some(DlgRapido::Pessoa {
                    alvo: AlvoRapido::Lancar,
                    papel: if a_receber {
                        Papel::Cliente
                    } else {
                        Papel::Fornecedor
                    },
                    tipo: TipoPessoa::Fisica,
                    nome: String::new(),
                });
            }
            ui.add_space(Espaco::E12);
            ui.columns(2, |c| {
                c[0].add(Campo::novo("Valor total", &mut f.valor).marcador("0,00"));
                c[1].add(Campo::novo("Emissão", &mut f.emissao).mascara(Mascara::Data));
            });
            ui.add_space(Espaco::E12);
            ui.columns(3, |c| {
                c[0].add(Campo::novo("Parcelas", &mut f.parcelas));
                c[1].add(
                    Campo::novo("1º vencimento", &mut f.primeiro_vencimento).mascara(Mascara::Data),
                );
                c[2].add(Campo::novo("Intervalo (dias)", &mut f.intervalo));
            });
            ui.add_space(Espaco::E12);
            cardeal_ui::molecules::SeletorOpcao::novo("Categoria (opcional)", &mut f.categoria)
                .opcoes(cats.clone())
                .placeholder("Sem categoria")
                .mostrar(ui);
            if ui.add(botao_cadastro_rapido("+ Nova categoria")).clicked() {
                estado.dlg_rapido = Some(DlgRapido::Categoria {
                    alvo: AlvoRapido::Lancar,
                    nome: String::new(),
                    especie: None,
                });
            }
            ui.add_space(Espaco::E12);
            ui.add(Campo::novo("Observação", &mut f.observacao));
        },
        |ui, estado| {
            if ui.add(Botao::primario("Lançar")).clicked() {
                lancar(ui.ctx(), motor, sessao, estado);
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

fn lancar(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    let a_receber = estado.aba.a_receber();
    let Dlg::Lancar(f) = &estado.dlg else { return };
    // Cliente/fornecedor é opcional (pedido explícito do usuário) — um título avulso não
    // precisa de uma pessoa cadastrada.
    let contraparte = f.contraparte;
    let (Ok(valor), Ok(emissao), Ok(parcelas), Ok(prim), Ok(intervalo)) = (
        f.valor.parse::<Dinheiro>(),
        f.emissao.parse::<Data>(),
        f.parcelas.parse::<u16>(),
        f.primeiro_vencimento.parse::<Data>(),
        f.intervalo.parse::<i32>(),
    ) else {
        notificar(
            ctx,
            Notificacao::aviso("Confira os campos — valor 0,00, datas dd/mm/aaaa."),
        );
        return;
    };
    let obs = (!f.observacao.trim().is_empty()).then(|| f.observacao.clone());
    let cat = f.categoria;

    let r = if a_receber {
        motor
            .executar(
                sessao,
                "financeiro.lancar_titulo_a_receber.v1",
                &LancarTituloAReceber {
                    cliente: contraparte,
                    valor_total: valor,
                    emissao,
                    parcelas,
                    primeiro_vencimento: prim,
                    intervalo_dias: intervalo,
                    observacao: obs,
                    categoria: cat,
                },
            )
            .map(|_: mod_financeiro::TituloAReceberLancado| ())
    } else {
        motor
            .executar(
                sessao,
                "financeiro.lancar_titulo_a_pagar.v1",
                &LancarTituloAPagar {
                    fornecedor: contraparte,
                    valor_total: valor,
                    emissao,
                    parcelas,
                    primeiro_vencimento: prim,
                    intervalo_dias: intervalo,
                    observacao: obs,
                    categoria: cat,
                },
            )
            .map(|_: mod_financeiro::TituloAPagarLancado| ())
    };
    match r {
        Ok(()) => {
            estado.dlg = Dlg::Fechado;
            estado.carregar(motor, sessao);
            notificar(ctx, Notificacao::sucesso("Título lançado"));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

fn dialogo_baixar(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    let Dlg::Baixar { indice, .. } = estado.dlg else {
        return;
    };
    let Some(p) = estado.parcelas.get(indice).cloned() else {
        estado.dlg = Dlg::Fechado;
        return;
    };
    let nome = estado.nome_contraparte(&p.contraparte);

    // Carrega o histórico de baixas uma vez por abertura do diálogo — não a cada frame
    // (mesmo padrão de `tela_estoque.rs::dialogo_ver` para os movimentos de rastreabilidade).
    if let Dlg::Baixar {
        baixas_carregadas: false,
        ..
    } = &estado.dlg
    {
        let historico: Vec<ItemBaixa> = motor
            .consultar(
                sessao,
                "financeiro.baixas_da_parcela.v1",
                &BaixasDaParcela { parcela: p.parcela },
            )
            .unwrap_or_default();
        if let Dlg::Baixar {
            baixas,
            baixas_carregadas,
            ..
        } = &mut estado.dlg
        {
            *baixas = historico;
            *baixas_carregadas = true;
        }
    }

    let fechar = Dialogo::nova(format!("Parcela {} · {}", p.numero, nome))
        .largura(560.0)
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                ui.columns(2, |c| {
                    kv(&mut c[0], "Vencimento", &p.vencimento.to_string());
                    kv(
                        &mut c[1],
                        "Valor original",
                        &p.valor_original.formatar_com_simbolo(),
                    );
                });
                ui.columns(2, |c| {
                    kv(
                        &mut c[0],
                        "Já baixado",
                        &p.valor_baixado.formatar_com_simbolo(),
                    );
                    kv(&mut c[1], "Saldo", &p.saldo().formatar_com_simbolo());
                });
                ui.add_space(Espaco::E16);
                ui.separator();
                ui.add_space(Espaco::E12);
                ui.add(Rotulo::titulo_secao("Dar baixa"));
                ui.add_space(Espaco::E8);
                let Dlg::Baixar {
                    valor,
                    data,
                    meio_pagamento,
                    ..
                } = &mut estado.dlg
                else {
                    return;
                };
                ui.columns(2, |c| {
                    c[0].add(Campo::novo("Valor recebido", valor));
                    c[1].add(Campo::novo("Data", data).mascara(Mascara::Data));
                });
                ui.add_space(Espaco::E8);
                SeletorOpcao::novo("Meio de pagamento", meio_pagamento)
                    .opcao(MeioPagamento::Dinheiro, "Dinheiro — cai no Caixa")
                    .opcao(MeioPagamento::Pix, "Pix — cai no Banco")
                    .opcao(MeioPagamento::Cartao, "Cartão — cai no Banco")
                    .mostrar(ui);

                let Dlg::Baixar { baixas, .. } = &estado.dlg else {
                    return;
                };
                if !baixas.is_empty() {
                    ui.add_space(Espaco::E16);
                    let titulo = format!("Baixas anteriores ({})", baixas.len());
                    let baixas = baixas.clone();
                    let mut estornar_clicada = None;
                    SecaoExpansivel::nova(titulo)
                        .aberta_por_padrao(true)
                        .mostrar(ui, |ui| {
                            for b in &baixas {
                                if let Some(id) = baixa_historico(ui, b) {
                                    estornar_clicada = Some(id);
                                }
                            }
                            ui.add_space(Espaco::E8);
                            let Dlg::Baixar { motivo_estorno, .. } = &mut estado.dlg else {
                                return;
                            };
                            ui.add(
                                Campo::novo("Motivo do estorno", motivo_estorno)
                                    .marcador("obrigatório para estornar uma baixa acima"),
                            );
                        });
                    if let Some(baixa_id) = estornar_clicada {
                        estornar(ui.ctx(), motor, sessao, estado, baixa_id);
                    }
                }
            },
            |ui, estado| {
                if ui.add(Botao::primario("Confirmar baixa")).clicked() {
                    baixar(ui.ctx(), motor, sessao, estado, &p);
                }
                if ui.add(Botao::secundario("Fechar")).clicked() {
                    estado.dlg = Dlg::Fechado;
                }
            },
        );
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

/// Uma linha do histórico de baixas: data, valor, e "Estornar" quando ainda não estornada.
/// Devolve o id da baixa cujo botão "Estornar" foi clicado neste frame, se algum.
fn baixa_historico(ui: &mut egui::Ui, b: &ItemBaixa) -> Option<Id> {
    let cores = ui.cores();
    let mut clicada = None;
    egui::Frame::none()
        .fill(cores.superficie_2)
        .rounding(Raio::ITEM)
        .inner_margin(egui::Margin::symmetric(Espaco::E12, Espaco::E8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.add(Rotulo::interface(format!(
                        "{} · {}",
                        b.data.formatar_curta(),
                        b.valor_recebido.formatar_com_simbolo()
                    )));
                    if (b.juros + b.multa + b.desconto).e_positivo() {
                        ui.add(Rotulo::campo(format!(
                            "principal {} · juros {} · multa {} · desconto {}",
                            b.principal.formatar_com_simbolo(),
                            b.juros.formatar_com_simbolo(),
                            b.multa.formatar_com_simbolo(),
                            b.desconto.formatar_com_simbolo()
                        )));
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if b.estornada {
                        ui.add(Etiqueta::neutra("Estornada"));
                    } else if ui.add(Botao::fantasma("Estornar").pequeno()).clicked() {
                        clicada = Some(b.baixa);
                    }
                });
            });
        });
    clicada
}

fn baixar(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
    p: &ItemTituloEmAberto,
) {
    let Dlg::Baixar {
        valor,
        data,
        meio_pagamento,
        ..
    } = &estado.dlg
    else {
        return;
    };
    let (Ok(valor), Ok(data)) = (valor.parse::<Dinheiro>(), data.parse::<Data>()) else {
        notificar(
            ctx,
            Notificacao::aviso("Valor (0,00) ou data (dd/mm/aaaa) inválidos."),
        );
        return;
    };
    let Some(meio_pagamento) = *meio_pagamento else {
        notificar(ctx, Notificacao::aviso("Escolha o meio de pagamento."));
        return;
    };
    let r = if estado.aba.a_receber() {
        motor
            .executar(
                sessao,
                "financeiro.baixar_recebimento.v1",
                &BaixarRecebimento {
                    parcela: p.parcela,
                    valor,
                    data,
                    meio_pagamento,
                    conta_destino: None,
                },
            )
            .map(|_: mod_financeiro::RecebimentoBaixado| ())
    } else {
        motor
            .executar(
                sessao,
                "financeiro.baixar_pagamento.v1",
                &BaixarPagamento {
                    parcela: p.parcela,
                    valor,
                    data,
                    meio_pagamento,
                    conta_destino: None,
                },
            )
            .map(|_: mod_financeiro::PagamentoBaixado| ())
    };
    match r {
        Ok(()) => {
            estado.dlg = Dlg::Fechado;
            estado.carregar(motor, sessao);
            notificar(ctx, Notificacao::sucesso("Baixa registrada"));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

/// Estorna uma baixa do histórico exibido no diálogo — reverte o lançamento no Razão e
/// reabre a parcela (`EstornarBaixa`, `docs/modulos/financeiro.md` §5). Fecha o diálogo e
/// recarrega a lista no sucesso, como `baixar` — o índice guardado em `Dlg::Baixar` não
/// sobrevive a um recarregamento (a ordem pode mudar), então manter o diálogo aberto
/// arriscaria apontar para a parcela errada.
fn estornar(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
    baixa: Id,
) {
    let Dlg::Baixar { motivo_estorno, .. } = &estado.dlg else {
        return;
    };
    let motivo = motivo_estorno.trim();
    if motivo.is_empty() {
        notificar(
            ctx,
            Notificacao::aviso("Informe o motivo do estorno antes de confirmar."),
        );
        return;
    }
    let motivo = motivo.to_owned();
    match motor
        .executar(
            sessao,
            "financeiro.estornar_baixa.v1",
            &EstornarBaixa { baixa, motivo },
        )
        .map(|_: mod_financeiro::BaixaFoiEstornada| ())
    {
        Ok(()) => {
            estado.dlg = Dlg::Fechado;
            estado.carregar(motor, sessao);
            notificar(ctx, Notificacao::sucesso("Baixa estornada"));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

fn nome_mes(comp: Competencia) -> String {
    const M: [&str; 12] = [
        "jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez",
    ];
    let i = (comp.mes().clamp(1, 12) - 1) as usize;
    format!("{}/{:02}", M[i], comp.ano() % 100)
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
