//! Leitura do resultado por conta num período — o insumo da DRE gerencial e da apuração de
//! receita bruta (RBT12) que o motor de preço usa. Só lê: soma as partidas das contas de
//! `Receita` e `Despesa` por competência.
//!
//! Entram os lançamentos `Confirmado`, `Realizado` e `Estornado` (o estorno anula o
//! original, então a soma dos dois dá zero); `Previsto` fica de fora — é intenção, não fato.

use cardeal_kernel::{Dinheiro, Id, Periodo};
use rusqlite::Connection;

use crate::conta::Natureza;
use crate::erros::ErroRazao;

/// Uma conta de resultado com o valor do período, no sentido natural da conta: receita é
/// crédito menos débito, despesa é débito menos crédito (positivo = comportamento normal).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaResultado {
    /// A conta.
    pub conta: Id,
    /// O código hierárquico (`"5.4"`).
    pub codigo: String,
    /// O nome.
    pub nome: String,
    /// `Receita` ou `Despesa`.
    pub natureza: Natureza,
    /// O papel semântico gravado na conta (o nome do [`PapelConta`](crate::PapelConta)),
    /// ou o do ancestral mais próximo que tenha um — contas abertas pelo usuário herdam o
    /// papel da sintética sob a qual nasceram.
    pub papel: Option<String>,
    /// O valor do período.
    pub valor: Dinheiro,
}

/// id, código, nome, natureza e papel gravado.
type ContaDeResultado = (Vec<u8>, String, String, String, Option<String>);

#[allow(clippy::needless_pass_by_value)]
fn persist(e: rusqlite::Error) -> ErroRazao {
    ErroRazao::FalhaDePersistencia(e.to_string())
}

fn natureza_de(s: &str) -> Natureza {
    if s == "Receita" {
        Natureza::Receita
    } else {
        Natureza::Despesa
    }
}

/// Soma o resultado de cada conta de receita/despesa com movimento na competência do
/// período. Contas sem movimento não aparecem. Ordenado por código.
///
/// # Errors
/// [`ErroRazao::FalhaDePersistencia`] em erro do SQLite.
pub fn resultado_por_conta(
    conexao: &Connection,
    empresa: Id,
    periodo: Periodo,
) -> Result<Vec<LinhaResultado>, ErroRazao> {
    let empresa_b = empresa.em_bytes().to_vec();

    // Papel herdado: todas as contas de resultado, para resolver por prefixo de código.
    let mut stmt = conexao
        .prepare(
            "SELECT id, codigo, nome, natureza, papel_padrao FROM razao_conta
             WHERE empresa = ?1 AND natureza IN ('Receita','Despesa')",
        )
        .map_err(persist)?;
    let contas: Vec<ContaDeResultado> = stmt
        .query_map([&empresa_b], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .map_err(persist)?
        .collect::<rusqlite::Result<_>>()
        .map_err(persist)?;
    let papel_do_codigo = |codigo: &str| -> Option<String> {
        let mut atual = codigo;
        loop {
            if let Some((_, _, _, _, Some(p))) = contas.iter().find(|c| c.1 == atual) {
                return Some(p.clone());
            }
            atual = &atual[..atual.rfind('.')?];
        }
    };

    let mut stmt = conexao
        .prepare(
            "SELECT rp.conta, SUM(rp.valor)
             FROM razao_partida rp
             JOIN razao_lancamento rl ON rl.id = rp.lancamento
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
        let Some((_, codigo, nome, nat, _)) = contas.iter().find(|c| c.0 == conta_b) else {
            continue; // conta de balanço: não é resultado
        };
        let natureza = natureza_de(nat);
        // Partidas: débito positivo, crédito negativo.
        let valor = if natureza == Natureza::Receita {
            -soma
        } else {
            soma
        };
        saida.push(LinhaResultado {
            conta: <[u8; 16]>::try_from(conta_b).map_or(Id::NULO, Id::de_bytes),
            codigo: codigo.clone(),
            nome: nome.clone(),
            natureza,
            papel: papel_do_codigo(codigo),
            valor: Dinheiro::centavos(valor),
        });
    }
    saida.sort_by(|a, b| a.codigo.cmp(&b.codigo));
    Ok(saida)
}
