//! Leitura dos movimentos de caixa realizados — o insumo do DFC pelo método direto — e das
//! variações das contas patrimoniais fora do caixa — o insumo do método indireto.
//!
//! "Caixa" aqui é o que tem papel `Caixa`, `Bancos` ou `ValoresEmTransito` e toda conta aberta
//! sob "1.1 Disponível" (as contas bancárias que o usuário cria), exceto as aplicações. Só entram
//! lançamentos `Realizado`/`Estornado`, na data de liquidação (ou na
//! competência, se não houver).

use cardeal_kernel::{Data, Dinheiro, Id};
use serde::{Deserialize, Serialize};

use crate::conta::Natureza;

/// Uma conta patrimonial fora do caixa com o quanto as partidas dela somaram no período
/// (débito positivo, crédito negativo) — pela competência, nos mesmos lançamentos da DRE.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariacaoPatrimonial {
    /// O código da conta.
    pub codigo: String,
    /// O nome.
    pub nome: String,
    /// `Ativo`, `Passivo` ou `PatrimonioLiquido`.
    pub natureza: Natureza,
    /// O papel efetivo (próprio ou herdado), em texto.
    pub papel: Option<String>,
    /// `Operacional`, `Investimento` ou `Financiamento`, quando a conta classifica.
    pub grupo_fluxo: Option<String>,
    /// A soma das partidas no período.
    pub valor: Dinheiro,
}

/// A outra ponta de um movimento de caixa.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContrapartidaDeCaixa {
    /// O código da conta.
    pub codigo: String,
    /// O nome da conta.
    pub nome: String,
    /// O papel efetivo (próprio ou herdado), em texto.
    pub papel: Option<String>,
    /// `Operacional`, `Investimento` ou `Financiamento`, quando a conta classifica.
    pub grupo_fluxo: Option<String>,
    /// O valor da partida (débito positivo, crédito negativo).
    pub valor: Dinheiro,
}

/// Um lançamento que mexeu no caixa.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MovimentoDeCaixa {
    /// O lançamento.
    pub lancamento: Id,
    /// A data em que o dinheiro se moveu.
    pub data: Data,
    /// O efeito líquido no caixa: positivo = entrou, negativo = saiu; zero = transferência
    /// entre contas de caixa.
    pub caixa: Dinheiro,
    /// As partidas que não são de caixa.
    pub contrapartidas: Vec<ContrapartidaDeCaixa>,
}

#[cfg(feature = "sqlite")]
mod sql {
    use cardeal_kernel::{Data, Dinheiro, Id, Periodo};
    use rusqlite::{params_from_iter, types::Value, Connection};

    use super::{ContrapartidaDeCaixa, MovimentoDeCaixa, VariacaoPatrimonial};
    use crate::conta::Natureza;
    use crate::erros::ErroRazao;

    #[allow(clippy::needless_pass_by_value)]
    fn persist(e: rusqlite::Error) -> ErroRazao {
        ErroRazao::FalhaDePersistencia(e.to_string())
    }

    /// id, código, nome, papel gravado, grupo de fluxo, natureza.
    type Conta = (
        Vec<u8>,
        String,
        String,
        Option<String>,
        Option<String>,
        String,
    );

    fn contas(conexao: &Connection, empresa: &[u8]) -> Result<Vec<Conta>, ErroRazao> {
        let mut stmt = conexao
            .prepare(
                "SELECT id, codigo, nome, papel_padrao, grupo_fluxo, natureza
                 FROM razao_conta WHERE empresa = ?1",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map([empresa], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            })
            .map_err(persist)?
            .collect::<rusqlite::Result<_>>()
            .map_err(persist)?;
        Ok(linhas)
    }

    /// O papel da conta ou, na falta, o do ancestral mais próximo que tenha um.
    fn papel_efetivo(contas: &[Conta], codigo: &str) -> Option<String> {
        let mut atual = codigo;
        loop {
            if let Some(c) = contas.iter().find(|c| c.1 == atual) {
                if c.3.is_some() {
                    return c.3.clone();
                }
            }
            atual = &atual[..atual.rfind('.')?];
        }
    }

    /// Caixa é o que tem papel `Caixa`, `Bancos` ou `ValoresEmTransito` — e qualquer conta aberta
    /// sob "1.1 Disponível" (as contas bancárias que o usuário cria), menos as aplicações.
    fn e_caixa(contas: &[Conta], codigo: &str) -> bool {
        let papel = papel_efetivo(contas, codigo);
        match papel.as_deref() {
            Some("Caixa" | "Bancos" | "ValoresEmTransito") => true,
            Some("Aplicacoes") => false,
            _ => codigo.starts_with("1.1."),
        }
    }

    const DATA: &str = "COALESCE(rl.liquidacao, rl.competencia)";

    fn abertura(
        conexao: &Connection,
        empresa: &[u8],
        antes_de: Data,
        caixa: &[Vec<u8>],
    ) -> Result<Dinheiro, ErroRazao> {
        let marcas = vec!["?"; caixa.len()].join(",");
        let sql = format!(
            "SELECT COALESCE(SUM(rp.valor), 0)
             FROM razao_partida rp JOIN razao_lancamento rl ON rl.id = rp.lancamento
             WHERE rl.empresa = ?1 AND rl.estado IN ('Realizado','Estornado')
               AND {DATA} < ?2 AND rp.conta IN ({marcas})"
        );
        let mut args = vec![
            Value::Blob(empresa.to_vec()),
            Value::Integer(i64::from(antes_de.em_dias())),
        ];
        args.extend(caixa.iter().cloned().map(Value::Blob));
        let centavos: i64 = conexao
            .query_row(&sql, params_from_iter(args), |r| r.get(0))
            .map_err(persist)?;
        Ok(Dinheiro::centavos(centavos))
    }

    /// (lançamento, dia, conta, valor) de toda partida dos lançamentos do período que tocam o caixa.
    type Partida = (Vec<u8>, i64, Vec<u8>, i64);

    fn partidas(
        conexao: &Connection,
        empresa: &[u8],
        periodo: Periodo,
        caixa: &[Vec<u8>],
    ) -> Result<Vec<Partida>, ErroRazao> {
        let marcas = vec!["?"; caixa.len()].join(",");
        let sql = format!(
            "SELECT rl.id, {DATA}, rp.conta, rp.valor
             FROM razao_partida rp JOIN razao_lancamento rl ON rl.id = rp.lancamento
             WHERE rl.empresa = ?1 AND rl.estado IN ('Realizado','Estornado')
               AND {DATA} BETWEEN ?2 AND ?3
               AND rl.id IN (SELECT lancamento FROM razao_partida WHERE conta IN ({marcas}))
             ORDER BY {DATA}, rl.numero, rp.ordem"
        );
        let mut args = vec![
            Value::Blob(empresa.to_vec()),
            Value::Integer(i64::from(periodo.de.em_dias())),
            Value::Integer(i64::from(periodo.ate.em_dias())),
        ];
        args.extend(caixa.iter().cloned().map(Value::Blob));
        let mut stmt = conexao.prepare(&sql).map_err(persist)?;
        let linhas = stmt
            .query_map(params_from_iter(args), |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .map_err(persist)?
            .collect::<rusqlite::Result<_>>()
            .map_err(persist)?;
        Ok(linhas)
    }

    /// O saldo de abertura (caixa antes do período) e os movimentos de caixa do período.
    ///
    /// # Errors
    /// [`ErroRazao::FalhaDePersistencia`] em erro do SQLite.
    pub fn movimentos_de_caixa(
        conexao: &Connection,
        empresa: Id,
        periodo: Periodo,
    ) -> Result<(Dinheiro, Vec<MovimentoDeCaixa>), ErroRazao> {
        let empresa_b = empresa.em_bytes().to_vec();
        let contas = contas(conexao, &empresa_b)?;
        let caixa: Vec<Vec<u8>> = contas
            .iter()
            .filter(|c| e_caixa(&contas, &c.1))
            .map(|c| c.0.clone())
            .collect();
        if caixa.is_empty() {
            return Ok((Dinheiro::ZERO, Vec::new()));
        }
        let abertura = abertura(conexao, &empresa_b, periodo.de, &caixa)?;

        let mut saida: Vec<MovimentoDeCaixa> = Vec::new();
        let mut atual: Option<Vec<u8>> = None;
        for (lanc, dia, conta_b, valor) in partidas(conexao, &empresa_b, periodo, &caixa)? {
            if atual.as_ref() != Some(&lanc) {
                saida.push(MovimentoDeCaixa {
                    lancamento: <[u8; 16]>::try_from(lanc.as_slice())
                        .map_or(Id::NULO, Id::de_bytes),
                    data: Data::de_dias(i32::try_from(dia).unwrap_or(0)),
                    caixa: Dinheiro::ZERO,
                    contrapartidas: Vec::new(),
                });
                atual = Some(lanc);
            }
            let Some(m) = saida.last_mut() else { continue };
            if caixa.contains(&conta_b) {
                m.caixa += Dinheiro::centavos(valor);
            } else if let Some(c) = contas.iter().find(|c| c.0 == conta_b) {
                m.contrapartidas.push(ContrapartidaDeCaixa {
                    codigo: c.1.clone(),
                    nome: c.2.clone(),
                    papel: papel_efetivo(&contas, &c.1),
                    grupo_fluxo: c.4.clone(),
                    valor: Dinheiro::centavos(valor),
                });
            }
        }
        Ok((abertura, saida))
    }

    /// As contas patrimoniais (ativo, passivo, PL) **fora do caixa** com a soma das partidas dos
    /// lançamentos `Confirmado`/`Realizado`/`Estornado` cuja competência cai no período — o
    /// mesmo recorte da DRE, para lucro + variações fecharem com o caixa. Contas sem variação
    /// ficam de fora.
    ///
    /// # Errors
    /// [`ErroRazao::FalhaDePersistencia`] em erro do SQLite.
    pub fn variacoes_patrimoniais(
        conexao: &Connection,
        empresa: Id,
        periodo: Periodo,
    ) -> Result<Vec<VariacaoPatrimonial>, ErroRazao> {
        let empresa_b = empresa.em_bytes().to_vec();
        let contas = contas(conexao, &empresa_b)?;
        let mut stmt = conexao
            .prepare(
                "SELECT rp.conta, SUM(rp.valor)
                 FROM razao_partida rp JOIN razao_lancamento rl ON rl.id = rp.lancamento
                 WHERE rl.empresa = ?1 AND rl.estado IN ('Confirmado','Realizado','Estornado')
                   AND rl.competencia BETWEEN ?2 AND ?3
                 GROUP BY rp.conta",
            )
            .map_err(persist)?;
        let somas: Vec<(Vec<u8>, i64)> = stmt
            .query_map(
                rusqlite::params![
                    &empresa_b,
                    i64::from(periodo.de.em_dias()),
                    i64::from(periodo.ate.em_dias())
                ],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(persist)?
            .collect::<rusqlite::Result<_>>()
            .map_err(persist)?;
        let mut saida = Vec::new();
        for (conta_b, soma) in somas {
            let Some(c) = contas.iter().find(|c| c.0 == conta_b) else {
                continue;
            };
            let natureza = match c.5.as_str() {
                "Ativo" => Natureza::Ativo,
                "Passivo" => Natureza::Passivo,
                "PatrimonioLiquido" => Natureza::PatrimonioLiquido,
                _ => continue, // resultado: entra pelo lucro
            };
            if soma == 0 || e_caixa(&contas, &c.1) {
                continue;
            }
            saida.push(VariacaoPatrimonial {
                codigo: c.1.clone(),
                nome: c.2.clone(),
                natureza,
                papel: papel_efetivo(&contas, &c.1),
                grupo_fluxo: c.4.clone(),
                valor: Dinheiro::centavos(soma),
            });
        }
        saida.sort_by(|a, b| a.codigo.cmp(&b.codigo));
        Ok(saida)
    }
}

#[cfg(feature = "sqlite")]
pub use sql::{movimentos_de_caixa, variacoes_patrimoniais};
