//! A unidade de trabalho — a fatia de transação que um comando vê.
//!
//! Dentro do fecho passado a [`Escritor::executar`](crate::Escritor::executar) o código é
//! síncrono e simples: pega a conexão, escreve suas linhas, publica seus eventos e audita
//! suas ações — tudo no mesmo `SAVEPOINT`, que ou confirma inteiro ou desfaz inteiro
//! (`docs/07-persistencia-sqlite.md` §3).

use cardeal_kernel::{Data, Fuso, Id, Instante};
use rusqlite::{Connection, OptionalExtension};

use crate::contexto::ContextoEscrita;
use crate::erros::ErroArmazenamento;
use crate::evento::{EventoDominio, RegistroAuditoria};
use crate::sql;

type Resultado<T> = Result<T, ErroArmazenamento>;

/// Uma faixa contígua de números reservada de uma sequência — usada na abertura de caixa
/// para o modo autônomo (`docs/contratos-internos.md` §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FaixaNumeracao {
    /// O primeiro número da faixa (inclusive).
    pub inicio: u64,
    /// O último número da faixa (inclusive).
    pub fim: u64,
}

impl FaixaNumeracao {
    /// Quantos números a faixa cobre.
    #[must_use]
    pub const fn tamanho(&self) -> u64 {
        self.fim - self.inicio + 1
    }
}

/// Um recurso travado de forma pessimista. A trava tem TTL: se o dono morrer, ela solta
/// sozinha quando o prazo vence. Chamar [`Trava::soltar`] a libera antes disso.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trava {
    recurso: String,
}

impl Trava {
    /// O nome do recurso travado.
    #[must_use]
    pub fn recurso(&self) -> &str {
        &self.recurso
    }

    /// Libera a trava explicitamente dentro de uma unidade de trabalho.
    ///
    /// # Errors
    /// Propaga erro do SQLite.
    pub fn soltar(self, uow: &mut UnidadeDeTrabalho) -> Resultado<()> {
        uow.conn
            .execute(
                "DELETE FROM nucleo_trava WHERE recurso = ?1",
                [self.recurso.as_str()],
            )
            .map_err(ErroArmazenamento::sqlite)?;
        Ok(())
    }
}

/// O contexto transacional de um comando.
pub struct UnidadeDeTrabalho<'a> {
    conn: &'a Connection,
    ctx: &'a ContextoEscrita,
}

impl<'a> UnidadeDeTrabalho<'a> {
    pub(crate) fn nova(conn: &'a Connection, ctx: &'a ContextoEscrita) -> Self {
        Self { conn, ctx }
    }

    /// A conexão da transação em curso. Módulos preparam e executam suas consultas por aqui
    /// — **nunca** abrem uma conexão própria (`docs/contratos-internos.md` §6, regra 1).
    #[must_use]
    pub const fn conexao(&self) -> &Connection {
        self.conn
    }

    /// O contexto ambiente (empresa, usuário, dispositivo, sessão, instante).
    #[must_use]
    pub const fn ctx(&self) -> &ContextoEscrita {
        self.ctx
    }

    /// A empresa da unidade de trabalho.
    #[must_use]
    pub const fn empresa(&self) -> Id {
        self.ctx.empresa
    }

    /// O usuário responsável.
    #[must_use]
    pub const fn usuario(&self) -> Id {
        self.ctx.usuario
    }

    /// O dispositivo de origem.
    #[must_use]
    pub const fn dispositivo(&self) -> Id {
        self.ctx.dispositivo
    }

    /// O instante da unidade de trabalho.
    #[must_use]
    pub const fn agora(&self) -> Instante {
        self.ctx.agora
    }

    /// A data corrente, no fuso da empresa (hoje: fixo em Brasília — ver `o que falta`).
    #[must_use]
    pub fn hoje(&self) -> Data {
        self.ctx.agora.data(Fuso::BRASILIA)
    }

    /// Publica um evento de domínio: grava em `nucleo_outbox` **na mesma transação**.
    ///
    /// # Errors
    /// Falha de serialização (`postcard`) ou do SQLite.
    #[allow(clippy::needless_pass_by_value)] // o evento é entregue à outbox
    pub fn publicar<E: EventoDominio>(&mut self, evento: E) -> Resultado<()> {
        let carga = postcard::to_stdvec(&evento).map_err(|e| {
            ErroArmazenamento::Sqlite(format!("serialização do evento {}: {e}", E::TIPO))
        })?;
        self.conn
            .execute(
                "INSERT INTO nucleo_outbox (empresa, tipo, agregado, carga, criado_em)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    sql::blob(self.ctx.empresa),
                    E::TIPO,
                    sql::blob_opt(evento.agregado()),
                    carga,
                    self.ctx.agora.em_micros(),
                ],
            )
            .map_err(ErroArmazenamento::sqlite)?;
        Ok(())
    }

    /// Registra uma ação na auditoria encadeada por hash (`docs/06-modelo-de-dados.md` §2.1).
    ///
    /// # Errors
    /// Falha do SQLite.
    #[allow(clippy::needless_pass_by_value)] // o registro é consumido pela auditoria
    pub fn auditar(&mut self, r: RegistroAuditoria) -> Resultado<()> {
        let anterior: [u8; 32] = self
            .conn
            .query_row(
                "SELECT hash FROM nucleo_auditoria ORDER BY seq DESC LIMIT 1",
                [],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()
            .map_err(ErroArmazenamento::sqlite)?
            .and_then(|v| <[u8; 32]>::try_from(v).ok())
            .unwrap_or([0u8; 32]);

        let antes = r
            .antes
            .as_ref()
            .map(|j| serde_json::to_vec(j).unwrap_or_default());
        let depois = r
            .depois
            .as_ref()
            .map(|j| serde_json::to_vec(j).unwrap_or_default());

        self.conn
            .execute(
                "INSERT INTO nucleo_auditoria
                   (empresa, usuario, dispositivo, quando, acao, entidade, entidade_id,
                    antes, depois, hash_anterior, hash)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, x'')",
                rusqlite::params![
                    sql::blob(self.ctx.empresa),
                    sql::blob(self.ctx.usuario),
                    sql::blob(self.ctx.dispositivo),
                    self.ctx.agora.em_micros(),
                    r.acao,
                    r.entidade,
                    sql::blob_opt(r.entidade_id),
                    antes,
                    depois,
                    anterior.as_slice(),
                ],
            )
            .map_err(ErroArmazenamento::sqlite)?;

        let seq = self.conn.last_insert_rowid();
        let mut h = blake3::Hasher::new();
        h.update(&anterior);
        h.update(&seq.to_le_bytes());
        h.update(&self.ctx.agora.em_micros().to_le_bytes());
        h.update(&self.ctx.usuario.em_bytes());
        h.update(r.acao.as_bytes());
        if let Some(id) = r.entidade_id {
            h.update(&id.em_bytes());
        }
        if let Some(a) = &antes {
            h.update(a);
        }
        if let Some(d) = &depois {
            h.update(d);
        }
        let hash = h.finalize();

        self.conn
            .execute(
                "UPDATE nucleo_auditoria SET hash = ?1 WHERE seq = ?2",
                rusqlite::params![hash.as_bytes().as_slice(), seq],
            )
            .map_err(ErroArmazenamento::sqlite)?;
        Ok(())
    }

    /// O próximo número de uma sequência por empresa (número de lançamento, de venda…).
    /// Cada chamada consome um número.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn proximo_numero(&mut self, sequencia: &str) -> Resultado<u64> {
        let n: i64 = self
            .conn
            .query_row(
                "INSERT INTO nucleo_sequencia (empresa, nome, proximo) VALUES (?1, ?2, 1)
                 ON CONFLICT (empresa, nome) DO UPDATE SET proximo = proximo + 1
                 RETURNING proximo",
                rusqlite::params![sql::blob(self.ctx.empresa), sequencia],
                |row| row.get(0),
            )
            .map_err(ErroArmazenamento::sqlite)?;
        Ok(n.max(0).unsigned_abs())
    }

    /// Reserva uma faixa contígua de `tamanho` números de uma sequência.
    ///
    /// # Errors
    /// Falha do SQLite; `tamanho` zero devolve [`ErroArmazenamento::Sqlite`].
    pub fn reservar_faixa(&mut self, sequencia: &str, tamanho: u64) -> Resultado<FaixaNumeracao> {
        if tamanho == 0 {
            return Err(ErroArmazenamento::Sqlite("faixa de tamanho zero".into()));
        }
        let passo = i64::try_from(tamanho).unwrap_or(i64::MAX);
        let fim: i64 = self
            .conn
            .query_row(
                "INSERT INTO nucleo_sequencia (empresa, nome, proximo) VALUES (?1, ?2, ?3)
                 ON CONFLICT (empresa, nome) DO UPDATE SET proximo = proximo + ?3
                 RETURNING proximo",
                rusqlite::params![sql::blob(self.ctx.empresa), sequencia, passo],
                |row| row.get(0),
            )
            .map_err(ErroArmazenamento::sqlite)?;
        let fim = fim.max(0).unsigned_abs();
        Ok(FaixaNumeracao {
            inicio: fim - tamanho + 1,
            fim,
        })
    }

    /// Adquire uma trava pessimista sobre `recurso`, com TTL em segundos. Uma trava vencida
    /// (dono anterior morreu) ou do próprio dono é substituída.
    ///
    /// # Errors
    /// [`ErroArmazenamento::RecursoTravado`] se outra sessão detém a trava e ela não venceu.
    pub fn travar(&mut self, recurso: &str, ttl_segundos: u32) -> Resultado<Trava> {
        let agora = self.ctx.agora.em_micros();
        let expira = agora + i64::from(ttl_segundos) * 1_000_000;

        let dono_atual: Option<(Vec<u8>, i64)> = self
            .conn
            .query_row(
                "SELECT dono, expira_em FROM nucleo_trava WHERE recurso = ?1",
                [recurso],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(ErroArmazenamento::sqlite)?;

        if let Some((dono, expira_em)) = dono_atual {
            let mesma_sessao = dono == sql::blob(self.ctx.sessao);
            if !mesma_sessao && expira_em > agora {
                return Err(ErroArmazenamento::RecursoTravado {
                    recurso: recurso.to_string(),
                });
            }
        }

        self.conn
            .execute(
                "INSERT INTO nucleo_trava (recurso, dono, expira_em, motivo)
                 VALUES (?1, ?2, ?3, NULL)
                 ON CONFLICT (recurso) DO UPDATE SET dono = ?2, expira_em = ?3",
                rusqlite::params![recurso, sql::blob(self.ctx.sessao), expira],
            )
            .map_err(ErroArmazenamento::sqlite)?;

        Ok(Trava {
            recurso: recurso.to_string(),
        })
    }
}
