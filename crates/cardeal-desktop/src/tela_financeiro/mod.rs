//! Tela de Financeiro — Contas a Receber / a Pagar. `docs/modulos/financeiro.md`.
//!
//! Duas abas (a receber / a pagar), cada uma a lista inteira de parcelas em aberto.
//! "+ Lançar título" e o clique numa linha abrem o `Dialogo` — ver a parcela e **dar baixa**.

mod baixa;
mod cadastro_rapido;
mod carga;
mod contas;
mod custos;
mod fluxo;
mod lancar;
mod pessoal;
mod recorrencias;
mod renegociar;
#[cfg(test)]
mod testes;
mod visao;

use std::collections::HashMap;

use baixa::*;
use cadastro_rapido::*;
use cardeal_cliente::{MotorLocal, SessaoLocal};
use cardeal_kernel::{Competencia, Data, Dinheiro, Fuso, Id, Periodo};
use cardeal_ledger::Contraparte;
use cardeal_modkit::Icone;
use cardeal_ui::atoms::{Botao, Divisor, Etiqueta, Rotulo, Tom, ValorDinheiro, ATALHO_NOVO};
use cardeal_ui::molecules::{
    dado, Abas, BarraFiltros, Campo, CartaoKpi, EstadoVazio, Mascara, SecaoExpansivel, SeletorOpcao,
};
use cardeal_ui::organisms::{
    notificar, ColunaGrade, Dialogo, Direcao, FaixaKpi, Grade, GraficoBarras,
    GraficoBarrasHorizontais, LayoutTela, Notificacao, Ordenacao, Painel, SerieBarras,
};
use cardeal_ui::tokens::{Espaco, TemaUi};
use contas::*;
use custos::*;
use eframe::egui;
use fluxo::*;
use lancar::*;
use mod_clientes::{ItemPessoa, Papel, PessoaCadastrada, PessoasPorPapel};
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
use recorrencias::*;
use renegociar::*;
use visao::*;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Aba {
    #[default]
    Visao,
    Receber,
    Pagar,
    Custos,
    Fluxo,
    Bancos,
}

/// A chave do topo: as contas da empresa ou as finanças pessoais do usuário.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Modo {
    #[default]
    Empresa,
    Pessoal,
}

impl Aba {
    const fn a_receber(self) -> bool {
        matches!(self, Self::Receber)
    }
}

/// Um mês da série de recebido × pago — em `Dinheiro`; só o gráfico converte para `f64`.
#[derive(Clone, Default)]
struct MesFluxo {
    rotulo: String,
    receita: Dinheiro,
    custo: Dinheiro,
}

/// Reais em `f64`, só para alimentar gráfico (nunca para somar ou exibir valor).
#[allow(clippy::cast_precision_loss)]
fn reais_para_grafico(d: Dinheiro) -> f64 {
    d.em_centavos() as f64 / 100.0
}

#[derive(Default)]
// Uma única instância por tela (o diálogo aberto): o tamanho da variante "Baixar" não custa
// nada, e encaixotar os campos só espalharia `Box` pelo formulário — mesma decisão de
// `tela_os::Dlg`.
#[allow(clippy::large_enum_variant)]
enum Dlg {
    #[default]
    Fechado,
    Lancar(FormLancar),
    Baixar {
        /// A parcela pelo `Id` — nunca pela posição no vetor, que muda quando a lista
        /// recarrega (depois de um estorno) ou é reordenada com o diálogo aberto.
        parcela: Id,
        valor: String,
        data: String,
        /// Meio (Dinheiro vai para o Caixa, Pix/cartão para o banco) e a conta bancária de
        /// destino, quando há mais de uma.
        pagamento: crate::pagamento::EstadoPagamento,
        /// O histórico de baixas da parcela — carregado uma vez na abertura do diálogo
        /// (`baixas_carregadas` marca isso, já que uma parcela recém-lançada legitimamente
        /// tem histórico vazio).
        baixas: Vec<ItemBaixa>,
        baixas_carregadas: bool,
        /// O motivo do estorno, digitado antes de clicar em "Estornar" numa baixa do
        /// histórico — `EstornarBaixa` exige um motivo não vazio.
        motivo_estorno: String,
        /// Quanto a parcela deve na `data` (juros/multa/desconto já calculados) e a data
        /// para a qual foi calculado — refeito quando a data muda.
        /// `None` dentro do par = a consulta falhou (sem permissão, por exemplo) e fica o
        /// principal que já estava no campo.
        situacao: Option<(String, Option<mod_financeiro::SituacaoNaData>)>,
        /// O valor que a tela sugeriu por último: enquanto o campo não for editado à mão,
        /// ele acompanha o total devido quando a data muda.
        valor_sugerido: String,
        /// A renegociação do saldo (seção recolhida no mesmo diálogo).
        renegociar: FormRenegociar,
    },
    /// Quitar várias parcelas selecionadas de uma vez (pelo total devido de cada uma).
    BaixarLote {
        data: String,
        pagamento: crate::pagamento::EstadoPagamento,
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
    /// Cadastro avulso, pelo botão do cabeçalho — não devolve a ninguém.
    Nenhum,
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
        pessoa: crate::pessoa::EstadoPessoa,
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

/// O lançamento: quem, o quê, quanto e como — "à vista" já nasce pago/recebido (título e
/// baixa num COMMIT, `quitado_agora`), "a prazo" fica em aberto em parcelas.
struct FormLancar {
    /// A receber ou a pagar — escolhido pelo botão, não pela aba (dá para lançar os dois de
    /// qualquer aba).
    a_receber: bool,
    /// Cliente/favorecido do catálogo, cadastrado ali mesmo ou ninguém (o padrão: a maioria
    /// dos lançamentos do dia — luz, aluguel, venda avulsa — não precisa de pessoa).
    pessoa: crate::pessoa::EstadoPessoa,
    /// O que é ("Conta de luz", "Conserto do notebook") — vira a descrição na grade.
    descricao: String,
    valor: String,
    /// Data do lançamento (emissão; também a da baixa quando à vista).
    data: String,
    categoria: Option<Id>,
    pagamento: crate::pagamento::EstadoPagamento,
}

impl FormLancar {
    fn novo(a_receber: bool) -> Self {
        Self {
            a_receber,
            ..Self::default()
        }
    }
}

impl Default for FormLancar {
    fn default() -> Self {
        Self {
            a_receber: true,
            pessoa: crate::pessoa::EstadoPessoa::sem_pessoa(),
            descricao: String::new(),
            valor: String::new(),
            data: Data::hoje(Fuso::BRASILIA).to_string(),
            categoria: None,
            pagamento: crate::pagamento::EstadoPagamento::novo(MeioPagamento::Pix),
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
    /// Filtro de situação da lista de parcelas; `None` = todas (o padrão).
    filtro_situacao: Option<FiltroParcelas>,
    /// Filtro de categoria da lista; `None` = todas.
    filtro_categoria: Option<FiltroCategoria>,
    /// Aba Custos: receitas em vez de custos, quantos meses, e a projeção carregada.
    custos_receitas: bool,
    custos_meses: u8,
    projecao: Vec<mod_financeiro::ItemProjecaoCategoria>,
    ordenacao: Ordenacao,
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
    /// O "em aberto" de cada espécie, carregado uma vez pelo painel e reaproveitado pela
    /// projeção das abas A receber/A pagar (antes cada uma consultava de novo).
    abertos_receber: Vec<ItemTituloEmAberto>,
    abertos_pagar: Vec<ItemTituloEmAberto>,
    /// Quantos títulos a geração de recorrências criou ao entrar — vira aviso no próximo
    /// quadro (o `carregar` não tem `egui::Context` para notificar).
    recorrencias_geradas: usize,
    /// Receita, custo das peças e margem das OS faturadas no mês (`os.margem_das_ordens…`);
    /// `None` sem permissão de ver OS (o cartão some).
    dash_margem_os: Option<mod_os::MargemDasOrdens>,
    /// `Some` = modo "baixar várias": as parcelas marcadas na grade.
    selecao: Option<Vec<Id>>,
    serie: Vec<MesFluxo>,
    // Fluxo de caixa / bancos.
    fluxo_dias: i64,
    extrato: Vec<ItemMovimentoDisponivel>,
    /// Rótulo amigável por `origem_id` ("OS #123 — João Silva"), resolvido uma vez ao
    /// carregar o extrato — pedido explícito do usuário: o fluxo de caixa deve dizer de onde
    /// veio o dinheiro. Resolvido aqui na UI, nunca no financeiro (`docs/contratos-internos.md`
    /// §7 regra 2 — o financeiro só sabe `origem_modulo`/`origem_id`, não o que é uma OS).
    origem_labels: HashMap<Id, String>,
    /// Receita, custo das peças e margem por OS (`os.margem_da_ordem`), consultada ao abrir a
    /// baixa de uma parcela da OS; `None` dentro = sem permissão ou OS não achada. Limpo a
    /// cada `carregar` (a OS pode ter mudado).
    margens_os: HashMap<Id, Option<mod_os::MargemDaOrdemServico>>,
    contas_disp: Vec<ItemContaDisponivel>,
    erro: Option<String>,
    dlg: Dlg,
    /// Cadastro rápido (cliente/fornecedor/categoria) aberto por cima do `dlg` principal —
    /// ver [`DlgRapido`].
    dlg_rapido: Option<DlgRapido>,
    /// Empresa ou pessoal.
    modo: Modo,
    /// A parte pessoal (carregada na primeira vez que a chave vai para "Pessoal").
    pessoal: pessoal::EstadoPessoal,
}

/// Desenha a tela inteira.
pub fn mostrar(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    if estado.recorrencias_geradas > 0 {
        let n = std::mem::take(&mut estado.recorrencias_geradas);
        notificar(
            ui.ctx(),
            Notificacao::info(if n == 1 {
                "1 título gerado de recorrência".to_owned()
            } else {
                format!("{n} títulos gerados de recorrências")
            }),
        );
    }

    let titulo = match estado.modo {
        Modo::Empresa => "Financeiro",
        Modo::Pessoal => "Financeiro pessoal",
    };
    LayoutTela::nova(titulo).mostrar(
        ui,
        estado,
        |ui, estado| match estado.modo {
            Modo::Empresa => acoes_empresa(ui, motor, sessao, estado),
            Modo::Pessoal => pessoal::acoes(ui, &mut estado.pessoal),
        },
        |ui, estado| {
            chave_modo(ui, estado);
            ui.add_space(Espaco::E12);
            if estado.modo == Modo::Pessoal {
                pessoal::corpo(ui, motor, sessao, &mut estado.pessoal);
                return;
            }
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
                Aba::Custos => painel_custos(ui, motor, sessao, estado),
                _ => lista(ui, motor, sessao, estado),
            }
        },
    );

    if estado.modo == Modo::Pessoal {
        pessoal::dialogos(ui.ctx(), motor, sessao, &mut estado.pessoal);
        return;
    }
    match estado.dlg {
        Dlg::Fechado => {}
        Dlg::Lancar(_) => dialogo_lancar(ui.ctx(), motor, sessao, estado),
        Dlg::Baixar { .. } => dialogo_baixar(ui.ctx(), motor, sessao, estado),
        Dlg::BaixarLote { .. } => dialogo_baixar_lote(ui.ctx(), motor, sessao, estado),
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

/// "CONTAS  [Da empresa | Pessoais]" — a chave entre o financeiro da empresa e o do usuário.
fn chave_modo(ui: &mut egui::Ui, estado: &mut EstadoTelaFinanceiro) {
    ui.horizontal(|ui| {
        ui.add(Rotulo::sobrelinha("Contas"));
        ui.add_space(Espaco::E8);
        if let Some(modo) =
            Abas::nova(&[(Modo::Empresa, "Da empresa"), (Modo::Pessoal, "Pessoais")])
                .selecionada(estado.modo)
                .id_salt("financeiro-modo")
                .mostrar(ui)
        {
            estado.modo = modo;
        }
    });
}

fn acoes_empresa(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    // Os dois lançamentos de qualquer aba; o da aba "A pagar" vem primeiro nela.
    let pagar_primeiro = estado.aba == Aba::Pagar;
    for a_receber in [!pagar_primeiro, pagar_primeiro] {
        let rot = if a_receber {
            "+ A receber"
        } else {
            "+ A pagar"
        };
        // Verde = dinheiro que entra, vermelho = que sai: o botão já diz o lado do caixa.
        let tom = if a_receber {
            Tom::Positivo
        } else {
            Tom::Negativo
        };
        let b = if a_receber == !pagar_primeiro {
            Botao::primario(rot).tom(tom).tecla(ATALHO_NOVO)
        } else {
            Botao::secundario(rot).tom(tom)
        };
        if ui.add(b).clicked() {
            estado.dlg = Dlg::Lancar(FormLancar::novo(a_receber));
        }
    }
    let (rot, papel) = if pagar_primeiro {
        ("+ Favorecido", Papel::Fornecedor)
    } else {
        ("+ Cliente", Papel::Cliente)
    };
    // Cadastros e configurações ficam um degrau abaixo dos lançamentos (fantasma), para o
    // olho achar primeiro "+ A receber"/"+ A pagar".
    if ui.add(Botao::fantasma(rot)).clicked() {
        estado.dlg_rapido = Some(DlgRapido::Pessoa {
            alvo: AlvoRapido::Nenhum,
            papel,
            pessoa: crate::pessoa::EstadoPessoa::cadastro(),
        });
    }
    if ui.add(Botao::fantasma("+ Categoria")).clicked() {
        estado.dlg = Dlg::NovaCategoria {
            nome: String::new(),
            especie: None,
        };
    }
    if ui.add(Botao::fantasma("Recorrências")).clicked() {
        estado.carregar_recorrencias(motor, sessao);
        estado.dlg = Dlg::Recorrencias;
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
        (Aba::Custos, "Custos por categoria"),
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

/// Recorte de categoria da lista de parcelas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FiltroCategoria {
    Todas,
    /// Só as sem categoria (para classificar).
    Sem,
    Uma(Id),
}

impl FiltroCategoria {
    fn combina(self, categoria: Option<Id>) -> bool {
        match self {
            Self::Todas => true,
            Self::Sem => categoria.is_none(),
            Self::Uma(id) => categoria == Some(id),
        }
    }
}

/// Recorte de situação da lista de parcelas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FiltroParcelas {
    Todas,
    EmAberto,
    Vencidas,
    Quitadas,
}

impl FiltroParcelas {
    fn combina(self, p: &ItemTituloEmAberto, hoje: Data) -> bool {
        match self {
            Self::Todas => true,
            Self::EmAberto => p.estado.aceita_baixa(),
            Self::Vencidas => p.estado.aceita_baixa() && p.vencimento < hoje,
            Self::Quitadas => p.estado == EstadoParcela::Quitada,
        }
    }
}

fn nome_mes(comp: Competencia) -> String {
    const M: [&str; 12] = [
        "jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez",
    ];
    let i = (comp.mes().clamp(1, 12) - 1) as usize;
    format!("{}/{:02}", M[i], comp.ano() % 100)
}

#[cfg(feature = "demo")]
impl EstadoTelaFinanceiro {
    /// Leva a tela à cena de demonstração (`demo_app`).
    pub fn preparar_demo(&mut self, motor: &MotorLocal, sessao: &SessaoLocal, cena: &str) {
        match cena {
            "financeiro" => {}
            "financeiro-custos" => self.aba = Aba::Custos,
            "financeiro-pagar" => self.aba = Aba::Pagar,
            "financeiro-fluxo" => self.aba = Aba::Fluxo,
            "financeiro-bancos" => self.aba = Aba::Bancos,
            "financeiro-pessoal" | "financeiro-cartoes" | "financeiro-pessoal-novo" => {
                self.modo = Modo::Pessoal;
                self.pessoal.preparar_demo(motor, sessao, cena);
            }
            _ => {
                self.aba = Aba::Receber;
                self.periodo.preset = PresetPeriodo::Semestre;
            }
        }
        self.carregar(motor, sessao);
        if cena == "financeiro-lancar" {
            let mut f = FormLancar::novo(false);
            f.pessoa.nenhuma = false;
            f.pessoa.novo = true;
            f.pessoa.nome = "Distribuidora Peças BH".to_owned();
            f.pessoa.telefone = "(31) 3333-4444".to_owned();
            f.descricao = "Tela de reposição — OS 12".to_owned();
            f.valor = "180,00".to_owned();
            self.dlg = Dlg::Lancar(f);
        }
        if cena == "financeiro-lote" {
            self.selecao = Some(
                self.parcelas
                    .iter()
                    .filter(|p| p.estado.aceita_baixa())
                    .map(|p| p.parcela)
                    .collect(),
            );
        }
        if cena == "financeiro-baixa" {
            let da_os = self.parcelas.iter().find(|p| p.origem_modulo == "os");
            if let Some(p) = da_os.or_else(|| self.parcelas.first()) {
                self.dlg = Dlg::Baixar {
                    parcela: p.parcela,
                    valor: p.saldo().formatar(),
                    data: Data::hoje(Fuso::BRASILIA).to_string(),
                    pagamento: crate::pagamento::EstadoPagamento::novo(MeioPagamento::Pix),
                    baixas: Vec::new(),
                    baixas_carregadas: false,
                    motivo_estorno: String::new(),
                    situacao: None,
                    valor_sugerido: String::new(),
                    renegociar: FormRenegociar::default(),
                };
            }
        }
    }
}
