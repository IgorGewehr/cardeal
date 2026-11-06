//! O DFC gerencial: o fluxo de caixa realizado do ano, **mês a mês e por conta do plano**, como
//! o dono do negócio lê o caixa — "recebi tanto de vendas, paguei tanto de aluguel" — e não
//! "recebi de clientes, paguei a fornecedores" como o DFC direto ([`crate::dfc`]).
//!
//! A diferença está na **natureza do título**. No Razão, o recebimento de uma parcela é
//! D Banco / C Clientes a receber: a contraparte é a conta de passagem, não a receita. Aqui cada
//! baixa (e o estorno dela) é ligada ao lançamento de competência da parcela — D Clientes a
//! receber / C Receita de vendas — e o dinheiro vai, na proporção, para as contas do **outro
//! lado** dele (a receita, a despesa da categoria, o estoque da compra). Do outro lado só contam
//! as contas de natureza diferente da passagem: na venda a prazo, a baixa de estoque do CMV
//! (C Estoque, um ativo como Clientes a receber) não é de onde o dinheiro veio.
//!
//! O que não se liga a um título (o cartão que a operadora pagou, o cheque compensado, o título
//! renegociado) fica na própria conta de passagem, com um rótulo que diz o que é. E a venda à
//! vista que traz o CMV no mesmo lançamento (D Caixa, C Receita, D CMV, C Estoque) tem o par
//! CMV/estoque anulado: ele não mexe no caixa.
//!
//! Os grupos seguem o recorte que o lojista conhece (o mesmo da DRE para as contas do resultado):
//! receitas operacionais, deduções e impostos, custos e compras, despesas operacionais,
//! resultado financeiro e as atividades de investimento e de financiamento. Ao lado do
//! realizado vem o **orçado** ([`crate::orcamento`]), no mesmo sinal do caixa, para o orçado ×
//! realizado e para a "visão DFC do orçamento".

use std::collections::HashMap;

#[cfg(feature = "sqlite")]
use cardeal_kernel::{CodigoErro, Data, Erro, Periodo, Resultado};
use cardeal_kernel::{Competencia, Dinheiro, Id, Percentual};
use cardeal_ledger::{LinhaResultado, MovimentoDeCaixa, Natureza, PapelConta};
use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// Os grupos do DFC gerencial, na ordem de exibição.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum GrupoGerencial {
    /// Vendas, serviços e as outras receitas que entraram.
    Receitas,
    /// Devoluções, descontos e os impostos sobre as vendas pagos.
    Deducoes,
    /// Compras de mercadoria e matéria-prima, custo dos serviços.
    Custos,
    /// Pessoal, ocupação, administrativas, comerciais.
    Despesas,
    /// Juros recebidos e pagos, tarifas financeiras.
    Financeiro,
    /// Imobilizado, aplicações.
    Investimentos,
    /// Empréstimos, aportes e retiradas dos sócios.
    Financiamentos,
    /// O que não cabe nos outros (conta corrente do grupo, contas patrimoniais sem papel).
    Outros,
}

impl GrupoGerencial {
    /// Todos, na ordem de exibição.
    pub const TODOS: [Self; 8] = [
        Self::Receitas,
        Self::Deducoes,
        Self::Custos,
        Self::Despesas,
        Self::Financeiro,
        Self::Investimentos,
        Self::Financiamentos,
        Self::Outros,
    ];

    /// O rótulo na tela.
    #[must_use]
    pub const fn rotulo(self) -> &'static str {
        match self {
            Self::Receitas => "Receitas operacionais",
            Self::Deducoes => "Deduções e impostos sobre as vendas",
            Self::Custos => "Custos e compras",
            Self::Despesas => "Despesas operacionais",
            Self::Financeiro => "Resultado financeiro",
            Self::Investimentos => "Atividades de investimento",
            Self::Financiamentos => "Atividades de financiamento",
            Self::Outros => "Outros movimentos",
        }
    }

    /// Se entra na geração de caixa operacional (receitas menos deduções, custos e despesas).
    #[must_use]
    pub const fn operacional(self) -> bool {
        matches!(
            self,
            Self::Receitas | Self::Deducoes | Self::Custos | Self::Despesas
        )
    }
}

/// Uma conta do plano como o DFC gerencial a enxerga.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContaDoPlano {
    /// A conta.
    pub id: Id,
    /// O código hierárquico (`"5.20"`).
    pub codigo: String,
    /// O nome.
    pub nome: String,
    /// A natureza.
    pub natureza: Natureza,
    /// Agrupadora (não recebe lançamento).
    pub sintetica: bool,
    /// O papel gravado na própria conta (o efetivo herda do ancestral).
    pub papel: Option<String>,
    /// `Operacional`, `Investimento` ou `Financiamento`, quando a conta classifica.
    pub grupo_fluxo: Option<String>,
    /// Aceita lançamento (a inativa ainda aparece se teve movimento).
    pub ativa: bool,
}

/// Uma conta patrimonial que se orça pelo caixa: a compra de mercadoria, o investimento, o
/// empréstimo, o aporte e a retirada — o que o planejamento do dono prevê e a DRE não mostra.
#[cfg(any(feature = "sqlite", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ContaOrcavelPeloCaixa {
    pub(crate) conta: Id,
    pub(crate) codigo: String,
    pub(crate) nome: String,
    /// "Compras", "Investimentos" ou "Financiamentos".
    pub(crate) bloco: &'static str,
    /// O orçado positivo entra no caixa (empréstimo tomado, aporte); senão sai.
    pub(crate) entra: bool,
}

/// As contas patrimoniais analíticas e ativas que se orçam pelo caixa, pela ordem do plano:
/// estoques (compras), imobilizado e aplicações (investimentos), empréstimos, capital e retiradas
/// (financiamentos). A depreciação e a amortização não: não passam pelo caixa.
#[cfg(any(feature = "sqlite", test))]
pub(crate) fn contas_orcaveis_pelo_caixa(contas: &[ContaDoPlano]) -> Vec<ContaOrcavelPeloCaixa> {
    let plano = Plano::novo(contas);
    let mut saida: Vec<ContaOrcavelPeloCaixa> = contas
        .iter()
        .filter(|c| {
            !c.sintetica
                && c.ativa
                && matches!(
                    c.natureza,
                    Natureza::Ativo | Natureza::Passivo | Natureza::PatrimonioLiquido
                )
                && !plano.e_caixa(&c.codigo)
        })
        .filter_map(|c| {
            let papel = plano.papel(&c.codigo).unwrap_or("");
            let nome = c.nome.to_lowercase();
            let bloco = if papel.starts_with("Estoque") {
                "Compras"
            } else if (c.codigo.starts_with("1.4.")
                && !nome.starts_with("deprecia")
                && !nome.starts_with("amortiza"))
                || papel == "Aplicacoes"
                || c.grupo_fluxo.as_deref() == Some("Investimento")
            {
                "Investimentos"
            } else if matches!(papel, "Emprestimos" | "Capital" | "Retiradas")
                || c.grupo_fluxo.as_deref() == Some("Financiamento")
            {
                "Financiamentos"
            } else {
                return None;
            };
            Some(ContaOrcavelPeloCaixa {
                conta: c.id,
                codigo: c.codigo.clone(),
                nome: c.nome.clone(),
                bloco,
                entra: plano.sinal_do_orcado(c) > 0,
            })
        })
        .collect();
    saida.sort_by_key(|a| chave_de_codigo(&a.codigo));
    saida
}

/// Uma linha (uma conta) do DFC gerencial.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinhaGerencial {
    /// A conta.
    pub conta: Id,
    /// O código da conta.
    pub codigo: String,
    /// O rótulo ("Aluguel", "Compras: Mercadorias para revenda", "Recebimentos de cartões").
    pub nome: String,
    /// A agrupadora intermediária da conta do resultado ("Despesas administrativas"), quando o
    /// plano tem uma entre a raiz e a conta.
    pub agrupadora: Option<String>,
    /// O que entrou (positivo) ou saiu (negativo) do caixa em cada mês, de janeiro a dezembro.
    pub realizado: [Dinheiro; 12],
    /// O que está em aberto para entrar ou sair, pelo vencimento (as parcelas a receber e a pagar
    /// ligadas à conta pelo título) — o vencido e ainda em aberto conta no mês de hoje.
    pub a_vencer: [Dinheiro; 12],
    /// O orçado de cada mês, no mesmo sinal.
    pub orcado: [Dinheiro; 12],
}

impl LinhaGerencial {
    /// O realizado de janeiro até o mês `ate` (1 a 12).
    #[must_use]
    pub fn realizado_ate(&self, ate: u32) -> Dinheiro {
        soma_ate(&self.realizado, ate)
    }

    /// O orçado de janeiro até o mês `ate` (1 a 12).
    #[must_use]
    pub fn orcado_ate(&self, ate: u32) -> Dinheiro {
        soma_ate(&self.orcado, ate)
    }
}

/// Um grupo do DFC gerencial com as suas contas e os totais de cada mês.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrupoDfcGerencial {
    /// O grupo.
    pub grupo: GrupoGerencial,
    /// As contas, pela ordem do plano.
    pub linhas: Vec<LinhaGerencial>,
    /// O realizado do grupo em cada mês.
    pub realizado: [Dinheiro; 12],
    /// O a vencer do grupo em cada mês.
    pub a_vencer: [Dinheiro; 12],
    /// O orçado do grupo em cada mês.
    pub orcado: [Dinheiro; 12],
}

/// O DFC gerencial de um ano.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DfcGerencial {
    /// O ano.
    pub ano: i32,
    /// O caixa no início de cada mês.
    pub saldo_inicial: [Dinheiro; 12],
    /// Os grupos com movimento ou orçamento, na ordem de [`GrupoGerencial::TODOS`].
    pub grupos: Vec<GrupoDfcGerencial>,
    /// A variação do caixa em cada mês (a soma dos grupos).
    pub variacao: [Dinheiro; 12],
    /// O caixa no fim de cada mês.
    pub saldo_final: [Dinheiro; 12],
    /// Quanto passou por clientes a receber ou fornecedores sem se ligar a um título (em valor
    /// absoluto): o que ficou nas linhas "sem conta de origem".
    pub sem_origem: Dinheiro,
}

impl DfcGerencial {
    /// A geração de caixa operacional de cada mês: receitas, deduções, custos e despesas.
    #[must_use]
    pub fn geracao_operacional(&self) -> [Dinheiro; 12] {
        self.somar(|g| g.grupo.operacional(), |g| g.realizado)
    }

    /// A geração de caixa operacional orçada de cada mês.
    #[must_use]
    pub fn geracao_operacional_orcada(&self) -> [Dinheiro; 12] {
        self.somar(|g| g.grupo.operacional(), |g| g.orcado)
    }

    /// A variação orçada do caixa em cada mês (todos os grupos).
    #[must_use]
    pub fn variacao_orcada(&self) -> [Dinheiro; 12] {
        self.somar(|_| true, |g| g.orcado)
    }

    /// O que está a vencer em cada mês (todos os grupos): o que entra menos o que sai.
    #[must_use]
    pub fn variacao_a_vencer(&self) -> [Dinheiro; 12] {
        self.somar(|_| true, |g| g.a_vencer)
    }

    /// Se há algo a vencer no ano.
    #[must_use]
    pub fn tem_a_vencer(&self) -> bool {
        self.grupos
            .iter()
            .any(|g| g.a_vencer.iter().any(|v| !v.e_zero()))
    }

    /// As receitas operacionais de cada mês: a base da análise vertical (o "%" de cada linha).
    #[must_use]
    pub fn receitas(&self) -> [Dinheiro; 12] {
        self.somar(|g| g.grupo == GrupoGerencial::Receitas, |g| g.realizado)
    }

    /// Se há algum orçamento no ano.
    #[must_use]
    pub fn tem_orcamento(&self) -> bool {
        self.grupos
            .iter()
            .any(|g| g.orcado.iter().any(|v| !v.e_zero()))
    }

    fn somar(
        &self,
        filtro: impl Fn(&GrupoDfcGerencial) -> bool,
        valores: impl Fn(&GrupoDfcGerencial) -> [Dinheiro; 12],
    ) -> [Dinheiro; 12] {
        let mut t = [Dinheiro::ZERO; 12];
        for g in self.grupos.iter().filter(|g| filtro(g)) {
            for (a, v) in t.iter_mut().zip(valores(g)) {
                *a += v;
            }
        }
        t
    }
}

/// A soma de janeiro até o mês `ate` (1 a 12).
#[must_use]
pub fn soma_ate(meses: &[Dinheiro; 12], ate: u32) -> Dinheiro {
    meses.iter().take(ate.min(12) as usize).copied().sum()
}

/// A parte sobre o todo (a análise vertical: a linha em % das receitas); `None` sem todo.
#[must_use]
pub fn percentual(parte: Dinheiro, todo: Dinheiro) -> Option<Percentual> {
    (!todo.e_zero()).then(|| Percentual::da_razao(parte.em_centavos(), todo.em_centavos().abs()))
}

/// O quanto do orçado se realizou (realizado ÷ orçado); `None` sem orçado. Os dois têm o sinal do
/// caixa, então 90% numa despesa é ter gasto 90% do previsto.
#[must_use]
pub fn atingido(realizado: Dinheiro, orcado: Dinheiro) -> Option<Percentual> {
    (!orcado.e_zero()).then(|| Percentual::da_razao(realizado.em_centavos(), orcado.em_centavos()))
}

/// As partidas do lançamento de competência de um título, para cada lançamento de caixa que é
/// baixa (ou estorno de baixa) dele: (código da conta, valor com sinal, débito positivo).
pub type OrigensDasBaixas = HashMap<Id, Vec<(String, Dinheiro)>>;

struct Plano<'a> {
    por_codigo: HashMap<&'a str, &'a ContaDoPlano>,
    por_id: HashMap<Id, &'a ContaDoPlano>,
}

impl<'a> Plano<'a> {
    fn novo(contas: &'a [ContaDoPlano]) -> Self {
        Self {
            por_codigo: contas.iter().map(|c| (c.codigo.as_str(), c)).collect(),
            por_id: contas.iter().map(|c| (c.id, c)).collect(),
        }
    }

    fn conta(&self, codigo: &str) -> Option<&'a ContaDoPlano> {
        self.por_codigo.get(codigo).copied()
    }

    /// O papel da conta ou, na falta, o do ancestral mais próximo que tenha um.
    fn papel(&self, codigo: &str) -> Option<&'a str> {
        let mut atual = codigo;
        loop {
            if let Some(p) = self.conta(atual).and_then(|c| c.papel.as_deref()) {
                return Some(p);
            }
            atual = &atual[..atual.rfind('.')?];
        }
    }

    /// Caixa: o que tem papel `Caixa`, `Bancos` ou `ValoresEmTransito` e toda conta sob "1.1
    /// Disponível", menos as aplicações — o mesmo recorte do Razão (`cardeal_ledger::caixa`).
    fn e_caixa(&self, codigo: &str) -> bool {
        match self.papel(codigo) {
            Some("Caixa" | "Bancos" | "ValoresEmTransito") => true,
            Some("Aplicacoes") => false,
            _ => codigo.starts_with("1.1."),
        }
    }

    fn linha_de_resultado(&self, c: &ContaDoPlano) -> LinhaResultado {
        LinhaResultado {
            conta: c.id,
            codigo: c.codigo.clone(),
            nome: c.nome.clone(),
            natureza: c.natureza,
            papel: self.papel(&c.codigo).map(str::to_owned),
            valor: Dinheiro::ZERO,
        }
    }

    fn grupo(&self, c: &ContaDoPlano) -> GrupoGerencial {
        use GrupoGerencial as G;
        if matches!(c.natureza, Natureza::Receita | Natureza::Despesa) {
            return crate::dre::grupo_gerencial(&self.linha_de_resultado(c));
        }
        let papel = self.papel(&c.codigo).unwrap_or("");
        let e = |p: PapelConta| papel == p.to_string();
        match c.grupo_fluxo.as_deref() {
            Some("Investimento") => return G::Investimentos,
            Some("Financiamento") => return G::Financiamentos,
            _ => {}
        }
        if c.codigo == "1.4" || c.codigo.starts_with("1.4.") {
            // O imobilizado do plano padrão (móveis, máquinas) não leva grupo de fluxo.
            G::Investimentos
        } else if papel.starts_with("Estoque")
            || e(PapelConta::Fornecedores)
            || e(PapelConta::AdiantamentoFornecedor)
            || e(PapelConta::ImpostosARecuperar)
        {
            G::Custos
        } else if e(PapelConta::ClientesAReceber)
            || e(PapelConta::CartoesAReceber)
            || e(PapelConta::ChequesAReceber)
            || e(PapelConta::AdiantamentoCliente)
        {
            G::Receitas
        } else if e(PapelConta::ImpostosARecolher) {
            G::Deducoes
        } else if e(PapelConta::SalariosAPagar) || e(PapelConta::CartoesARepassar) {
            G::Despesas
        } else if e(PapelConta::Capital) || e(PapelConta::Retiradas) || e(PapelConta::Emprestimos) {
            G::Financiamentos
        } else {
            G::Outros
        }
    }

    /// O rótulo da linha: o nome da conta, ou o que a conta de passagem quer dizer no caixa.
    fn rotulo(&self, c: &ContaDoPlano) -> String {
        let papel = self.papel(&c.codigo).unwrap_or("");
        let e = |p: PapelConta| papel == p.to_string();
        if matches!(c.natureza, Natureza::Receita | Natureza::Despesa) {
            c.nome.clone()
        } else if papel.starts_with("Estoque") {
            format!("Compras: {}", c.nome)
        } else if e(PapelConta::ClientesAReceber) {
            "Recebimentos de clientes (sem conta de origem)".to_owned()
        } else if e(PapelConta::Fornecedores) {
            "Pagamentos a fornecedores (sem conta de origem)".to_owned()
        } else if e(PapelConta::CartoesAReceber) {
            "Recebimentos de cartões".to_owned()
        } else if e(PapelConta::ChequesAReceber) {
            "Cheques compensados".to_owned()
        } else if e(PapelConta::ImpostosARecolher) {
            "Impostos recolhidos".to_owned()
        } else if e(PapelConta::SalariosAPagar) {
            "Salários e encargos pagos".to_owned()
        } else {
            c.nome.clone()
        }
    }

    /// A agrupadora entre a raiz e a conta do resultado, quando há uma ("5.50 Despesas
    /// administrativas" de "5.50.01 Aluguel").
    fn agrupadora(&self, c: &ContaDoPlano) -> Option<String> {
        if !matches!(c.natureza, Natureza::Receita | Natureza::Despesa) {
            return None;
        }
        let pai = &c.codigo[..c.codigo.rfind('.')?];
        if !pai.contains('.') {
            return None;
        }
        self.conta(pai)
            .filter(|p| p.sintetica)
            .map(|p| p.nome.clone())
    }

    /// O sinal do orçado no caixa: a conta se orça "no sentido em que se comporta" (a receita
    /// vendida, a despesa paga, a compra feita, o empréstimo tomado), e isso entra ou sai.
    fn sinal_do_orcado(&self, c: &ContaDoPlano) -> i64 {
        match c.natureza {
            Natureza::Receita | Natureza::Despesa => {
                if crate::dre::soma_no_resultado(&self.linha_de_resultado(c)) {
                    1
                } else {
                    -1
                }
            }
            Natureza::Ativo => -1,
            Natureza::Passivo => 1,
            Natureza::PatrimonioLiquido => {
                if self.papel(&c.codigo) == Some("Retiradas") {
                    -1
                } else {
                    1
                }
            }
        }
    }
}

/// Reparte `valor` na proporção dos `pesos` (todos do mesmo sinal), sem perder centavo: o resto
/// do arredondamento vai para o maior peso.
fn repartir(valor: Dinheiro, pesos: &[Dinheiro]) -> Vec<Dinheiro> {
    let total: i128 = pesos.iter().map(|p| i128::from(p.em_centavos())).sum();
    if total == 0 {
        return vec![Dinheiro::ZERO; pesos.len()];
    }
    let v = i128::from(valor.em_centavos());
    let mut partes: Vec<i128> = pesos
        .iter()
        .map(|p| v * i128::from(p.em_centavos()) / total)
        .collect();
    let resto = v - partes.iter().sum::<i128>();
    if let Some(maior) = pesos
        .iter()
        .enumerate()
        .max_by_key(|(_, p)| p.em_centavos().abs())
        .map(|(i, _)| i)
    {
        partes[maior] += resto;
    }
    partes
        .into_iter()
        .map(|p| Dinheiro::centavos(i64::try_from(p).unwrap_or(0)))
        .collect()
}

/// O que a contrapartida de passagem (`codigo`, com `valor` no lançamento de caixa) carrega,
/// repartido pelas contas do outro lado do lançamento do título. `None` quando não dá para ligar:
/// a passagem não está no título, está dos dois lados dele (renegociação) ou o outro lado está
/// vazio.
fn pela_origem(
    plano: &Plano,
    codigo: &str,
    valor: Dinheiro,
    origem: &[(String, Dinheiro)],
) -> Option<Vec<(String, Dinheiro)>> {
    let na_passagem: Vec<Dinheiro> = origem
        .iter()
        .filter(|(c, _)| c == codigo)
        .map(|(_, v)| *v)
        .collect();
    let positivas = na_passagem.iter().any(|v| v.e_positivo());
    let negativas = na_passagem.iter().any(|v| v.e_negativo());
    if positivas == negativas {
        // Fora do título, ou dos dois lados dele.
        return None;
    }
    let natureza = plano.conta(codigo).map(|c| c.natureza);
    let outro_lado: Vec<(String, Dinheiro)> = origem
        .iter()
        .filter(|(c, v)| {
            c != codigo
                && (if positivas {
                    v.e_negativo()
                } else {
                    v.e_positivo()
                })
                && !plano.e_caixa(c)
                && plano.conta(c).map(|k| k.natureza) != natureza
        })
        .cloned()
        .collect();
    if outro_lado.is_empty() {
        return None;
    }
    let pesos: Vec<Dinheiro> = outro_lado.iter().map(|(_, v)| *v).collect();
    let partes = repartir(valor, &pesos);
    Some(
        outro_lado
            .into_iter()
            .zip(partes)
            .map(|((c, _), v)| (c, v))
            .collect(),
    )
}

/// Anula, num lançamento de caixa, o par CMV × estoque da venda à vista (D CMV / C Estoque no
/// mesmo lançamento da receita): ele não mexe no caixa e apareceria como um custo e uma "compra"
/// negativa que se cancelam.
fn anular_custo_da_venda(plano: &Plano, partes: &mut Vec<(String, Dinheiro)>) {
    let e_custo = |c: &str| matches!(plano.papel(c), Some("Cmv" | "CustoServico"));
    let e_estoque = |c: &str| plano.papel(c).is_some_and(|p| p.starts_with("Estoque"));
    let custo: Dinheiro = partes
        .iter()
        .filter(|(c, v)| e_custo(c) && v.e_positivo())
        .map(|(_, v)| *v)
        .sum();
    let baixa: Dinheiro = partes
        .iter()
        .filter(|(c, v)| e_estoque(c) && v.e_negativo())
        .map(|(_, v)| -*v)
        .sum();
    let mut a_anular = custo.min(baixa);
    if !a_anular.e_positivo() {
        return;
    }
    let mut no_estoque = a_anular;
    for (c, v) in partes.iter_mut() {
        if e_custo(c) && v.e_positivo() && a_anular.e_positivo() {
            let tira = (*v).min(a_anular);
            *v -= tira;
            a_anular -= tira;
        } else if e_estoque(c) && v.e_negativo() && no_estoque.e_positivo() {
            let tira = (-*v).min(no_estoque);
            *v += tira;
            no_estoque -= tira;
        }
    }
    partes.retain(|(_, v)| !v.e_zero());
}

#[derive(Default)]
struct Acumulado {
    realizado: [Dinheiro; 12],
    a_vencer: [Dinheiro; 12],
    orcado: [Dinheiro; 12],
}

/// Soma os movimentos do ano nas linhas (pelo código; fora do plano, pelo nome com `?`) — no
/// realizado ou no a vencer — e devolve a variação do caixa de cada mês e o que ficou sem título
/// de origem.
fn acumular(
    plano: &Plano,
    ano: i32,
    movimentos: &[MovimentoDeCaixa],
    origens: &OrigensDasBaixas,
    linhas: &mut HashMap<String, Acumulado>,
    a_vencer: bool,
) -> ([Dinheiro; 12], Dinheiro) {
    let mut variacao = [Dinheiro::ZERO; 12];
    let mut sem_origem = Dinheiro::ZERO;
    for m in movimentos {
        let c = m.data.competencia();
        if c.ano() != ano {
            continue;
        }
        let mes = c.mes() as usize - 1;
        variacao[mes] += m.caixa;
        let origem = origens.get(&m.lancamento);
        let mut partes: Vec<(String, Dinheiro)> = Vec::new();
        for cp in &m.contrapartidas {
            if let Some(repartida) =
                origem.and_then(|o| pela_origem(plano, &cp.codigo, cp.valor, o.as_slice()))
            {
                partes.extend(repartida);
                continue;
            }
            if matches!(
                cp.papel.as_deref(),
                Some("ClientesAReceber" | "Fornecedores")
            ) {
                sem_origem += cp.valor.abs();
            }
            let chave = if plano.conta(&cp.codigo).is_some() {
                cp.codigo.clone()
            } else {
                format!("?{}", cp.nome)
            };
            partes.push((chave, cp.valor));
        }
        anular_custo_da_venda(plano, &mut partes);
        for (chave, valor) in partes {
            // O que a contrapartida carrega é, com o sinal trocado, o que entrou no caixa.
            let a = linhas.entry(chave).or_default();
            if a_vencer {
                a.a_vencer[mes] -= valor;
            } else {
                a.realizado[mes] -= valor;
            }
        }
    }
    (variacao, sem_origem)
}

/// As contas com realizado ou orçado, nos grupos, pela ordem do plano, com os totais.
fn agrupar(plano: &Plano, linhas: HashMap<String, Acumulado>) -> Vec<GrupoDfcGerencial> {
    let mut por_grupo: HashMap<GrupoGerencial, Vec<LinhaGerencial>> = HashMap::new();
    for (chave, a) in linhas {
        if a.realizado
            .iter()
            .chain(&a.a_vencer)
            .chain(&a.orcado)
            .all(|v| v.e_zero())
        {
            continue;
        }
        let (grupo, linha) = match plano.conta(&chave) {
            Some(c) => (
                plano.grupo(c),
                LinhaGerencial {
                    conta: c.id,
                    codigo: c.codigo.clone(),
                    nome: plano.rotulo(c),
                    agrupadora: plano.agrupadora(c),
                    realizado: a.realizado,
                    a_vencer: a.a_vencer,
                    orcado: a.orcado,
                },
            ),
            None => (
                GrupoGerencial::Outros,
                LinhaGerencial {
                    conta: Id::NULO,
                    codigo: String::new(),
                    nome: chave.trim_start_matches('?').to_owned(),
                    agrupadora: None,
                    realizado: a.realizado,
                    a_vencer: a.a_vencer,
                    orcado: a.orcado,
                },
            ),
        };
        por_grupo.entry(grupo).or_default().push(linha);
    }
    GrupoGerencial::TODOS
        .iter()
        .filter_map(|g| {
            let mut ls = por_grupo.remove(g)?;
            ls.sort_by(|a, b| {
                chave_de_codigo(&a.codigo)
                    .cmp(&chave_de_codigo(&b.codigo))
                    .then_with(|| a.nome.cmp(&b.nome))
            });
            let mut realizado = [Dinheiro::ZERO; 12];
            let mut a_vencer = [Dinheiro::ZERO; 12];
            let mut orcado = [Dinheiro::ZERO; 12];
            for l in &ls {
                for i in 0..12 {
                    realizado[i] += l.realizado[i];
                    a_vencer[i] += l.a_vencer[i];
                    orcado[i] += l.orcado[i];
                }
            }
            Some(GrupoDfcGerencial {
                grupo: *g,
                linhas: ls,
                realizado,
                a_vencer,
                orcado,
            })
        })
        .collect()
}

/// Monta o DFC gerencial do ano a partir do caixa de 1º de janeiro, dos movimentos de caixa do
/// ano, das parcelas em aberto (como movimentos na data em que se espera o dinheiro, ligadas ao
/// título pelas mesmas origens), das origens das baixas, do plano e do orçado de cada conta (no
/// sentido em que se orça).
#[must_use]
pub fn montar(
    ano: i32,
    abertura: Dinheiro,
    movimentos: &[MovimentoDeCaixa],
    a_vencer: &[MovimentoDeCaixa],
    origens: &OrigensDasBaixas,
    contas: &[ContaDoPlano],
    orcado: &[(Id, [Dinheiro; 12])],
) -> DfcGerencial {
    let plano = Plano::novo(contas);
    let mut linhas: HashMap<String, Acumulado> = HashMap::new();
    let (variacao, sem_origem) = acumular(&plano, ano, movimentos, origens, &mut linhas, false);
    let _ = acumular(&plano, ano, a_vencer, origens, &mut linhas, true);
    for (conta, meses) in orcado {
        let Some(c) = plano.por_id.get(conta) else {
            continue;
        };
        let sinal = plano.sinal_do_orcado(c);
        let a = linhas.entry(c.codigo.clone()).or_default();
        for (o, v) in a.orcado.iter_mut().zip(meses) {
            *o += if sinal > 0 { *v } else { -*v };
        }
    }
    let grupos = agrupar(&plano, linhas);
    let mut saldo_inicial = [Dinheiro::ZERO; 12];
    let mut saldo_final = [Dinheiro::ZERO; 12];
    let mut saldo = abertura;
    for i in 0..12 {
        saldo_inicial[i] = saldo;
        saldo += variacao[i];
        saldo_final[i] = saldo;
    }
    DfcGerencial {
        ano,
        saldo_inicial,
        grupos,
        variacao,
        saldo_final,
        sem_origem,
    }
}

fn chave_de_codigo(c: &str) -> Vec<u32> {
    if c.is_empty() {
        return vec![u32::MAX];
    }
    c.split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

/// O DFC gerencial de um ano: realizado e orçado, mês a mês, por conta.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DfcGerencialDoAno {
    /// O ano.
    pub ano: i32,
}

impl DfcGerencialDoAno {
    #[cfg(feature = "sqlite")]
    fn apurar(
        self,
        conexao: &Connection,
        empresa: Id,
        orcado: &[(Id, [Dinheiro; 12])],
    ) -> Resultado<DfcGerencial> {
        let periodo = Periodo::novo(
            Data::de_ymd(self.ano, 1, 1)?,
            Data::de_ymd(self.ano, 12, 31)?,
        );
        let (abertura, movimentos) = cardeal_ledger::movimentos_de_caixa(conexao, empresa, periodo)
            .map_err(|e| {
                Erro::novo(
                    CodigoErro::FALHA_INTERNA,
                    format!("financeiro/DFC gerencial: {e}"),
                )
            })?;
        let contas = sqlite::plano(conexao, empresa)?;
        let mut origens = sqlite::origens(conexao, empresa, periodo)?;
        let a_vencer = sqlite::a_vencer(
            conexao,
            empresa,
            self.ano,
            Data::hoje(cardeal_kernel::Fuso::BRASILIA),
            &contas,
            &mut origens,
        )?;
        Ok(montar(
            self.ano,
            abertura,
            &movimentos,
            &a_vencer,
            &origens,
            &contas,
            orcado,
        ))
    }
}

impl Consulta for DfcGerencialDoAno {
    type Saida = DfcGerencial;
    const PERMISSAO: &'static str = "financeiro.dre.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<DfcGerencial> {
        let orcado = crate::orcamento::sqlite::orcado_por_conta(conexao, ctx.empresa, self.ano)?;
        self.apurar(conexao, ctx.empresa, &orcado)
    }
}

/// O que entrou (positivo) ou saiu (negativo) do caixa por conta em cada mês do ano — o realizado
/// das contas que se orçam pelo caixa ([`crate::orcamento`]).
///
/// # Errors
/// Os da leitura do Razão.
#[cfg(feature = "sqlite")]
pub(crate) fn caixa_por_conta(
    conexao: &Connection,
    empresa: Id,
    ano: i32,
) -> Resultado<HashMap<Id, [Dinheiro; 12]>> {
    let d = DfcGerencialDoAno { ano }.apurar(conexao, empresa, &[])?;
    Ok(d.grupos
        .into_iter()
        .flat_map(|g| g.linhas)
        .filter(|l| l.conta != Id::NULO)
        .map(|l| (l.conta, l.realizado))
        .collect())
}

/// O mês do ano (1 a 12) de uma competência; `None` noutro ano.
#[must_use]
pub fn mes_do_ano(c: Competencia, ano: i32) -> Option<u32> {
    (c.ano() == ano).then(|| c.mes())
}

#[cfg(feature = "sqlite")]
pub(crate) mod sqlite {
    use cardeal_kernel::{CodigoErro, Data, Dinheiro, Erro, Id, Periodo, Resultado};
    use cardeal_ledger::{ContrapartidaDeCaixa, MovimentoDeCaixa, Natureza};
    use rusqlite::{params, Connection};

    use super::{ContaDoPlano, OrigensDasBaixas};

    #[allow(clippy::needless_pass_by_value)]
    fn falha(e: rusqlite::Error) -> Erro {
        Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string())
    }

    fn id_de(b: &[u8]) -> Id {
        <[u8; 16]>::try_from(b).map_or(Id::NULO, Id::de_bytes)
    }

    fn natureza_de(s: &str) -> Natureza {
        match s {
            "Ativo" => Natureza::Ativo,
            "Passivo" => Natureza::Passivo,
            "PatrimonioLiquido" => Natureza::PatrimonioLiquido,
            "Receita" => Natureza::Receita,
            _ => Natureza::Despesa,
        }
    }

    /// Todas as contas da empresa (as inativas também: podem ter movimento no ano).
    pub(crate) fn plano(c: &Connection, empresa: Id) -> Resultado<Vec<ContaDoPlano>> {
        let mut stmt = c
            .prepare(
                "SELECT id, codigo, nome, natureza, tipo, papel_padrao, grupo_fluxo, ativa
                 FROM razao_conta WHERE empresa = ?1",
            )
            .map_err(falha)?;
        let contas = stmt
            .query_map([empresa.em_bytes().as_slice()], |r| {
                Ok(ContaDoPlano {
                    id: id_de(&r.get::<_, Vec<u8>>(0)?),
                    codigo: r.get(1)?,
                    nome: r.get(2)?,
                    natureza: natureza_de(&r.get::<_, String>(3)?),
                    sintetica: r.get::<_, String>(4)? == "Sintetica",
                    papel: r.get(5)?,
                    grupo_fluxo: r.get(6)?,
                    ativa: r.get::<_, i64>(7)? != 0,
                })
            })
            .map_err(falha)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(falha)?;
        Ok(contas)
    }

    /// id, vencimento, saldo, espécie e o lançamento de competência de uma parcela em aberto.
    type Parcela = (Vec<u8>, i64, i64, String, Option<Vec<u8>>);

    /// As parcelas em aberto como movimentos de caixa esperados: na data do vencimento (ou hoje,
    /// para a vencida), entrando (a receber) ou saindo (a pagar) pelo saldo, com a contrapartida
    /// em clientes a receber ou fornecedores — e, nas `origens`, as partidas do lançamento de
    /// competência da parcela, para o dinheiro ir para a conta do título como numa baixa. Só as
    /// que caem no `ano`.
    pub(super) fn a_vencer(
        c: &Connection,
        empresa: Id,
        ano: i32,
        hoje: Data,
        contas: &[ContaDoPlano],
        origens: &mut OrigensDasBaixas,
    ) -> Resultado<Vec<MovimentoDeCaixa>> {
        let fim = Data::de_ymd(ano, 12, 31)?;
        let mut stmt = c
            .prepare(
                "SELECT p.id, p.vencimento, p.valor - p.valor_baixado, t.especie, p.lancamento
                 FROM financeiro_parcela p JOIN financeiro_titulo t ON t.id = p.titulo
                 WHERE t.empresa = ?1 AND p.estado IN ('Aberta','Parcial')
                   AND p.vencimento <= ?2 AND p.valor > p.valor_baixado",
            )
            .map_err(falha)?;
        let parcelas: Vec<Parcela> = stmt
            .query_map(
                params![empresa.em_bytes().as_slice(), i64::from(fim.em_dias())],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .map_err(falha)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(falha)?;
        let plano = super::Plano::novo(contas);
        let passagem_do_papel = |papel: &str| {
            contas
                .iter()
                .filter(|k| !k.sintetica && plano.papel(&k.codigo) == Some(papel))
                .min_by_key(|k| (k.codigo.len(), k.codigo.clone()))
                .map(|k| (k.codigo.clone(), k.nome.clone()))
        };
        let mut partidas = c
            .prepare(
                "SELECT rc.codigo, rp.valor FROM razao_partida rp
                 JOIN razao_conta rc ON rc.id = rp.conta
                 WHERE rp.lancamento = ?1 ORDER BY rp.ordem",
            )
            .map_err(falha)?;
        let mut saida = Vec::new();
        for (id, venc, saldo, especie, lancamento) in parcelas {
            let vencimento = Data::de_dias(i32::try_from(venc).unwrap_or(0));
            let data = if vencimento < hoje { hoje } else { vencimento };
            if data.ano() != ano {
                continue;
            }
            let receber = especie == "Receber";
            let papel = if receber {
                "ClientesAReceber"
            } else {
                "Fornecedores"
            };
            let parcela = id_de(&id);
            let origem: Vec<(String, Dinheiro)> = match &lancamento {
                Some(l) => partidas
                    .query_map([l.as_slice()], |r| {
                        Ok((r.get::<_, String>(0)?, Dinheiro::centavos(r.get(1)?)))
                    })
                    .map_err(falha)?
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(falha)?,
                None => Vec::new(),
            };
            // A passagem: a conta de clientes/fornecedores do próprio título, ou a do papel.
            let passagem = origem
                .iter()
                .find(|(k, _)| plano.papel(k) == Some(papel))
                .and_then(|(k, _)| plano.conta(k).map(|c| (c.codigo.clone(), c.nome.clone())))
                .or_else(|| passagem_do_papel(papel));
            let Some((codigo, nome)) = passagem else {
                continue;
            };
            let caixa = if receber {
                Dinheiro::centavos(saldo)
            } else {
                -Dinheiro::centavos(saldo)
            };
            if !origem.is_empty() {
                origens.insert(parcela, origem);
            }
            saida.push(MovimentoDeCaixa {
                lancamento: parcela,
                data,
                caixa,
                contrapartidas: vec![ContrapartidaDeCaixa {
                    codigo,
                    nome,
                    papel: Some(papel.to_owned()),
                    grupo_fluxo: None,
                    valor: -caixa,
                }],
            });
        }
        Ok(saida)
    }

    /// Para cada lançamento de caixa do período que é uma baixa — ou o estorno de uma —, as
    /// partidas do lançamento de competência da parcela baixada.
    pub(super) fn origens(
        c: &Connection,
        empresa: Id,
        periodo: Periodo,
    ) -> Resultado<OrigensDasBaixas> {
        let mut stmt = c
            .prepare(
                "WITH caixa(id) AS (
                     SELECT id FROM razao_lancamento
                     WHERE empresa = ?1 AND estado IN ('Realizado','Estornado')
                       AND COALESCE(liquidacao, competencia) BETWEEN ?2 AND ?3
                 ),
                 ligacao(lanc, origem) AS (
                     SELECT b.lancamento, p.lancamento
                     FROM financeiro_baixa b JOIN financeiro_parcela p ON p.id = b.parcela
                     WHERE b.empresa = ?1 AND p.lancamento IS NOT NULL
                       AND b.lancamento IN (SELECT id FROM caixa)
                     UNION ALL
                     SELECT rl.id, p.lancamento
                     FROM razao_lancamento rl
                     JOIN financeiro_baixa b ON b.lancamento = rl.estorna
                     JOIN financeiro_parcela p ON p.id = b.parcela
                     WHERE rl.empresa = ?1 AND p.lancamento IS NOT NULL
                       AND rl.id IN (SELECT id FROM caixa)
                 )
                 SELECT l.lanc, rc.codigo, rp.valor
                 FROM ligacao l
                 JOIN razao_partida rp ON rp.lancamento = l.origem
                 JOIN razao_conta rc ON rc.id = rp.conta
                 ORDER BY l.lanc, rp.ordem",
            )
            .map_err(falha)?;
        let linhas = stmt
            .query_map(
                params![
                    empresa.em_bytes().as_slice(),
                    i64::from(periodo.de.em_dias()),
                    i64::from(periodo.ate.em_dias())
                ],
                |r| {
                    Ok((
                        r.get::<_, Vec<u8>>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, i64>(2)?,
                    ))
                },
            )
            .map_err(falha)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(falha)?;
        let mut origens = OrigensDasBaixas::new();
        for (lanc, codigo, valor) in linhas {
            origens
                .entry(id_de(&lanc))
                .or_default()
                .push((codigo, Dinheiro::centavos(valor)));
        }
        Ok(origens)
    }
}

#[cfg(test)]
mod testes {
    use cardeal_kernel::Data;
    use cardeal_ledger::ContrapartidaDeCaixa;

    use super::*;

    fn conta(codigo: &str, nome: &str, natureza: Natureza, papel: Option<&str>) -> ContaDoPlano {
        ContaDoPlano {
            id: Id::novo(),
            codigo: codigo.into(),
            nome: nome.into(),
            natureza,
            sintetica: false,
            papel: papel.map(Into::into),
            grupo_fluxo: None,
            ativa: true,
        }
    }

    fn plano() -> Vec<ContaDoPlano> {
        use Natureza::{Ativo, Despesa, Passivo, PatrimonioLiquido, Receita};
        let mut emprestimos = conta("2.1.04", "Empréstimos", Passivo, Some("Emprestimos"));
        emprestimos.grupo_fluxo = Some("Financiamento".into());
        let mut administrativas = conta("5.50", "Despesas administrativas", Despesa, None);
        administrativas.sintetica = true;
        vec![
            conta("1.1.01", "Caixa", Ativo, Some("Caixa")),
            conta("1.1.02", "Bancos", Ativo, Some("Bancos")),
            conta(
                "1.2.01",
                "Clientes a receber",
                Ativo,
                Some("ClientesAReceber"),
            ),
            conta(
                "1.2.02",
                "Cartões a receber",
                Ativo,
                Some("CartoesAReceber"),
            ),
            conta(
                "1.2.05",
                "Impostos a recuperar",
                Ativo,
                Some("ImpostosARecuperar"),
            ),
            conta(
                "1.3.01",
                "Mercadorias para revenda",
                Ativo,
                Some("EstoqueMercadorias"),
            ),
            conta("1.4.01", "Móveis, máquinas e equipamentos", Ativo, None),
            conta("2.1.01", "Fornecedores", Passivo, Some("Fornecedores")),
            emprestimos,
            conta(
                "3.3",
                "Retiradas / pró-labore",
                PatrimonioLiquido,
                Some("Retiradas"),
            ),
            conta(
                "4.1",
                "Receita de vendas de mercadorias",
                Receita,
                Some("ReceitaVendas"),
            ),
            conta(
                "4.4",
                "Descontos concedidos",
                Receita,
                Some("DescontosConcedidos"),
            ),
            conta(
                "4.6",
                "Receitas financeiras",
                Receita,
                Some("ReceitaFinanceira"),
            ),
            conta("5.1", "CMV", Despesa, Some("Cmv")),
            conta(
                "5.8",
                "Impostos sobre venda",
                Despesa,
                Some("ImpostosSobreVenda"),
            ),
            administrativas,
            conta("5.50.01", "Aluguel", Despesa, None),
        ]
    }

    fn papel_de(codigo: &str) -> Option<String> {
        plano()
            .into_iter()
            .find(|c| c.codigo == codigo)
            .and_then(|c| c.papel)
    }

    fn cp(codigo: &str, valor: i64) -> ContrapartidaDeCaixa {
        let c = plano().into_iter().find(|c| c.codigo == codigo).unwrap();
        ContrapartidaDeCaixa {
            codigo: codigo.into(),
            nome: c.nome,
            papel: papel_de(codigo),
            grupo_fluxo: c.grupo_fluxo,
            valor: Dinheiro::reais(valor),
        }
    }

    fn mov(mes: u32, caixa: i64, contrapartidas: Vec<ContrapartidaDeCaixa>) -> MovimentoDeCaixa {
        MovimentoDeCaixa {
            lancamento: Id::novo(),
            data: Data::de_ymd(2026, mes, 10).unwrap(),
            caixa: Dinheiro::reais(caixa),
            contrapartidas,
        }
    }

    fn origem(partidas: &[(&str, i64)]) -> Vec<(String, Dinheiro)> {
        partidas
            .iter()
            .map(|(c, v)| ((*c).to_owned(), Dinheiro::reais(*v)))
            .collect()
    }

    fn linha<'a>(d: &'a DfcGerencial, codigo: &str) -> &'a LinhaGerencial {
        d.grupos
            .iter()
            .flat_map(|g| &g.linhas)
            .find(|l| l.codigo == codigo)
            .unwrap_or_else(|| panic!("a linha {codigo}"))
    }

    fn grupo(d: &DfcGerencial, g: GrupoGerencial) -> &GrupoDfcGerencial {
        d.grupos.iter().find(|x| x.grupo == g).unwrap()
    }

    #[test]
    fn a_baixa_vai_para_a_conta_do_titulo_e_nao_para_clientes_ou_fornecedores() {
        let contas = plano();
        // Venda a prazo de 1.100 com 100 de desconto e CMV de 600: a parcela é de 1.000.
        let venda = origem(&[
            ("1.2.01", 1_000),
            ("4.4", 100),
            ("4.1", -1_100),
            ("5.1", 600),
            ("1.3.01", -600),
        ]);
        // Compra de mercadoria a prazo: 900 de estoque e 100 de imposto a recuperar.
        let compra = origem(&[("1.3.01", 900), ("1.2.05", 100), ("2.1.01", -1_000)]);
        // Aluguel lançado com a categoria ligada à conta.
        let aluguel = origem(&[("5.50.01", 300), ("2.1.01", -300)]);

        let recebe = mov(3, 1_030, vec![cp("1.2.01", -1_000), cp("4.6", -30)]);
        let paga_metade = mov(3, -500, vec![cp("2.1.01", 500)]);
        let paga_aluguel = mov(4, -300, vec![cp("2.1.01", 300)]);
        let mut origens = OrigensDasBaixas::new();
        origens.insert(recebe.lancamento, venda);
        origens.insert(paga_metade.lancamento, compra);
        origens.insert(paga_aluguel.lancamento, aluguel);

        let d = montar(
            2026,
            Dinheiro::reais(200),
            &[recebe, paga_metade, paga_aluguel],
            &[],
            &origens,
            &contas,
            &[],
        );
        // A venda: os 1.000 recebidos são receita de vendas (o desconto já ficou fora: o outro
        // lado da parcela é a receita bruta, na proporção); o estoque do CMV não entra.
        assert_eq!(linha(&d, "4.1").realizado[2], Dinheiro::reais(1_000));
        assert_eq!(linha(&d, "4.6").realizado[2], Dinheiro::reais(30));
        // A compra paga pela metade: metade do estoque, metade do imposto a recuperar.
        assert_eq!(linha(&d, "1.3.01").realizado[2], Dinheiro::reais(-450));
        assert_eq!(
            linha(&d, "1.3.01").nome,
            "Compras: Mercadorias para revenda"
        );
        assert_eq!(linha(&d, "1.2.05").realizado[2], Dinheiro::reais(-50));
        // O aluguel, na agrupadora dele.
        let a = linha(&d, "5.50.01");
        assert_eq!(a.realizado[3], Dinheiro::reais(-300));
        assert_eq!(a.agrupadora.as_deref(), Some("Despesas administrativas"));
        // Nada ficou em clientes ou fornecedores.
        assert!(d
            .grupos
            .iter()
            .flat_map(|g| &g.linhas)
            .all(|l| l.codigo != "1.2.01" && l.codigo != "2.1.01"));
        assert_eq!(d.sem_origem, Dinheiro::ZERO);
        // Os grupos.
        assert_eq!(
            grupo(&d, GrupoGerencial::Receitas).realizado[2],
            Dinheiro::reais(1_000)
        );
        assert_eq!(
            grupo(&d, GrupoGerencial::Custos).realizado[2],
            Dinheiro::reais(-500)
        );
        assert_eq!(
            grupo(&d, GrupoGerencial::Despesas).realizado[3],
            Dinheiro::reais(-300)
        );
        assert_eq!(
            grupo(&d, GrupoGerencial::Financeiro).realizado[2],
            Dinheiro::reais(30)
        );
        // O caixa fecha: a soma das linhas é a variação, mês a mês.
        assert_eq!(d.variacao[2], Dinheiro::reais(530));
        assert_eq!(d.variacao[3], Dinheiro::reais(-300));
        for i in 0..12 {
            let soma: Dinheiro = d.grupos.iter().map(|g| g.realizado[i]).sum();
            assert_eq!(soma, d.variacao[i], "mês {}", i + 1);
        }
        assert_eq!(d.saldo_inicial[0], Dinheiro::reais(200));
        assert_eq!(d.saldo_final[2], Dinheiro::reais(730));
        assert_eq!(d.saldo_inicial[3], Dinheiro::reais(730));
        assert_eq!(d.saldo_final[11], Dinheiro::reais(430));
        assert_eq!(d.geracao_operacional()[2], Dinheiro::reais(500));
    }

    #[test]
    fn o_estorno_da_baixa_desfaz_na_mesma_conta() {
        let contas = plano();
        let venda = origem(&[("1.2.01", 500), ("4.1", -500)]);
        let recebe = mov(5, 500, vec![cp("1.2.01", -500)]);
        let estorno = mov(5, -500, vec![cp("1.2.01", 500)]);
        let mut origens = OrigensDasBaixas::new();
        origens.insert(recebe.lancamento, venda.clone());
        origens.insert(estorno.lancamento, venda);
        let d = montar(
            2026,
            Dinheiro::ZERO,
            &[recebe, estorno],
            &[],
            &origens,
            &contas,
            &[],
        );
        // Recebeu e estornou: nada sobra, nem na receita nem em clientes.
        assert!(d.grupos.is_empty(), "{:?}", d.grupos);
        assert_eq!(d.variacao[4], Dinheiro::ZERO);
    }

    #[test]
    fn sem_titulo_fica_na_passagem_com_um_rotulo_que_diz_o_que_e() {
        let contas = plano();
        // O cartão que a operadora pagou e um recebimento de cliente sem título ligado.
        let cartao = mov(6, 970, vec![cp("1.2.02", -970)]);
        let avulso = mov(6, 100, vec![cp("1.2.01", -100)]);
        // Uma renegociação: clientes a receber dos dois lados do título.
        let renegociado = mov(6, 210, vec![cp("1.2.01", -210)]);
        let mut origens = OrigensDasBaixas::new();
        origens.insert(
            renegociado.lancamento,
            origem(&[("1.2.01", 210), ("1.2.01", -200), ("4.6", -10)]),
        );
        let d = montar(
            2026,
            Dinheiro::ZERO,
            &[cartao, avulso, renegociado],
            &[],
            &origens,
            &contas,
            &[],
        );
        assert_eq!(linha(&d, "1.2.02").nome, "Recebimentos de cartões");
        assert_eq!(linha(&d, "1.2.02").realizado[5], Dinheiro::reais(970));
        let clientes = linha(&d, "1.2.01");
        assert_eq!(
            clientes.nome,
            "Recebimentos de clientes (sem conta de origem)"
        );
        assert_eq!(clientes.realizado[5], Dinheiro::reais(310));
        assert_eq!(d.sem_origem, Dinheiro::reais(310));
        // Tudo nas receitas operacionais.
        assert_eq!(
            grupo(&d, GrupoGerencial::Receitas).realizado[5],
            Dinheiro::reais(1_280)
        );
    }

    #[test]
    fn a_venda_a_vista_com_cmv_no_mesmo_lancamento_nao_vira_compra() {
        let contas = plano();
        // D Caixa 100 / C Receita 100 / D CMV 60 / C Estoque 60.
        let venda = mov(
            7,
            100,
            vec![cp("4.1", -100), cp("5.1", 60), cp("1.3.01", -60)],
        );
        // E uma compra à vista de verdade: D Estoque 400 / C Caixa 400.
        let compra = mov(7, -400, vec![cp("1.3.01", 400)]);
        let d = montar(
            2026,
            Dinheiro::ZERO,
            &[venda, compra],
            &[],
            &OrigensDasBaixas::new(),
            &contas,
            &[],
        );
        assert_eq!(linha(&d, "4.1").realizado[6], Dinheiro::reais(100));
        assert_eq!(linha(&d, "1.3.01").realizado[6], Dinheiro::reais(-400));
        assert!(d
            .grupos
            .iter()
            .flat_map(|g| &g.linhas)
            .all(|l| l.codigo != "5.1"));
        assert_eq!(d.variacao[6], Dinheiro::reais(-300));
    }

    #[test]
    fn investimento_financiamento_e_o_orcado_no_sinal_do_caixa() {
        let contas = plano();
        let id = |codigo: &str| contas.iter().find(|c| c.codigo == codigo).unwrap().id;
        let maquina = mov(2, -5_000, vec![cp("1.4.01", 5_000)]);
        let emprestimo = mov(2, 20_000, vec![cp("2.1.04", -20_000)]);
        let retirada = mov(2, -3_000, vec![cp("3.3", 3_000)]);
        let mut aluguel = [Dinheiro::ZERO; 12];
        aluguel[1] = Dinheiro::reais(300);
        let mut vendas = [Dinheiro::ZERO; 12];
        vendas[1] = Dinheiro::reais(10_000);
        let mut descontos = [Dinheiro::ZERO; 12];
        descontos[1] = Dinheiro::reais(200);
        let mut retiradas = [Dinheiro::ZERO; 12];
        retiradas[1] = Dinheiro::reais(2_000);
        let d = montar(
            2026,
            Dinheiro::ZERO,
            &[maquina, emprestimo, retirada],
            &[],
            &OrigensDasBaixas::new(),
            &contas,
            &[
                (id("5.50.01"), aluguel),
                (id("4.1"), vendas),
                (id("4.4"), descontos),
                (id("3.3"), retiradas),
            ],
        );
        assert_eq!(
            grupo(&d, GrupoGerencial::Investimentos).realizado[1],
            Dinheiro::reais(-5_000)
        );
        let fin = grupo(&d, GrupoGerencial::Financiamentos);
        assert_eq!(fin.realizado[1], Dinheiro::reais(17_000));
        // O orçado entra no sentido do caixa: a venda entra, o desconto e o aluguel saem, a
        // retirada sai.
        assert_eq!(linha(&d, "4.1").orcado[1], Dinheiro::reais(10_000));
        assert_eq!(linha(&d, "4.4").orcado[1], Dinheiro::reais(-200));
        assert_eq!(linha(&d, "5.50.01").orcado[1], Dinheiro::reais(-300));
        assert_eq!(linha(&d, "3.3").orcado[1], Dinheiro::reais(-2_000));
        assert_eq!(
            grupo(&d, GrupoGerencial::Deducoes).orcado[1],
            Dinheiro::reais(-200)
        );
        // A conta orçada sem movimento aparece, zerada no realizado.
        assert_eq!(linha(&d, "4.1").realizado[1], Dinheiro::ZERO);
        assert_eq!(d.variacao_orcada()[1], Dinheiro::reais(7_500));
        assert_eq!(d.geracao_operacional_orcada()[1], Dinheiro::reais(9_500));
        assert!(d.tem_orcamento());
        assert_eq!(
            atingido(Dinheiro::reais(-270), Dinheiro::reais(-300)),
            Some(Percentual::pontos(90))
        );
        assert_eq!(
            percentual(Dinheiro::reais(-300), Dinheiro::reais(1_000)),
            Some(Percentual::pontos(-30))
        );
    }

    #[test]
    fn o_a_vencer_vai_para_a_conta_do_titulo_sem_mexer_no_realizado() {
        let contas = plano();
        // Uma venda a prazo em aberto (vence em dezembro) e o aluguel a pagar.
        let venda = origem(&[("1.2.01", 1_000), ("4.1", -1_000)]);
        let aluguel = origem(&[("5.50.01", 300), ("2.1.01", -300)]);
        let receber = mov(12, 1_000, vec![cp("1.2.01", -1_000)]);
        let pagar = mov(12, -300, vec![cp("2.1.01", 300)]);
        let mut origens = OrigensDasBaixas::new();
        origens.insert(receber.lancamento, venda);
        origens.insert(pagar.lancamento, aluguel);
        let d = montar(
            2026,
            Dinheiro::reais(50),
            &[],
            &[receber, pagar],
            &origens,
            &contas,
            &[],
        );
        assert_eq!(linha(&d, "4.1").a_vencer[11], Dinheiro::reais(1_000));
        assert_eq!(linha(&d, "5.50.01").a_vencer[11], Dinheiro::reais(-300));
        assert!(linha(&d, "4.1").realizado.iter().all(|v| v.e_zero()));
        assert!(d.variacao.iter().all(|v| v.e_zero()));
        assert_eq!(d.variacao_a_vencer()[11], Dinheiro::reais(700));
        assert!(d.tem_a_vencer());
        assert_eq!(d.saldo_final[11], Dinheiro::reais(50));
    }

    #[test]
    fn orcaveis_pelo_caixa_sao_compras_investimentos_e_financiamentos() {
        let mut contas = plano();
        contas.push(conta(
            "1.4.02",
            "Depreciação acumulada",
            Natureza::Ativo,
            None,
        ));
        let mut capital = conta(
            "3.1",
            "Capital",
            Natureza::PatrimonioLiquido,
            Some("Capital"),
        );
        capital.grupo_fluxo = Some("Financiamento".into());
        contas.push(capital);
        let o: Vec<(String, &str, bool)> = contas_orcaveis_pelo_caixa(&contas)
            .into_iter()
            .map(|c| (c.codigo, c.bloco, c.entra))
            .collect();
        assert_eq!(
            o,
            [
                ("1.3.01".to_owned(), "Compras", false),
                ("1.4.01".to_owned(), "Investimentos", false),
                ("2.1.04".to_owned(), "Financiamentos", true),
                ("3.1".to_owned(), "Financiamentos", true),
                ("3.3".to_owned(), "Financiamentos", false),
            ]
        );
    }

    #[test]
    fn repartir_nao_perde_centavo() {
        let partes = repartir(
            Dinheiro::centavos(-1_000),
            &[
                Dinheiro::centavos(-333),
                Dinheiro::centavos(-333),
                Dinheiro::centavos(-334),
            ],
        );
        assert_eq!(
            partes.iter().copied().sum::<Dinheiro>(),
            Dinheiro::centavos(-1_000)
        );
        assert_eq!(partes[2], Dinheiro::centavos(-334));
        assert_eq!(mes_do_ano(Competencia::nova(2026, 4), 2026), Some(4));
        assert_eq!(soma_ate(&[Dinheiro::reais(1); 12], 3), Dinheiro::reais(3));
    }
}
