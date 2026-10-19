//! O balancete de verificação e o diário — os dois relatórios que o contador pede todo mês. Só
//! lê. Entram os lançamentos `Confirmado`, `Realizado` e `Estornado` pela **competência** (a
//! mesma data em todas as partidas de um lançamento, então o período sempre fecha: débitos =
//! créditos).
//!
//! Sem encerramento de exercício no sistema, as contas de resultado mostram só o acumulado do
//! **ano** do período; o resultado dos anos anteriores aparece numa linha própria do patrimônio
//! líquido ("Resultado de exercícios anteriores"), para o balancete continuar fechando.

use std::cmp::Ordering;

use cardeal_kernel::{Data, Dinheiro};
use serde::{Deserialize, Serialize};

use crate::conta::Natureza;

/// Uma conta no balancete. Valores com o sinal das partidas: débito positivo, crédito negativo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinhaBalancete {
    /// O código (`"1.1.02"`); vazio na linha do resultado de exercícios anteriores.
    pub codigo: String,
    /// O nome.
    pub nome: String,
    /// A natureza.
    pub natureza: Natureza,
    /// Agrupadora (soma das filhas) ou que recebe lançamento.
    pub sintetica: bool,
    /// O nível na árvore (1 = `"1"`).
    pub nivel: u8,
    /// O saldo antes do período (resultado: só desde o início do ano).
    pub saldo_anterior: Dinheiro,
    /// Os débitos do período (positivo).
    pub debitos: Dinheiro,
    /// Os créditos do período (positivo).
    pub creditos: Dinheiro,
}

impl LinhaBalancete {
    /// Saldo anterior + débitos − créditos.
    #[must_use]
    pub fn saldo_atual(&self) -> Dinheiro {
        self.saldo_anterior + self.debitos - self.creditos
    }
}

/// Uma partida no diário.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartidaDoDiario {
    /// O código da conta.
    pub codigo: String,
    /// O nome da conta.
    pub nome: String,
    /// Débito positivo, crédito negativo.
    pub valor: Dinheiro,
}

/// Um lançamento no diário.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LancamentoDoDiario {
    /// O número sequencial do lançamento na empresa.
    pub numero: i64,
    /// A competência.
    pub data: Data,
    /// O histórico.
    pub historico: String,
    /// `Confirmado`, `Realizado` ou `Estornado`.
    pub estado: String,
    /// As partidas, na ordem.
    pub partidas: Vec<PartidaDoDiario>,
}

/// Ordena códigos de conta pela hierarquia numérica (`1.10` depois de `1.9`).
#[must_use]
pub fn comparar_codigos(a: &str, b: &str) -> Ordering {
    let segs = |s: &str| -> Vec<u32> { s.split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    segs(a).cmp(&segs(b))
}

/// Uma conta lida do plano: código, nome, natureza, sintética.
pub(crate) type ContaDoPlano = (String, String, Natureza, bool);

/// Monta o balancete a partir das contas e dos valores de cada analítica — (código, saldo
/// anterior, débitos, créditos) —, somando nas agrupadoras e acrescentando o resultado dos
/// exercícios anteriores. Domínio puro.
#[must_use]
pub fn montar_balancete(
    contas: &[ContaDoPlano],
    valores: &[(String, Dinheiro, Dinheiro, Dinheiro)],
    resultado_anos_anteriores: Dinheiro,
) -> Vec<LinhaBalancete> {
    use std::collections::BTreeMap;
    // código → (anterior, débitos, créditos), já somado em todos os ancestrais.
    let mut soma: BTreeMap<String, (Dinheiro, Dinheiro, Dinheiro)> = BTreeMap::new();
    for (codigo, anterior, debitos, creditos) in valores {
        if anterior.e_zero() && debitos.e_zero() && creditos.e_zero() {
            continue;
        }
        let mut atual = codigo.as_str();
        loop {
            let e = soma.entry(atual.to_owned()).or_insert((
                Dinheiro::ZERO,
                Dinheiro::ZERO,
                Dinheiro::ZERO,
            ));
            e.0 += *anterior;
            e.1 += *debitos;
            e.2 += *creditos;
            match atual.rfind('.') {
                Some(i) => atual = &atual[..i],
                None => break,
            }
        }
    }
    let mut linhas: Vec<LinhaBalancete> = soma
        .into_iter()
        .filter_map(|(codigo, (anterior, debitos, creditos))| {
            let (_, nome, natureza, sintetica) = contas.iter().find(|c| c.0 == codigo)?;
            Some(LinhaBalancete {
                nivel: u8::try_from(codigo.split('.').count()).unwrap_or(u8::MAX),
                codigo,
                nome: nome.clone(),
                natureza: *natureza,
                sintetica: *sintetica,
                saldo_anterior: anterior,
                debitos,
                creditos,
            })
        })
        .collect();
    linhas.sort_by(|a, b| comparar_codigos(&a.codigo, &b.codigo));
    if !resultado_anos_anteriores.e_zero() {
        // Logo depois do último item do patrimônio líquido (ou no fim).
        let pos = linhas
            .iter()
            .rposition(|l| l.natureza == Natureza::PatrimonioLiquido)
            .map_or(linhas.len(), |i| i + 1);
        linhas.insert(
            pos,
            LinhaBalancete {
                codigo: String::new(),
                nome: "Resultado de exercícios anteriores (não encerrado)".to_owned(),
                natureza: Natureza::PatrimonioLiquido,
                sintetica: false,
                nivel: 2,
                saldo_anterior: resultado_anos_anteriores,
                debitos: Dinheiro::ZERO,
                creditos: Dinheiro::ZERO,
            },
        );
    }
    linhas
}

#[cfg(feature = "sqlite")]
mod sql {
    use cardeal_kernel::{Data, Dinheiro, Id, Periodo};
    use rusqlite::Connection;

    use super::{
        montar_balancete, ContaDoPlano, LancamentoDoDiario, LinhaBalancete, PartidaDoDiario,
    };
    use crate::conta::Natureza;
    use crate::erros::ErroRazao;

    #[allow(clippy::needless_pass_by_value)]
    fn persist(e: rusqlite::Error) -> ErroRazao {
        ErroRazao::FalhaDePersistencia(e.to_string())
    }

    const FATOS: &str = "rl.estado IN ('Confirmado','Realizado','Estornado')";

    fn natureza(s: &str) -> Natureza {
        match s {
            "Ativo" => Natureza::Ativo,
            "Passivo" => Natureza::Passivo,
            "PatrimonioLiquido" => Natureza::PatrimonioLiquido,
            "Receita" => Natureza::Receita,
            _ => Natureza::Despesa,
        }
    }

    /// O balancete do período (ver o módulo).
    ///
    /// # Errors
    /// [`ErroRazao::FalhaDePersistencia`] em erro do SQLite.
    pub fn balancete(
        conexao: &Connection,
        empresa: Id,
        periodo: Periodo,
    ) -> Result<Vec<LinhaBalancete>, ErroRazao> {
        let e = empresa.em_bytes().to_vec();
        let mut stmt = conexao
            .prepare("SELECT codigo, nome, natureza, tipo FROM razao_conta WHERE empresa = ?1")
            .map_err(persist)?;
        let contas: Vec<ContaDoPlano> = stmt
            .query_map([&e], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    natureza(&r.get::<_, String>(2)?),
                    r.get::<_, String>(3)? == "Sintetica",
                ))
            })
            .map_err(persist)?
            .collect::<rusqlite::Result<_>>()
            .map_err(persist)?;
        let de = i64::from(periodo.de.em_dias());
        let ate = i64::from(periodo.ate.em_dias());
        let inicio_ano =
            Data::de_ymd(periodo.de.ano(), 1, 1).map_or(de, |d| i64::from(d.em_dias()));
        // Por conta: anterior (patrimonial: tudo antes; resultado: só no ano), anos anteriores
        // (só resultado), débitos e créditos do período.
        let sql = format!(
            "SELECT rc.codigo, rc.natureza IN ('Receita','Despesa'),
                    COALESCE(SUM(CASE WHEN rl.competencia < ?2 AND
                        (rc.natureza NOT IN ('Receita','Despesa') OR rl.competencia >= ?4)
                        THEN rp.valor END), 0),
                    COALESCE(SUM(CASE WHEN rl.competencia < ?4
                        AND rc.natureza IN ('Receita','Despesa') THEN rp.valor END), 0),
                    COALESCE(SUM(CASE WHEN rl.competencia BETWEEN ?2 AND ?3 AND rp.valor > 0
                        THEN rp.valor END), 0),
                    COALESCE(SUM(CASE WHEN rl.competencia BETWEEN ?2 AND ?3 AND rp.valor < 0
                        THEN -rp.valor END), 0)
             FROM razao_partida rp
             JOIN razao_lancamento rl ON rl.id = rp.lancamento
             JOIN razao_conta rc ON rc.id = rp.conta
             WHERE rl.empresa = ?1 AND {FATOS} AND rl.competencia <= ?3
             GROUP BY rc.id"
        );
        let mut stmt = conexao.prepare(&sql).map_err(persist)?;
        let mut anos_anteriores = Dinheiro::ZERO;
        let mut valores = Vec::new();
        let linhas = stmt
            .query_map(rusqlite::params![&e, de, ate, inicio_ano], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, i64>(5)?,
                ))
            })
            .map_err(persist)?;
        for l in linhas {
            let (codigo, anterior, antigos, debitos, creditos) = l.map_err(persist)?;
            anos_anteriores += Dinheiro::centavos(antigos);
            valores.push((
                codigo,
                Dinheiro::centavos(anterior),
                Dinheiro::centavos(debitos),
                Dinheiro::centavos(creditos),
            ));
        }
        Ok(montar_balancete(&contas, &valores, anos_anteriores))
    }

    /// O diário do período: cada lançamento com as partidas, por competência e número.
    ///
    /// # Errors
    /// [`ErroRazao::FalhaDePersistencia`] em erro do SQLite.
    pub fn diario(
        conexao: &Connection,
        empresa: Id,
        periodo: Periodo,
    ) -> Result<Vec<LancamentoDoDiario>, ErroRazao> {
        let sql = format!(
            "SELECT rl.numero, rl.competencia, rl.historico, rl.estado, rc.codigo, rc.nome, rp.valor
             FROM razao_lancamento rl
             JOIN razao_partida rp ON rp.lancamento = rl.id
             JOIN razao_conta rc ON rc.id = rp.conta
             WHERE rl.empresa = ?1 AND {FATOS} AND rl.competencia BETWEEN ?2 AND ?3
             ORDER BY rl.competencia, rl.numero, rp.ordem"
        );
        let mut stmt = conexao.prepare(&sql).map_err(persist)?;
        let linhas = stmt
            .query_map(
                rusqlite::params![
                    empresa.em_bytes().as_slice(),
                    i64::from(periodo.de.em_dias()),
                    i64::from(periodo.ate.em_dias())
                ],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, String>(5)?,
                        r.get::<_, i64>(6)?,
                    ))
                },
            )
            .map_err(persist)?;
        let mut saida: Vec<LancamentoDoDiario> = Vec::new();
        for l in linhas {
            let (numero, dia, historico, estado, codigo, nome, valor) = l.map_err(persist)?;
            if saida.last().is_none_or(|u| u.numero != numero) {
                saida.push(LancamentoDoDiario {
                    numero,
                    data: Data::de_dias(i32::try_from(dia).unwrap_or(0)),
                    historico,
                    estado,
                    partidas: Vec::new(),
                });
            }
            if let Some(u) = saida.last_mut() {
                u.partidas.push(PartidaDoDiario {
                    codigo,
                    nome,
                    valor: Dinheiro::centavos(valor),
                });
            }
        }
        Ok(saida)
    }
}

#[cfg(feature = "sqlite")]
pub use sql::{balancete, diario};

#[cfg(test)]
mod testes {
    use super::*;

    fn conta(codigo: &str, nome: &str, natureza: Natureza, sintetica: bool) -> ContaDoPlano {
        (codigo.into(), nome.into(), natureza, sintetica)
    }

    #[test]
    fn soma_nas_agrupadoras_ordena_e_fecha() {
        let contas = [
            conta("1", "Ativo", Natureza::Ativo, true),
            conta("1.1", "Disponível", Natureza::Ativo, true),
            conta("1.1.01", "Caixa", Natureza::Ativo, false),
            conta("1.1.10", "Banco novo", Natureza::Ativo, false),
            conta("1.1.9", "Banco velho", Natureza::Ativo, false),
            conta("3", "Patrimônio Líquido", Natureza::PatrimonioLiquido, true),
            conta("3.1", "Capital", Natureza::PatrimonioLiquido, false),
            conta("4", "Receitas", Natureza::Receita, true),
            conta("4.1", "Vendas", Natureza::Receita, false),
        ];
        let r = |v: i64| Dinheiro::reais(v);
        let valores = [
            ("1.1.01".to_owned(), r(100), r(50), r(20)),
            ("1.1.10".to_owned(), r(0), r(30), r(0)),
            ("1.1.9".to_owned(), r(0), r(0), r(0)), // sem movimento: some
            ("3.1".to_owned(), r(-60), r(0), r(0)),
            ("4.1".to_owned(), r(0), r(0), r(60)),
        ];
        // 40 de lucro de anos anteriores (crédito).
        let b = montar_balancete(&contas, &valores, r(-40));
        let codigos: Vec<&str> = b.iter().map(|l| l.codigo.as_str()).collect();
        assert_eq!(
            codigos,
            ["1", "1.1", "1.1.01", "1.1.10", "3", "3.1", "", "4", "4.1"]
        );
        let um = &b[0];
        assert!(um.sintetica);
        assert_eq!(
            (um.saldo_anterior, um.debitos, um.creditos),
            (r(100), r(80), r(20))
        );
        assert_eq!(um.saldo_atual(), r(160));
        // Fecha: anterior das analíticas soma zero, débitos = créditos.
        let analiticas = b.iter().filter(|l| !l.sintetica);
        let (ant, deb, cre) = analiticas.fold((r(0), r(0), r(0)), |(a, d, c), l| {
            (a + l.saldo_anterior, d + l.debitos, c + l.creditos)
        });
        assert_eq!(ant, Dinheiro::ZERO);
        assert_eq!(deb, cre);
    }

    #[test]
    fn codigos_em_ordem_numerica() {
        assert_eq!(comparar_codigos("1.1.9", "1.1.10"), Ordering::Less);
        assert_eq!(comparar_codigos("1", "1.1"), Ordering::Less);
        assert_eq!(comparar_codigos("2", "1.9"), Ordering::Greater);
    }
}
