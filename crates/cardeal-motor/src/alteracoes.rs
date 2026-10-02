//! O fluxo de alterações da empresa, lido do `nucleo_outbox` — a fonte do tempo real.
//!
//! Todo comando bem-sucedido deixa uma linha no outbox **na mesma transação** (o despachante
//! garante), além dos eventos de domínio que ele publicar. Quem acompanha a empresa guarda o
//! último `seq` visto e pede o que veio depois: nada se perde entre uma conexão e outra, e
//! uma lacuna (o outbox foi podado além do ponto do cliente) é dita explicitamente — o
//! cliente recarrega tudo em vez de ficar com uma tela velha em silêncio.

use cardeal_kernel::{Instante, Resultado};
use cardeal_storage::{ContextoEscrita, ErroArmazenamento};
use rusqlite::OptionalExtension;

use crate::erro_armazenamento;
use crate::motor::MotorLocal;

/// O que mudou depois de um ponto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alteracoes {
    /// `(seq, tipo)` em ordem — `tipo` é o nome do comando ou do evento (`os.ordem_aberta.v1`).
    pub itens: Vec<(u64, String)>,
    /// O último `seq` já emitido pela base (o ponto para continuar quando `itens` vem vazio).
    pub ultimo: u64,
    /// O ponto pedido não pode ser continuado sem perda (podado, ou de outra base): recarregue.
    pub lacuna: bool,
}

impl MotorLocal {
    /// O último `seq` já emitido (0 numa base sem alterações) — onde começa quem só quer o
    /// que vier daqui em diante.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn ultima_alteracao(&self) -> Resultado<u64> {
        self.arm
            .leitor()
            .consultar(|c| ultimo_seq(c).map_err(sqlite))
            .map_err(erro_armazenamento)
    }

    /// Até `limite` alterações depois de `desde`.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn alteracoes_desde(&self, desde: u64, limite: usize) -> Resultado<Alteracoes> {
        self.arm
            .leitor()
            .consultar(move |c| ler(c, desde, limite).map_err(sqlite))
            .map_err(erro_armazenamento)
    }

    /// Apaga do outbox as alterações gravadas antes de `antes_de` (manutenção). Devolve
    /// quantas apagou.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn podar_alteracoes(&self, antes_de: Instante) -> Resultado<usize> {
        let ctx = ContextoEscrita::novo(
            self.empresa,
            cardeal_kernel::Id::NULO,
            self.dispositivo,
            cardeal_kernel::Id::novo(),
        );
        self.arm
            .escritor()
            .executar(ctx, move |uow| {
                uow.conexao()
                    .execute(
                        "DELETE FROM nucleo_outbox WHERE criado_em < ?1",
                        [antes_de.em_micros()],
                    )
                    .map_err(sqlite)
            })
            .map(|c| c.valor)
            .map_err(erro_armazenamento)
    }
}

#[allow(clippy::needless_pass_by_value)] // usado em `map_err`
fn sqlite(e: rusqlite::Error) -> ErroArmazenamento {
    ErroArmazenamento::Sqlite(e.to_string())
}

fn como_u64(n: i64) -> u64 {
    u64::try_from(n).unwrap_or(0)
}

/// O maior `seq` já usado — de `sqlite_sequence`, que sobrevive à poda (o `MAX(seq)` da
/// tabela não sobreviveria a um outbox podado até o fim).
fn ultimo_seq(c: &rusqlite::Connection) -> rusqlite::Result<u64> {
    let n: Option<i64> = c
        .query_row(
            "SELECT seq FROM sqlite_sequence WHERE name = 'nucleo_outbox'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    Ok(n.map_or(0, como_u64))
}

fn ler(c: &rusqlite::Connection, desde: u64, limite: usize) -> rusqlite::Result<Alteracoes> {
    let ultimo = ultimo_seq(c)?;
    if desde > ultimo {
        // Adiante da base: o cliente veio de outra base (ou de antes de uma restauração).
        return Ok(Alteracoes {
            itens: Vec::new(),
            ultimo,
            lacuna: true,
        });
    }
    if desde == ultimo {
        return Ok(Alteracoes {
            itens: Vec::new(),
            ultimo,
            lacuna: false,
        });
    }
    let primeiro_retido: Option<i64> =
        c.query_row("SELECT MIN(seq) FROM nucleo_outbox", [], |r| r.get(0))?;
    // `seq` é contíguo (AUTOINCREMENT, só a poda apaga, e só do começo): há lacuna se o
    // próximo que o cliente precisa já foi apagado.
    let lacuna = primeiro_retido.is_none_or(|p| como_u64(p) > desde + 1);
    let desde_sql = i64::try_from(desde).unwrap_or(i64::MAX);
    let limite_sql = i64::try_from(limite).unwrap_or(i64::MAX);
    let mut stmt = c.prepare_cached(
        "SELECT seq, tipo FROM nucleo_outbox WHERE seq > ?1 ORDER BY seq LIMIT ?2",
    )?;
    let itens = stmt
        .query_map([desde_sql, limite_sql], |r| {
            Ok((como_u64(r.get::<_, i64>(0)?), r.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Alteracoes {
        itens,
        ultimo,
        lacuna,
    })
}
