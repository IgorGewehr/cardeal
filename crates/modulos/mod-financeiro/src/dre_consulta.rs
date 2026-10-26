//! As consultas da DRE gerencial — `docs/modulos/financeiro.md` §6 (`DreDoMes`).

#[cfg(feature = "sqlite")]
use cardeal_kernel::{CodigoErro, Erro, Resultado};
use cardeal_kernel::{Competencia, Periodo};
use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::dre::Dre;

#[cfg(feature = "sqlite")]
fn apurar(conexao: &Connection, ctx: &Ctx, periodo: Periodo) -> Resultado<Dre> {
    let linhas = cardeal_ledger::resultado_por_conta(conexao, ctx.empresa, periodo)
        .map_err(|e| Erro::novo(CodigoErro::FALHA_INTERNA, format!("financeiro/DRE: {e}")))?;
    Ok(crate::dre::montar(periodo, &linhas))
}

/// A DRE de um período qualquer (competência de cada lançamento dentro dele).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DreDoPeriodo {
    /// O período, inclusive nas pontas.
    pub periodo: Periodo,
}

impl Consulta for DreDoPeriodo {
    type Saida = Dre;
    const PERMISSAO: &'static str = "financeiro.dre.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        apurar(conexao, ctx, self.periodo)
    }
}

/// O mês pedido ao lado do anterior e do mesmo mês do ano anterior — o comparativo da tela.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComparativoDre {
    /// O mês pedido.
    pub atual: Dre,
    /// O mês imediatamente anterior.
    pub mes_anterior: Dre,
    /// O mesmo mês, um ano antes.
    pub mesmo_mes_ano_anterior: Dre,
    /// A DRE do orçamento do mês (`None` sem orçamento): a coluna "Orçado".
    pub orcado: Option<Dre>,
}

/// A DRE do mês, comparada com o mês anterior e com o mesmo mês do ano anterior.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DreComparativa {
    /// O mês de referência.
    pub mes: Competencia,
}

#[cfg(feature = "sqlite")]
fn periodo_do_mes(c: Competencia) -> Periodo {
    Periodo::novo(c.primeiro_dia(), c.ultimo_dia())
}

impl Consulta for DreComparativa {
    type Saida = ComparativoDre;
    const PERMISSAO: &'static str = "financeiro.dre.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let ano_antes = Competencia::nova(self.mes.ano() - 1, self.mes.mes());
        let periodo = periodo_do_mes(self.mes);
        // O orçamento do mês: a DRE do orçado, e as contas orçadas aparecem no realizado mesmo
        // sem movimento.
        let orcado = crate::orcamento::sqlite::orcado_do_mes(conexao, ctx.empresa, self.mes)?;
        let (atual, dre_orcada) = if orcado.is_empty() {
            (apurar(conexao, ctx, periodo)?, None)
        } else {
            let contas =
                cardeal_ledger::contas_de_resultado(conexao, ctx.empresa).map_err(|e| {
                    Erro::novo(CodigoErro::FALHA_INTERNA, format!("financeiro/DRE: {e}"))
                })?;
            let linhas_orcadas: Vec<cardeal_ledger::LinhaResultado> = orcado
                .iter()
                .filter_map(|(conta, valor)| {
                    contas
                        .iter()
                        .find(|c| c.conta == *conta)
                        .map(|c| crate::dre::linha_orcada(c, *valor))
                })
                .collect();
            let sempre: Vec<cardeal_kernel::Id> = orcado.iter().map(|(c, _)| *c).collect();
            let mut realizado = cardeal_ledger::resultado_por_conta(conexao, ctx.empresa, periodo)
                .map_err(|e| {
                    Erro::novo(CodigoErro::FALHA_INTERNA, format!("financeiro/DRE: {e}"))
                })?;
            for l in &linhas_orcadas {
                if !realizado.iter().any(|r| r.conta == l.conta) {
                    let mut zerada = l.clone();
                    zerada.valor = cardeal_kernel::Dinheiro::ZERO;
                    realizado.push(zerada);
                }
            }
            (
                crate::dre::montar_mantendo(periodo, &realizado, &sempre),
                Some(crate::dre::montar_mantendo(
                    periodo,
                    &linhas_orcadas,
                    &sempre,
                )),
            )
        };
        Ok(ComparativoDre {
            atual,
            mes_anterior: apurar(conexao, ctx, periodo_do_mes(self.mes.anterior()))?,
            mesmo_mes_ano_anterior: apurar(conexao, ctx, periodo_do_mes(ano_antes))?,
            orcado: dre_orcada,
        })
    }
}

/// O DFC (fluxo de caixa realizado, método direto) de um período.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DfcDoPeriodo {
    /// O período, inclusive nas pontas.
    pub periodo: Periodo,
}

impl Consulta for DfcDoPeriodo {
    type Saida = crate::dfc::Dfc;
    const PERMISSAO: &'static str = "financeiro.dre.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let (abertura, movimentos) =
            cardeal_ledger::movimentos_de_caixa(conexao, ctx.empresa, self.periodo).map_err(
                |e| Erro::novo(CodigoErro::FALHA_INTERNA, format!("financeiro/DFC: {e}")),
            )?;
        Ok(crate::dfc::montar(self.periodo, abertura, &movimentos))
    }
}

/// O DFC pelo método indireto de um período: do lucro líquido da DRE à variação do caixa.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DfcIndiretoDoPeriodo {
    /// O período, inclusive nas pontas.
    pub periodo: Periodo,
}

impl Consulta for DfcIndiretoDoPeriodo {
    type Saida = crate::dfc::DfcIndireto;
    const PERMISSAO: &'static str = "financeiro.dre.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let erro = |e: cardeal_ledger::ErroRazao| {
            Erro::novo(CodigoErro::FALHA_INTERNA, format!("financeiro/DFC: {e}"))
        };
        let (abertura, movimentos) =
            cardeal_ledger::movimentos_de_caixa(conexao, ctx.empresa, self.periodo)
                .map_err(erro)?;
        let direto = crate::dfc::montar(self.periodo, abertura, &movimentos);
        let lucro = apurar(conexao, ctx, self.periodo)?.lucro_liquido;
        let variacoes = cardeal_ledger::variacoes_patrimoniais(conexao, ctx.empresa, self.periodo)
            .map_err(erro)?;
        Ok(crate::dfc::montar_indireto(&direto, lucro, &variacoes))
    }
}

/// O balancete de verificação do período (`cardeal_ledger::balancete`): saldo anterior,
/// débitos, créditos e saldo atual de cada conta, com as agrupadoras somadas.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BalanceteDoPeriodo {
    /// O período, inclusive nas pontas.
    pub periodo: Periodo,
}

impl Consulta for BalanceteDoPeriodo {
    type Saida = Vec<cardeal_ledger::LinhaBalancete>;
    const PERMISSAO: &'static str = "financeiro.dre.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        cardeal_ledger::balancete(conexao, ctx.empresa, self.periodo).map_err(|e| {
            Erro::novo(
                CodigoErro::FALHA_INTERNA,
                format!("financeiro/balancete: {e}"),
            )
        })
    }
}

/// O diário do período: cada lançamento com as partidas (`cardeal_ledger::diario`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DiarioDoPeriodo {
    /// O período, inclusive nas pontas.
    pub periodo: Periodo,
}

impl Consulta for DiarioDoPeriodo {
    type Saida = Vec<cardeal_ledger::LancamentoDoDiario>;
    const PERMISSAO: &'static str = "financeiro.dre.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        cardeal_ledger::diario(conexao, ctx.empresa, self.periodo)
            .map_err(|e| Erro::novo(CodigoErro::FALHA_INTERNA, format!("financeiro/diário: {e}")))
    }
}

/// O score de saúde financeira (ver [`crate::saude`]). O giro de estoque pertence a outro módulo:
/// quem o conhece (a tela, via `estoque.analise_de_estoque.v1`) o informa; sem ele o sinal sai
/// do cálculo.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SaudeFinanceira {
    /// Giro mensal do estoque (100% = o estoque inteiro girou uma vez por mês).
    pub giro_mensal: Option<cardeal_kernel::Percentual>,
}

impl Consulta for SaudeFinanceira {
    type Saida = crate::saude::Saude;
    const PERMISSAO: &'static str = "financeiro.dre.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        use crate::saude::{calcular, EntradasDeSaude};
        use crate::titulo::EspecieTitulo;

        let erro = |e: cardeal_ledger::ErroRazao| {
            Erro::novo(CodigoErro::FALHA_INTERNA, format!("financeiro/saúde: {e}"))
        };
        let hoje = ctx.hoje();
        // Caixa de hoje = o saldo de abertura de amanhã.
        let amanha = hoje.mais_dias(1);
        let (caixa, _) = cardeal_ledger::movimentos_de_caixa(
            conexao,
            ctx.empresa,
            Periodo::novo(amanha, amanha),
        )
        .map_err(erro)?;

        let receber =
            crate::consultas::titulos_em_aberto(conexao, ctx.empresa, EspecieTitulo::Receber)?;
        let pagar =
            crate::consultas::titulos_em_aberto(conexao, ctx.empresa, EspecieTitulo::Pagar)?;
        let ate = hoje.mais_dias(30);
        let soma = |v: &[crate::consultas::ItemTituloEmAberto]| {
            v.iter()
                .filter(|p| p.vencimento <= ate)
                .fold(cardeal_kernel::Dinheiro::ZERO, |a, p| a + p.saldo())
        };
        let aging = crate::aging::montar(&receber, hoje);

        let linhas = cardeal_ledger::resultado_por_conta(
            conexao,
            ctx.empresa,
            Periodo::novo(hoje.mais_dias(-90), hoje),
        )
        .map_err(erro)?;
        let dre = crate::dre::montar(Periodo::novo(hoje.mais_dias(-90), hoje), &linhas);
        let margem_ebitda = (!dre.receita_liquida.e_zero()).then(|| {
            cardeal_kernel::Percentual::da_razao(
                dre.ebitda.em_centavos(),
                dre.receita_liquida.em_centavos(),
            )
        });

        Ok(calcular(&EntradasDeSaude {
            caixa,
            a_receber_30d: soma(&receber),
            a_pagar_30d: soma(&pagar),
            inadimplencia: aging.taxa_de_inadimplencia(),
            margem_ebitda,
            giro_mensal: self.giro_mensal,
        }))
    }
}
