//! O resumo do Simples Nacional do mês — o RBT12 pela regra da lei, a alíquota efetiva, o DAS
//! estimado e os avisos de limite — e o RBT12 que o preço sugerido usa (os dois saem do mesmo
//! lugar, para o imposto do preço e o do resumo não discordarem).
//!
//! Lição do `gestao-raiz` (`api/industrial/financial/tax-summary`): lá o imposto é a alíquota
//! **nominal** da faixa sobre o acumulado **do ano**, pelo que foi **recebido**, e o MEI paga 5%.
//! Aqui:
//! - o RBT12 é a receita bruta dos **12 meses anteriores** ao mês (LC 123/2006, art. 18 §1º),
//!   pela competência (o Razão), não pelo caixa;
//! - a alíquota é a **efetiva**: `(RBT12 × nominal − parcela a deduzir) ÷ RBT12`
//!   ([`aliquota_efetiva`]);
//! - no início de atividade a receita é **anualizada** (art. 18 §§2º e 3º): no 1º mês, a do
//!   próprio mês × 12; nos 11 seguintes, a média dos meses anteriores × 12;
//! - a receita de **antes do Cardeal** (o Razão começa na migração) pode ser informada — sem ela
//!   e sem o início de atividade, o RBT12 sai menor que o real, e o resumo avisa;
//! - o MEI paga o DAS fixo: o que importa é o limite anual (R$ 81 mil).
//!
//! O DAS estimado não separa as receitas com ST ou monofásico (que pagam menos no Anexo I): é o
//! teto do mês, não o valor da guia — quem apura é o contador.

use std::collections::BTreeMap;

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
use cardeal_kernel::{Arredondamento, Competencia, Dinheiro, Percentual};
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::{Comando, Consulta, Risco};
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::precificacao::{aliquota_efetiva, faixa_do_simples, AnexoSimples, RegimeImposto};

/// O que o Cardeal não sabe sozinho: o mês de abertura da empresa (para anualizar a receita no
/// primeiro ano) e a receita bruta de cada mês de antes do Cardeal (o Razão começa na
/// migração). Somada à do Razão, mês a mês.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoricoDoSimples {
    /// O mês de abertura (CNPJ), se a empresa tem menos de um ano.
    pub inicio_atividade: Option<Competencia>,
    /// A receita bruta de meses de antes do Cardeal.
    pub receita_anterior: Vec<(Competencia, Dinheiro)>,
}

impl HistoricoDoSimples {
    /// Confere os valores.
    ///
    /// # Errors
    /// A mensagem para a tela.
    pub fn validar(&self, hoje: Competencia) -> Result<(), String> {
        if self.inicio_atividade.is_some_and(|i| i > hoje) {
            return Err("o mês de abertura não pode ser no futuro".into());
        }
        if self.receita_anterior.len() > 36 {
            return Err("informe no máximo 36 meses".into());
        }
        let mut vistos = std::collections::BTreeSet::new();
        for (c, v) in &self.receita_anterior {
            if v.e_negativo() {
                return Err(format!(
                    "a receita de {} não pode ser negativa",
                    c.formatar()
                ));
            }
            if *c > hoje {
                return Err(format!("{} ainda não aconteceu", c.formatar()));
            }
            if !vistos.insert(*c) {
                return Err(format!("{} aparece duas vezes", c.formatar()));
            }
        }
        Ok(())
    }
}

/// O resumo do Simples de um mês (Financeiro › Resultado e caixa). Mostra o faturamento da
/// empresa: permissão própria, que os papéis do caixa, do vendedor e do estoquista não têm.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ResumoDoSimplesDoMes {
    /// O mês.
    pub mes: Competencia,
}

impl Consulta for ResumoDoSimplesDoMes {
    type Saida = ResumoDoSimples;
    const PERMISSAO: &'static str = "estoque.preco.simples.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<ResumoDoSimples> {
        let regime = crate::repositorio_preco::parametros_preco(c, ctx.empresa)?.regime;
        let historico = sqlite::historico(c, ctx.empresa)?;
        let receitas = sqlite::receitas_por_mes(c, ctx.empresa, self.mes, &historico)?;
        Ok(resumir(
            regime,
            &receitas,
            self.mes,
            historico.inicio_atividade,
        ))
    }
}

/// O histórico do Simples da empresa (para editar).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct HistoricoDoSimplesDaEmpresa;

impl Consulta for HistoricoDoSimplesDaEmpresa {
    type Saida = HistoricoDoSimples;
    const PERMISSAO: &'static str = "estoque.preco.simples.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<HistoricoDoSimples> {
        sqlite::historico(c, ctx.empresa)
    }
}

/// Grava o histórico do Simples (substitui o anterior).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefinirHistoricoDoSimples {
    /// O histórico.
    pub historico: HistoricoDoSimples,
}

impl Comando for DefinirHistoricoDoSimples {
    type Saida = ();
    const PERMISSAO: &'static str = "estoque.tributario.editar";
    const RISCO: Risco = Risco::Alto;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<()> {
        self.historico
            .validar(ctx.hoje().competencia())
            .map_err(|m| {
                cardeal_kernel::Erro::novo(cardeal_kernel::CodigoErro::FORA_DA_FAIXA, m)
            })?;
        sqlite::salvar_historico(uow.conexao(), ctx.empresa, &self.historico)
    }
}

/// O sublimite estadual: acima dele, ICMS e ISS saem do DAS.
pub const SUBLIMITE: Dinheiro = Dinheiro::reais(3_600_000);
/// O limite do Simples.
pub const LIMITE: Dinheiro = Dinheiro::reais(4_800_000);
/// O limite anual do MEI.
pub const LIMITE_MEI: Dinheiro = Dinheiro::reais(81_000);

/// Como o RBT12 do mês foi obtido.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrigemDoRbt12 {
    /// A soma dos 12 meses anteriores.
    DozeMeses,
    /// Primeiro mês de atividade: a receita do próprio mês × 12.
    PrimeiroMes,
    /// Início de atividade: a média dos `meses` anteriores × 12.
    Media {
        /// Quantos meses de atividade antes deste.
        meses: u32,
    },
    /// O histórico tem só `meses` com receita e o início de atividade não foi informado: somou
    /// o que há (fica menor que o real se a empresa é mais antiga).
    HistoricoCurto {
        /// Quantos meses de histórico antes deste.
        meses: u32,
    },
}

/// Um mês do Simples.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MesDoSimples {
    /// O mês.
    pub competencia: Competencia,
    /// A receita bruta do mês.
    pub receita: Dinheiro,
    /// O RBT12 que vale para ele.
    pub rbt12: Dinheiro,
    /// Como o RBT12 foi obtido.
    pub origem: OrigemDoRbt12,
    /// A alíquota efetiva (`None` fora do Simples, e no MEI).
    pub aliquota: Option<Percentual>,
    /// O DAS estimado (receita × alíquota efetiva).
    pub das: Option<Dinheiro>,
}

/// Um aviso do resumo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AvisoDoSimples {
    /// O RBT12 passou de 80% do sublimite (R$ 3,6 mi).
    PertoDoSublimite,
    /// O RBT12 passou do sublimite: ICMS e ISS saem do DAS.
    AcimaDoSublimite,
    /// O RBT12 passou de 80% do limite (R$ 4,8 mi).
    PertoDoLimite,
    /// O RBT12 passou do limite do Simples.
    AcimaDoLimite,
    /// A receita do ano passou de 80% do limite do MEI.
    MeiPertoDoLimite,
    /// A receita do ano passou do limite do MEI.
    MeiAcimaDoLimite,
    /// O histórico é curto e o início de atividade não foi informado.
    HistoricoCurto {
        /// Quantos meses de receita há antes do mês.
        meses: u32,
    },
}

impl AvisoDoSimples {
    /// O texto para a tela.
    #[must_use]
    pub fn texto(&self) -> String {
        match self {
            Self::PertoDoSublimite => "O faturamento dos últimos 12 meses passou de 80% do \
                                       sublimite de R$ 3,6 milhões: acima dele, ICMS e ISS saem \
                                       do DAS. Converse com o contador."
                .into(),
            Self::AcimaDoSublimite => "O faturamento dos últimos 12 meses passou do sublimite de \
                                       R$ 3,6 milhões: ICMS e ISS deixam de ser pagos no DAS. \
                                       Converse com o contador."
                .into(),
            Self::PertoDoLimite => "O faturamento dos últimos 12 meses passou de 80% do limite \
                                    do Simples (R$ 4,8 milhões)."
                .into(),
            Self::AcimaDoLimite => "O faturamento dos últimos 12 meses passou do limite do \
                                    Simples (R$ 4,8 milhões): a empresa sai do regime. Converse \
                                    com o contador."
                .into(),
            Self::MeiPertoDoLimite => "A receita do ano passou de 80% do limite do MEI \
                                       (R$ 81 mil)."
                .into(),
            Self::MeiAcimaDoLimite => "A receita do ano passou do limite do MEI (R$ 81 mil): a \
                                       empresa passa para o Simples. Converse com o contador."
                .into(),
            Self::HistoricoCurto { meses } => format!(
                "O Cardeal tem só {meses} mês(es) de receita antes deste. Se a empresa é mais \
                 antiga, informe o faturamento de antes do Cardeal; se abriu há menos de um ano, \
                 informe o mês de abertura — senão o faturamento dos 12 meses (e a alíquota) sai \
                 menor que o real."
            ),
        }
    }
}

/// Em que regime o resumo foi feito.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegimeDoResumo {
    /// Simples Nacional, no anexo.
    Simples(AnexoSimples),
    /// MEI (DAS fixo).
    Mei,
    /// Lucro presumido ou real: o resumo não se aplica.
    ForaDoSimples,
}

/// O resumo do Simples de um mês.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumoDoSimples {
    /// O regime.
    pub regime: RegimeDoResumo,
    /// O mês pedido.
    pub mes: MesDoSimples,
    /// Os 12 meses até o pedido, o mais antigo primeiro.
    pub meses: Vec<MesDoSimples>,
    /// A faixa do mês (1 a 6), no Simples.
    pub faixa: Option<u8>,
    /// O teto da faixa.
    pub teto_da_faixa: Option<Dinheiro>,
    /// A próxima faixa: a partir de que RBT12, e a alíquota efetiva logo no começo dela.
    pub proxima_faixa: Option<(Dinheiro, Percentual)>,
    /// A receita do ano-calendário até o mês.
    pub receita_no_ano: Dinheiro,
    /// Os avisos.
    pub avisos: Vec<AvisoDoSimples>,
}

/// A posição do mês numa contagem contínua (para medir a distância entre meses).
fn indice(c: Competencia) -> i64 {
    i64::from(c.ano()) * 12 + i64::from(c.mes())
}

fn receita(receitas: &BTreeMap<Competencia, Dinheiro>, c: Competencia) -> Dinheiro {
    receitas.get(&c).copied().unwrap_or(Dinheiro::ZERO)
}

/// O RBT12 do mês `mes`, a partir da receita de cada mês e do início de atividade (o mês de
/// abertura), se informado.
#[must_use]
pub fn rbt12(
    receitas: &BTreeMap<Competencia, Dinheiro>,
    mes: Competencia,
    inicio: Option<Competencia>,
) -> (Dinheiro, OrigemDoRbt12) {
    let mut anteriores = Vec::with_capacity(12);
    let mut c = mes;
    for _ in 0..12 {
        c = c.anterior();
        anteriores.push(c);
    }
    let soma: Dinheiro = anteriores.iter().map(|c| receita(receitas, *c)).sum();
    if let Some(inicio) = inicio.filter(|i| *i <= mes) {
        let meses = indice(mes) - indice(inicio);
        if meses == 0 {
            return (receita(receitas, mes) * 12, OrigemDoRbt12::PrimeiroMes);
        }
        if meses < 12 {
            let n = u32::try_from(meses).unwrap_or(1);
            let soma_n: Dinheiro = anteriores
                .iter()
                .filter(|c| **c >= inicio)
                .map(|c| receita(receitas, *c))
                .sum();
            return (
                soma_n.fracao(12, meses, Arredondamento::MeioAcima),
                OrigemDoRbt12::Media { meses: n },
            );
        }
        return (soma, OrigemDoRbt12::DozeMeses);
    }
    let primeiro = receitas
        .iter()
        .find(|(c, v)| v.e_positivo() && **c < mes)
        .map(|(c, _)| *c);
    match primeiro {
        Some(p) if indice(mes) - indice(p) < 12 => (
            soma,
            OrigemDoRbt12::HistoricoCurto {
                meses: u32::try_from(indice(mes) - indice(p)).unwrap_or(0),
            },
        ),
        _ => (soma, OrigemDoRbt12::DozeMeses),
    }
}

fn mes_do_simples(
    regime: RegimeImposto,
    receitas: &BTreeMap<Competencia, Dinheiro>,
    mes: Competencia,
    inicio: Option<Competencia>,
) -> MesDoSimples {
    let (rbt12, origem) = rbt12(receitas, mes, inicio);
    let receita = receita(receitas, mes);
    let aliquota = match regime {
        RegimeImposto::Simples(anexo) => Some(aliquota_efetiva(anexo, rbt12)),
        RegimeImposto::Mei | RegimeImposto::Manual(_) => None,
    };
    MesDoSimples {
        competencia: mes,
        receita,
        rbt12,
        origem,
        das: aliquota.map(|a| receita.aplicar(a, Arredondamento::MeioAcima)),
        aliquota,
    }
}

/// Monta o resumo do mês `mes`.
#[must_use]
pub fn resumir(
    regime: RegimeImposto,
    receitas: &BTreeMap<Competencia, Dinheiro>,
    mes: Competencia,
    inicio: Option<Competencia>,
) -> ResumoDoSimples {
    let mut meses = Vec::with_capacity(12);
    let mut c = mes;
    for _ in 0..12 {
        meses.push(mes_do_simples(regime, receitas, c, inicio));
        c = c.anterior();
    }
    meses.reverse();
    let atual = meses
        .last()
        .cloned()
        .unwrap_or_else(|| mes_do_simples(regime, receitas, mes, inicio));
    let receita_no_ano: Dinheiro = receitas
        .range(Competencia::nova(mes.ano(), 1)..=mes)
        .map(|(_, v)| *v)
        .sum();
    let mut avisos = Vec::new();
    if let OrigemDoRbt12::HistoricoCurto { meses } = atual.origem {
        if matches!(regime, RegimeImposto::Simples(_)) {
            avisos.push(AvisoDoSimples::HistoricoCurto { meses });
        }
    }
    let oitenta = |d: Dinheiro| d.fracao(8, 10, Arredondamento::MeioAcima);
    let (regime_do_resumo, faixa, teto, proxima) = match regime {
        RegimeImposto::Simples(anexo) => {
            if atual.rbt12 > LIMITE {
                avisos.push(AvisoDoSimples::AcimaDoLimite);
            } else if atual.rbt12 > SUBLIMITE {
                avisos.push(AvisoDoSimples::AcimaDoSublimite);
                if atual.rbt12 > oitenta(LIMITE) {
                    avisos.push(AvisoDoSimples::PertoDoLimite);
                }
            } else if atual.rbt12 > oitenta(SUBLIMITE) {
                avisos.push(AvisoDoSimples::PertoDoSublimite);
            }
            let (indice, teto) = faixa_do_simples(anexo, atual.rbt12);
            let proxima = (indice < 5).then(|| {
                let inicio_da_proxima = teto + Dinheiro::centavos(1);
                (
                    inicio_da_proxima,
                    aliquota_efetiva(anexo, inicio_da_proxima),
                )
            });
            (
                RegimeDoResumo::Simples(anexo),
                Some(u8::try_from(indice + 1).unwrap_or(6)),
                Some(teto),
                proxima,
            )
        }
        RegimeImposto::Mei => {
            if receita_no_ano > LIMITE_MEI {
                avisos.push(AvisoDoSimples::MeiAcimaDoLimite);
            } else if receita_no_ano > oitenta(LIMITE_MEI) {
                avisos.push(AvisoDoSimples::MeiPertoDoLimite);
            }
            (RegimeDoResumo::Mei, None, None, None)
        }
        RegimeImposto::Manual(_) => (RegimeDoResumo::ForaDoSimples, None, None, None),
    };
    ResumoDoSimples {
        regime: regime_do_resumo,
        mes: atual,
        meses,
        faixa,
        teto_da_faixa: teto,
        proxima_faixa: proxima,
        receita_no_ano,
        avisos,
    }
}

#[cfg(feature = "sqlite")]
pub(crate) mod sqlite {
    use std::collections::BTreeMap;

    use cardeal_kernel::{CodigoErro, Competencia, Dinheiro, Erro, Id, Resultado};
    use rusqlite::{params, Connection, OptionalExtension};

    use super::HistoricoDoSimples;

    #[allow(clippy::needless_pass_by_value)]
    fn falha(e: rusqlite::Error) -> Erro {
        Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string())
    }

    fn competencia(aaaamm: i64) -> Option<Competencia> {
        let n = u32::try_from(aaaamm).ok()?;
        let (ano, mes) = (i32::try_from(n / 100).ok()?, n % 100);
        (1..=12).contains(&mes).then(|| Competencia::nova(ano, mes))
    }

    pub(crate) fn historico(c: &Connection, empresa: Id) -> Resultado<HistoricoDoSimples> {
        let e = empresa.em_bytes();
        let inicio: Option<i64> = c
            .query_row(
                "SELECT inicio_atividade FROM estoque_simples_historico WHERE empresa = ?1",
                params![e.as_slice()],
                |r| r.get(0),
            )
            .optional()
            .map_err(falha)?
            .flatten();
        let mut stmt = c
            .prepare(
                "SELECT competencia, valor FROM estoque_simples_receita_anterior
                 WHERE empresa = ?1 ORDER BY competencia",
            )
            .map_err(falha)?;
        let receita_anterior = stmt
            .query_map(params![e.as_slice()], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
            })
            .map_err(falha)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(falha)?
            .into_iter()
            .filter_map(|(c, v)| Some((competencia(c)?, Dinheiro::centavos(v))))
            .collect();
        Ok(HistoricoDoSimples {
            inicio_atividade: inicio.and_then(competencia),
            receita_anterior,
        })
    }

    pub(crate) fn salvar_historico(
        c: &Connection,
        empresa: Id,
        h: &HistoricoDoSimples,
    ) -> Resultado<()> {
        let e = empresa.em_bytes();
        c.execute(
            "INSERT INTO estoque_simples_historico (empresa, inicio_atividade) VALUES (?1, ?2)
             ON CONFLICT (empresa) DO UPDATE SET inicio_atividade = excluded.inicio_atividade",
            params![e.as_slice(), h.inicio_atividade.map(Competencia::em_aaaamm)],
        )
        .map_err(falha)?;
        c.execute(
            "DELETE FROM estoque_simples_receita_anterior WHERE empresa = ?1",
            params![e.as_slice()],
        )
        .map_err(falha)?;
        for (comp, v) in &h.receita_anterior {
            c.execute(
                "INSERT INTO estoque_simples_receita_anterior (empresa, competencia, valor)
                 VALUES (?1, ?2, ?3)",
                params![e.as_slice(), comp.em_aaaamm(), v.em_centavos()],
            )
            .map_err(falha)?;
        }
        Ok(())
    }

    /// A receita bruta de cada mês (o Razão mais a de antes do Cardeal), dos 24 meses até `ate`.
    pub(crate) fn receitas_por_mes(
        c: &Connection,
        empresa: Id,
        ate: Competencia,
        historico: &HistoricoDoSimples,
    ) -> Resultado<BTreeMap<Competencia, Dinheiro>> {
        let mut receitas = BTreeMap::new();
        let mut m = ate;
        for _ in 0..24 {
            let linhas = cardeal_ledger::resultado_por_conta(c, empresa, m.periodo())
                .map_err(|e| Erro::novo(CodigoErro::FALHA_INTERNA, format!("Simples: {e}")))?;
            let receita = cardeal_ledger::resumir(&linhas).receita_bruta;
            if !receita.e_zero() {
                receitas.insert(m, receita);
            }
            m = m.anterior();
        }
        for (comp, v) in &historico.receita_anterior {
            if *comp <= ate {
                *receitas.entry(*comp).or_insert(Dinheiro::ZERO) += *v;
            }
        }
        Ok(receitas)
    }

    /// O RBT12 da empresa para o mês `mes` — o que o preço sugerido usa.
    pub(crate) fn rbt12_da_empresa(
        c: &Connection,
        empresa: Id,
        mes: Competencia,
    ) -> Resultado<Dinheiro> {
        let historico = historico(c, empresa)?;
        let receitas = receitas_por_mes(c, empresa, mes, &historico)?;
        Ok(super::rbt12(&receitas, mes, historico.inicio_atividade).0)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn c(ano: i32, mes: u32) -> Competencia {
        Competencia::nova(ano, mes)
    }

    fn receitas(de: Competencia, valores: &[i64]) -> BTreeMap<Competencia, Dinheiro> {
        let mut m = BTreeMap::new();
        let mut atual = de;
        for v in valores {
            m.insert(atual, Dinheiro::reais(*v));
            atual = atual.proxima();
        }
        m
    }

    #[test]
    fn o_rbt12_sao_os_doze_meses_anteriores_e_nao_o_ano() {
        // 24 meses de R$ 30 mil: o RBT12 de outubro/2026 é out/2025 a set/2026 = R$ 360 mil.
        let r = receitas(c(2024, 11), &[30_000; 24]);
        let (rbt12, origem) = rbt12(&r, c(2026, 10), None);
        assert_eq!(rbt12, Dinheiro::reais(360_000));
        assert_eq!(origem, OrigemDoRbt12::DozeMeses);
    }

    #[test]
    fn o_inicio_de_atividade_anualiza() {
        let inicio = c(2026, 6);
        let r = receitas(inicio, &[10_000, 20_000, 30_000, 40_000, 50_000]);
        // 1º mês: a receita do próprio mês × 12.
        assert_eq!(
            rbt12(&r, inicio, Some(inicio)),
            (Dinheiro::reais(120_000), OrigemDoRbt12::PrimeiroMes)
        );
        // 5º mês (outubro): a média de jun–set (25 mil) × 12.
        assert_eq!(
            rbt12(&r, c(2026, 10), Some(inicio)),
            (Dinheiro::reais(300_000), OrigemDoRbt12::Media { meses: 4 })
        );
        // Sem o início informado: soma o que há e avisa.
        let (soma, origem) = rbt12(&r, c(2026, 10), None);
        assert_eq!(soma, Dinheiro::reais(100_000));
        assert_eq!(origem, OrigemDoRbt12::HistoricoCurto { meses: 4 });
    }

    #[test]
    fn o_das_usa_a_aliquota_efetiva_e_nao_a_nominal() {
        // Comércio com R$ 360 mil em 12 meses: 2ª faixa, nominal 7,30%, parcela R$ 5.940 →
        // efetiva = 7,30% − 5.940/360.000 = 5,65%. O gestao-raiz cobraria os 7,30%.
        let r = receitas(c(2025, 10), &[30_000; 13]);
        let s = resumir(
            RegimeImposto::Simples(AnexoSimples::I),
            &r,
            c(2026, 10),
            None,
        );
        assert_eq!(s.mes.aliquota, Some(Percentual::milesimos(5_650)));
        assert_eq!(s.mes.das, Some(Dinheiro::reais(1_695)));
        assert_eq!(s.faixa, Some(2));
        assert_eq!(s.teto_da_faixa, Some(Dinheiro::reais(360_000)));
        let (proxima, aliquota) = s.proxima_faixa.unwrap();
        assert_eq!(proxima, Dinheiro::partes(360_000, 1));
        assert!(aliquota.unidades_internas() > Percentual::milesimos(5_650).unidades_internas());
        assert_eq!(s.meses.len(), 12);
        assert_eq!(s.meses[0].competencia, c(2025, 11));
        assert_eq!(s.receita_no_ano, Dinheiro::reais(300_000));
        assert!(s.avisos.is_empty());
    }

    #[test]
    fn os_limites_avisam() {
        let perto = receitas(c(2025, 10), &[250_000; 13]); // 3 mi em 12 meses
        let s = resumir(
            RegimeImposto::Simples(AnexoSimples::I),
            &perto,
            c(2026, 10),
            None,
        );
        assert_eq!(s.avisos, vec![AvisoDoSimples::PertoDoSublimite]);
        let acima = receitas(c(2025, 10), &[350_000; 13]); // 4,2 mi
        let s = resumir(
            RegimeImposto::Simples(AnexoSimples::I),
            &acima,
            c(2026, 10),
            None,
        );
        assert_eq!(
            s.avisos,
            vec![
                AvisoDoSimples::AcimaDoSublimite,
                AvisoDoSimples::PertoDoLimite
            ]
        );
        let mei = receitas(c(2026, 1), &[7_000; 10]); // 70 mil no ano
        let s = resumir(RegimeImposto::Mei, &mei, c(2026, 10), None);
        assert_eq!(s.regime, RegimeDoResumo::Mei);
        assert_eq!(s.mes.das, None);
        assert_eq!(s.avisos, vec![AvisoDoSimples::MeiPertoDoLimite]);
        // Nenhum texto sai com o recuo do código no meio.
        // O histórico recusa o que não faz sentido.
        let hoje = c(2026, 10);
        assert!(HistoricoDoSimples {
            inicio_atividade: Some(c(2026, 11)),
            ..HistoricoDoSimples::default()
        }
        .validar(hoje)
        .is_err());
        assert!(HistoricoDoSimples {
            inicio_atividade: None,
            receita_anterior: vec![
                (c(2026, 1), Dinheiro::reais(1)),
                (c(2026, 1), Dinheiro::reais(2))
            ],
        }
        .validar(hoje)
        .is_err());
        for a in [
            AvisoDoSimples::PertoDoSublimite,
            AvisoDoSimples::AcimaDoSublimite,
            AvisoDoSimples::PertoDoLimite,
            AvisoDoSimples::AcimaDoLimite,
            AvisoDoSimples::MeiPertoDoLimite,
            AvisoDoSimples::MeiAcimaDoLimite,
            AvisoDoSimples::HistoricoCurto { meses: 3 },
        ] {
            assert!(!a.texto().contains("  "), "{}", a.texto());
        }
    }
}
