//! O orçamento por conta do resultado: quanto se espera de cada receita e de cada despesa, mês a
//! mês, e o orçado × realizado — no ano (a aba Orçamento) e na DRE do mês (a coluna "Orçado").
//!
//! Lição do `gestao-raiz` (`financial_budgets`, a aba "Orçamento"): orçamento anual por conta do
//! plano com os 12 meses, a DRE com orçado, realizado e variação, e "copiar do ano anterior".
//! Lá a variação é `realizado ÷ orçado − 1` em toda linha — numa despesa, gastar mais que o
//! orçado aparece **positivo**, como se fosse bom. Aqui a variação segue o sinal da DRE (positivo
//! é sempre melhor), a cópia do ano anterior aceita um reajuste e só preenche as contas que ainda
//! não têm orçamento no ano (não apaga o que já foi feito).

use cardeal_kernel::{Arredondamento, Competencia, Dinheiro, Id, Percentual};
#[cfg(feature = "sqlite")]
use cardeal_kernel::{CodigoErro, Erro, Resultado};
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::{Comando, Consulta, Risco};
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// Uma conta do resultado no orçamento do ano.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinhaDeOrcamento {
    /// A conta.
    pub conta: Id,
    /// O código.
    pub codigo: String,
    /// O nome.
    pub nome: String,
    /// O bloco da DRE em que ela entra ("Receita bruta", "(−) Despesas operacionais"...).
    pub bloco: String,
    /// Receita (soma no resultado) ou não (subtrai). Na conta orçada pelo caixa: o orçado
    /// positivo entra no caixa (empréstimo tomado, aporte) ou não (compra, investimento,
    /// retirada) — o desvio segue o mesmo sentido.
    pub soma_no_resultado: bool,
    /// Conta patrimonial orçada **pelo caixa** (compras de mercadoria, investimentos,
    /// financiamentos): o realizado é o que entrou ou saiu do caixa (`crate::dfc_gerencial`), não
    /// a competência, e o orçado pode ser negativo (a parcela paga de um empréstimo). Fica fora do
    /// resultado orçado e da DRE.
    pub de_caixa: bool,
    /// O orçado de cada mês (jan a dez), em valor positivo.
    pub orcado: [Dinheiro; 12],
    /// O realizado de cada mês, no mesmo sentido do orçado.
    pub realizado: [Dinheiro; 12],
}

impl LinhaDeOrcamento {
    /// O orçado no ano.
    #[must_use]
    pub fn orcado_no_ano(&self) -> Dinheiro {
        self.orcado.iter().copied().sum()
    }

    /// O orçado até o mês `ate` (1 a 12).
    #[must_use]
    pub fn orcado_ate(&self, ate: u32) -> Dinheiro {
        self.orcado.iter().take(ate as usize).copied().sum()
    }

    /// O realizado até o mês `ate` (1 a 12).
    #[must_use]
    pub fn realizado_ate(&self, ate: u32) -> Dinheiro {
        self.realizado.iter().take(ate as usize).copied().sum()
    }

    /// Se há orçamento no ano.
    #[must_use]
    pub fn tem_orcamento(&self) -> bool {
        self.orcado.iter().any(|v| !v.e_zero())
    }
}

/// A diferença entre o realizado e o orçado, no sentido do resultado: positiva é sempre melhor
/// (receita acima do orçado, despesa abaixo). Em % do orçado; `None` sem orçado.
#[must_use]
pub fn desvio(
    realizado: Dinheiro,
    orcado: Dinheiro,
    soma_no_resultado: bool,
) -> Option<Percentual> {
    if orcado.e_zero() {
        return None;
    }
    let diferenca = if soma_no_resultado {
        realizado - orcado
    } else {
        orcado - realizado
    };
    Some(Percentual::da_razao(
        diferenca.em_centavos(),
        orcado.em_centavos().abs(),
    ))
}

/// O orçamento de um ano: todas as contas do resultado e as contas patrimoniais que se orçam pelo
/// caixa (as sem orçamento também, para orçar).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct OrcamentoAnual {
    /// O ano.
    pub ano: i32,
    /// As contas, pela ordem do plano.
    pub linhas: Vec<LinhaDeOrcamento>,
}

impl OrcamentoAnual {
    /// O resultado orçado no ano (receitas − custos e despesas; as contas de caixa ficam fora).
    #[must_use]
    pub fn resultado_orcado(&self) -> Dinheiro {
        self.linhas
            .iter()
            .filter(|l| !l.de_caixa)
            .map(|l| {
                let v = l.orcado_no_ano();
                if l.soma_no_resultado {
                    v
                } else {
                    -v
                }
            })
            .sum()
    }
}

/// O orçamento do ano, com o realizado de cada mês.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct OrcamentoDoAno {
    /// O ano.
    pub ano: i32,
}

impl Consulta for OrcamentoDoAno {
    type Saida = OrcamentoAnual;
    const PERMISSAO: &'static str = "financeiro.dre.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<OrcamentoAnual> {
        sqlite::orcamento_anual(c, ctx.empresa, self.ano)
    }
}

/// Grava o orçamento de uma conta num ano (os 12 meses; zero = sem orçamento no mês).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefinirOrcamentoDaConta {
    /// A conta do resultado.
    pub conta: Id,
    /// O ano.
    pub ano: i32,
    /// Jan a dez.
    pub meses: [Dinheiro; 12],
}

impl Comando for DefinirOrcamentoDaConta {
    type Saida = ();
    const PERMISSAO: &'static str = "financeiro.orcamento.editar";
    const RISCO: Risco = Risco::Medio;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<()> {
        let orcaveis = sqlite::Orcaveis::ler(uow.conexao(), ctx.empresa)?;
        orcaveis.conferir(self.conta, &self.meses)?;
        sqlite::gravar(
            uow.conexao(),
            ctx.empresa,
            self.conta,
            self.ano,
            &self.meses,
        )
    }
}

/// Quantas contas a planilha grava de uma vez, no máximo.
pub const MAXIMO_DE_CONTAS_NA_PLANILHA: usize = 500;

/// Grava o orçamento de várias contas num ano de uma vez — a planilha do ano, "Salvar orçamento".
/// Tudo ou nada: uma conta que não se orça (ou um valor negativo onde não cabe) recusa o lote
/// inteiro. Devolve quantas contas foram gravadas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefinirOrcamentoDoAno {
    /// O ano.
    pub ano: i32,
    /// Cada conta com os 12 meses (zero = sem orçamento no mês).
    pub contas: Vec<(Id, [Dinheiro; 12])>,
}

impl Comando for DefinirOrcamentoDoAno {
    type Saida = u32;
    const PERMISSAO: &'static str = "financeiro.orcamento.editar";
    const RISCO: Risco = Risco::Medio;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<u32> {
        if self.contas.len() > MAXIMO_DE_CONTAS_NA_PLANILHA {
            return Err(Erro::novo(
                CodigoErro::FORA_DA_FAIXA,
                format!("no máximo {MAXIMO_DE_CONTAS_NA_PLANILHA} contas de uma vez"),
            ));
        }
        let orcaveis = sqlite::Orcaveis::ler(uow.conexao(), ctx.empresa)?;
        for (i, (conta, meses)) in self.contas.iter().enumerate() {
            if self.contas[..i].iter().any(|(c, _)| c == conta) {
                return Err(Erro::novo(
                    CodigoErro::FORA_DA_FAIXA,
                    "a mesma conta duas vezes no orçamento",
                ));
            }
            orcaveis.conferir(*conta, meses)?;
        }
        for (conta, meses) in &self.contas {
            sqlite::gravar(uow.conexao(), ctx.empresa, *conta, self.ano, meses)?;
        }
        Ok(u32::try_from(self.contas.len()).unwrap_or(u32::MAX))
    }
}

/// Copia o orçamento de um ano para outro, com reajuste — só para as contas que ainda não têm
/// orçamento no ano de destino. Devolve quantas contas foram copiadas.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CopiarOrcamentoDoAno {
    /// O ano de onde copiar.
    pub de: i32,
    /// O ano para onde copiar.
    pub para: i32,
    /// O reajuste (ex.: 5% = 5 pontos; zero copia igual).
    pub reajuste: Percentual,
}

impl Comando for CopiarOrcamentoDoAno {
    type Saida = u32;
    const PERMISSAO: &'static str = "financeiro.orcamento.editar";
    const RISCO: Risco = Risco::Medio;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<u32> {
        if self.de == self.para {
            return Err(Erro::novo(
                CodigoErro::FORA_DA_FAIXA,
                "escolha outro ano para copiar",
            ));
        }
        let origem = sqlite::orcado_por_conta(uow.conexao(), ctx.empresa, self.de)?;
        let destino = sqlite::orcado_por_conta(uow.conexao(), ctx.empresa, self.para)?;
        let mut copiadas = 0;
        for (conta, meses) in origem {
            if destino.iter().any(|(c, _)| *c == conta) {
                continue;
            }
            let reajustados = meses.map(|v| reajustar(v, self.reajuste));
            sqlite::gravar(uow.conexao(), ctx.empresa, conta, self.para, &reajustados)?;
            copiadas += 1;
        }
        Ok(copiadas)
    }
}

/// O valor com o reajuste (arredondado ao centavo).
#[must_use]
pub fn reajustar(v: Dinheiro, reajuste: Percentual) -> Dinheiro {
    v + v.aplicar(reajuste, Arredondamento::MeioAcima)
}

/// O mês (1 a 12) de uma competência no ano, ou `None` fora dele.
#[must_use]
pub fn mes_no_ano(c: Competencia, ano: i32) -> Option<usize> {
    (c.ano() == ano).then(|| c.mes() as usize - 1)
}

#[cfg(feature = "sqlite")]
pub(crate) mod sqlite {
    use cardeal_kernel::{CodigoErro, Competencia, Dinheiro, Erro, Id, Periodo, Resultado};
    use rusqlite::{params, Connection};

    use super::{LinhaDeOrcamento, OrcamentoAnual};

    #[allow(clippy::needless_pass_by_value)]
    fn falha(e: rusqlite::Error) -> Erro {
        Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string())
    }

    #[allow(clippy::needless_pass_by_value)]
    fn ledger(e: cardeal_ledger::ErroRazao) -> Erro {
        Erro::novo(
            CodigoErro::FALHA_INTERNA,
            format!("financeiro/orçamento: {e}"),
        )
    }

    /// As contas que se orçam: as do resultado e as de caixa (compras, investimentos,
    /// financiamentos).
    pub(crate) struct Orcaveis {
        resultado: Vec<Id>,
        de_caixa: Vec<Id>,
    }

    impl Orcaveis {
        pub(crate) fn ler(c: &Connection, empresa: Id) -> Resultado<Self> {
            let resultado = cardeal_ledger::contas_de_resultado(c, empresa)
                .map_err(ledger)?
                .into_iter()
                .map(|l| l.conta)
                .collect();
            let plano = crate::dfc_gerencial::sqlite::plano(c, empresa)?;
            let de_caixa = crate::dfc_gerencial::contas_orcaveis_pelo_caixa(&plano)
                .into_iter()
                .map(|c| c.conta)
                .collect();
            Ok(Self {
                resultado,
                de_caixa,
            })
        }

        /// A conta se orça e os valores cabem nela: negativo só na conta de caixa (a parcela
        /// paga do empréstimo, a retirada devolvida).
        pub(crate) fn conferir(&self, conta: Id, meses: &[Dinheiro; 12]) -> Resultado<()> {
            if self.de_caixa.contains(&conta) {
                return Ok(());
            }
            if !self.resultado.contains(&conta) {
                return Err(Erro::novo(
                    CodigoErro::FORA_DA_FAIXA,
                    "orçamento só em conta de receita, despesa, compra, investimento ou \
                     financiamento (analítica e ativa)",
                ));
            }
            if meses.iter().any(|v| v.e_negativo()) {
                return Err(Erro::novo(
                    CodigoErro::FORA_DA_FAIXA,
                    "o orçado de receita ou despesa não pode ser negativo",
                ));
            }
            Ok(())
        }
    }

    pub(crate) fn gravar(
        c: &Connection,
        empresa: Id,
        conta: Id,
        ano: i32,
        meses: &[Dinheiro; 12],
    ) -> Resultado<()> {
        let (e, k) = (empresa.em_bytes(), conta.em_bytes());
        c.execute(
            "DELETE FROM financeiro_orcamento
             WHERE empresa = ?1 AND conta = ?2 AND competencia BETWEEN ?3 AND ?4",
            params![e.as_slice(), k.as_slice(), ano * 100 + 1, ano * 100 + 12],
        )
        .map_err(falha)?;
        for (i, v) in meses.iter().enumerate() {
            if v.e_zero() {
                continue;
            }
            let mes = u32::try_from(i).unwrap_or(0) + 1;
            c.execute(
                "INSERT INTO financeiro_orcamento (empresa, conta, competencia, valor)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    e.as_slice(),
                    k.as_slice(),
                    Competencia::nova(ano, mes).em_aaaamm(),
                    v.em_centavos()
                ],
            )
            .map_err(falha)?;
        }
        Ok(())
    }

    /// O orçado de cada conta num ano: (conta, os 12 meses).
    pub(crate) fn orcado_por_conta(
        c: &Connection,
        empresa: Id,
        ano: i32,
    ) -> Resultado<Vec<(Id, [Dinheiro; 12])>> {
        let mut stmt = c
            .prepare(
                "SELECT conta, competencia, valor FROM financeiro_orcamento
                 WHERE empresa = ?1 AND competencia BETWEEN ?2 AND ?3
                 ORDER BY conta, competencia",
            )
            .map_err(falha)?;
        let linhas = stmt
            .query_map(
                params![empresa.em_bytes().as_slice(), ano * 100 + 1, ano * 100 + 12],
                |r| {
                    Ok((
                        r.get::<_, Vec<u8>>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                    ))
                },
            )
            .map_err(falha)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(falha)?;
        let mut saida: Vec<(Id, [Dinheiro; 12])> = Vec::new();
        for (conta, comp, valor) in linhas {
            let conta = <[u8; 16]>::try_from(conta).map_or(Id::NULO, Id::de_bytes);
            let mes = usize::try_from(comp % 100).unwrap_or(1).clamp(1, 12) - 1;
            if let Some((_, meses)) = saida.iter_mut().find(|(c, _)| *c == conta) {
                meses[mes] = Dinheiro::centavos(valor);
            } else {
                let mut meses = [Dinheiro::ZERO; 12];
                meses[mes] = Dinheiro::centavos(valor);
                saida.push((conta, meses));
            }
        }
        Ok(saida)
    }

    /// O orçado de cada conta num mês (para a DRE).
    pub(crate) fn orcado_do_mes(
        c: &Connection,
        empresa: Id,
        mes: Competencia,
    ) -> Resultado<Vec<(Id, Dinheiro)>> {
        let mut stmt = c
            .prepare(
                "SELECT conta, valor FROM financeiro_orcamento
                 WHERE empresa = ?1 AND competencia = ?2",
            )
            .map_err(falha)?;
        let v = stmt
            .query_map(
                params![empresa.em_bytes().as_slice(), mes.em_aaaamm()],
                |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, i64>(1)?)),
            )
            .map_err(falha)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(falha)?
            .into_iter()
            .map(|(k, v)| {
                (
                    <[u8; 16]>::try_from(k).map_or(Id::NULO, Id::de_bytes),
                    Dinheiro::centavos(v),
                )
            })
            .collect();
        Ok(v)
    }

    pub(crate) fn orcamento_anual(
        c: &Connection,
        empresa: Id,
        ano: i32,
    ) -> Resultado<OrcamentoAnual> {
        let contas = cardeal_ledger::contas_de_resultado(c, empresa).map_err(ledger)?;
        let orcado = orcado_por_conta(c, empresa, ano)?;
        let mut linhas: Vec<LinhaDeOrcamento> = contas
            .iter()
            .map(|l| LinhaDeOrcamento {
                conta: l.conta,
                codigo: l.codigo.clone(),
                nome: l.nome.clone(),
                bloco: crate::dre::rotulo_do_bloco(l).to_owned(),
                soma_no_resultado: crate::dre::soma_no_resultado(l),
                de_caixa: false,
                orcado: orcado
                    .iter()
                    .find(|(c, _)| *c == l.conta)
                    .map_or([Dinheiro::ZERO; 12], |(_, m)| *m),
                realizado: [Dinheiro::ZERO; 12],
            })
            .collect();
        for mes in 1..=12 {
            let comp = Competencia::nova(ano, mes);
            let periodo = Periodo::novo(comp.primeiro_dia(), comp.ultimo_dia());
            for r in cardeal_ledger::resultado_por_conta(c, empresa, periodo).map_err(ledger)? {
                if let Some(l) = linhas.iter_mut().find(|l| l.conta == r.conta) {
                    l.realizado[mes as usize - 1] = crate::dre::no_sentido_do_orcado(&r);
                }
            }
        }
        // As contas de caixa, com o realizado pelo caixa no sentido em que se orçam.
        let plano = crate::dfc_gerencial::sqlite::plano(c, empresa)?;
        let caixa = crate::dfc_gerencial::caixa_por_conta(c, empresa, ano)?;
        for k in crate::dfc_gerencial::contas_orcaveis_pelo_caixa(&plano) {
            let realizado = caixa.get(&k.conta).map_or([Dinheiro::ZERO; 12], |m| {
                m.map(|v| if k.entra { v } else { -v })
            });
            linhas.push(LinhaDeOrcamento {
                conta: k.conta,
                codigo: k.codigo,
                nome: k.nome,
                bloco: k.bloco.to_owned(),
                soma_no_resultado: k.entra,
                de_caixa: true,
                orcado: orcado
                    .iter()
                    .find(|(c, _)| *c == k.conta)
                    .map_or([Dinheiro::ZERO; 12], |(_, m)| *m),
                realizado,
            });
        }
        Ok(OrcamentoAnual { ano, linhas })
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_desvio_e_positivo_quando_e_melhor() {
        // Receita: vender mais que o orçado é bom.
        assert_eq!(
            desvio(Dinheiro::reais(110), Dinheiro::reais(100), true),
            Some(Percentual::pontos(10))
        );
        // Despesa: gastar mais que o orçado é ruim (o gestao-raiz mostraria +10%).
        assert_eq!(
            desvio(Dinheiro::reais(110), Dinheiro::reais(100), false),
            Some(Percentual::pontos(-10))
        );
        assert_eq!(desvio(Dinheiro::reais(5), Dinheiro::ZERO, false), None);
    }

    #[test]
    fn o_reajuste_e_sobre_cada_mes() {
        assert_eq!(
            reajustar(Dinheiro::reais(1_000), Percentual::pontos(5)),
            Dinheiro::reais(1_050)
        );
        assert_eq!(
            reajustar(Dinheiro::reais(1_000), Percentual::ZERO),
            Dinheiro::reais(1_000)
        );
        assert_eq!(mes_no_ano(Competencia::nova(2026, 3), 2026), Some(2));
        assert_eq!(mes_no_ano(Competencia::nova(2025, 3), 2026), None);
    }
}
