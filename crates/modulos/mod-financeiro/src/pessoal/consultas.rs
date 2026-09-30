//! Leitura dos lançamentos pessoais — sempre só os do usuário da sessão.

use cardeal_kernel::{Data, Dinheiro, Id, Periodo, Resultado};
use cardeal_modkit::{Consulta, Ctx};
use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};

use super::{LancamentoPessoal, TipoPessoal};
use crate::repositorio::{blob, data_de, id_de, persist};

pub(super) const fn tipo_txt(t: TipoPessoal) -> &'static str {
    match t {
        TipoPessoal::Receita => "Receita",
        TipoPessoal::Despesa => "Despesa",
    }
}

fn de_linha(r: &Row<'_>) -> rusqlite::Result<LancamentoPessoal> {
    Ok(LancamentoPessoal {
        id: id_de(r.get(0)?),
        grupo: id_de(r.get(1)?),
        tipo: if r.get::<_, String>(2)? == "Receita" {
            TipoPessoal::Receita
        } else {
            TipoPessoal::Despesa
        },
        descricao: r.get(3)?,
        categoria: r.get(4)?,
        valor: Dinheiro::centavos(r.get(5)?),
        vencimento: data_de(r.get(6)?),
        parcela: u16::try_from(r.get::<_, i64>(7)?).unwrap_or(1),
        parcelas: u16::try_from(r.get::<_, i64>(8)?).unwrap_or(1),
        cartao: r.get(9)?,
        pago_em: r.get::<_, Option<i64>>(10)?.map(data_de),
    })
}

/// Os lançamentos do usuário com vencimento no período, por vencimento.
///
/// # Errors
/// Falha do SQLite.
pub fn lancamentos_do_usuario(
    conexao: &Connection,
    empresa: Id,
    usuario: Id,
    periodo: Periodo,
) -> Resultado<Vec<LancamentoPessoal>> {
    let mut stmt = conexao
        .prepare(
            "SELECT id, grupo, tipo, descricao, categoria, valor, vencimento, parcela, parcelas,
                    cartao, pago_em
             FROM financeiro_pessoal
             WHERE empresa = ?1 AND usuario = ?2 AND vencimento BETWEEN ?3 AND ?4
             ORDER BY vencimento, descricao",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map(
            params![
                blob(empresa),
                blob(usuario),
                i64::from(periodo.de.em_dias()),
                i64::from(periodo.ate.em_dias()),
            ],
            de_linha,
        )
        .map_err(persist)?;
    linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)
}

/// Os lançamentos pessoais do usuário da sessão com vencimento no período.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LancamentosPessoais {
    /// A janela de vencimentos.
    pub periodo: Periodo,
}

impl Consulta for LancamentosPessoais {
    type Saida = Vec<LancamentoPessoal>;
    const PERMISSAO: &'static str = "financeiro.pessoal";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        lancamentos_do_usuario(conexao, ctx.empresa, ctx.usuario, self.periodo)
    }
}

/// As categorias e cartões que o usuário já usou — sugestões do formulário.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SugestoesPessoais;

/// O que [`SugestoesPessoais`] devolve.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sugestoes {
    /// Categorias já usadas, por nome.
    pub categorias: Vec<String>,
    /// Cartões já usados, por nome.
    pub cartoes: Vec<String>,
}

impl Consulta for SugestoesPessoais {
    type Saida = Sugestoes;
    const PERMISSAO: &'static str = "financeiro.pessoal";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let distintos = |coluna: &str| -> Resultado<Vec<String>> {
            let sql = format!(
                "SELECT DISTINCT {coluna} FROM financeiro_pessoal
                 WHERE empresa = ?1 AND usuario = ?2 AND {coluna} IS NOT NULL
                 ORDER BY {coluna}"
            );
            let mut stmt = conexao.prepare(&sql).map_err(persist)?;
            let linhas = stmt
                .query_map([blob(ctx.empresa), blob(ctx.usuario)], |r| r.get(0))
                .map_err(persist)?;
            linhas
                .collect::<rusqlite::Result<Vec<String>>>()
                .map_err(persist)
        };
        Ok(Sugestoes {
            categorias: distintos("categoria")?,
            cartoes: distintos("cartao")?,
        })
    }
}

/// A data (em dias) para o SQL.
pub(super) fn dias(d: Data) -> i64 {
    i64::from(d.em_dias())
}
