//! Abertura e configuração de conexões — pragmas do `docs/07-persistencia-sqlite.md` §2.

use rusqlite::{Connection, OpenFlags};

use crate::config::ConfigArmazenamento;
use crate::erros::ErroArmazenamento;

type Resultado<T> = Result<T, ErroArmazenamento>;

/// A string de conexão efetiva: o caminho do arquivo, ou uma URI de base em memória com
/// cache compartilhado (para os testes verem as escritas entre conexões).
#[derive(Debug, Clone)]
pub(crate) struct Alvo {
    uri: String,
    memoria: bool,
}

impl Alvo {
    pub(crate) fn de(cfg: &ConfigArmazenamento) -> Self {
        if cfg.e_memoria() {
            let id = cardeal_kernel::Id::novo();
            Self {
                uri: format!("file:cardeal-mem-{}?mode=memory&cache=shared", id.curto()),
                memoria: true,
            }
        } else {
            Self {
                uri: cfg.caminho.to_string_lossy().into_owned(),
                memoria: false,
            }
        }
    }
}

fn abrir(alvo: &Alvo, flags: OpenFlags) -> Resultado<Connection> {
    let flags = if alvo.memoria {
        flags | OpenFlags::SQLITE_OPEN_URI
    } else {
        flags
    };
    Connection::open_with_flags(&alvo.uri, flags)
        .map_err(|e| ErroArmazenamento::Abertura(e.to_string()))
}

/// Abre e configura a conexão do escritor único. `synchronous = FULL` é inegociável.
pub(crate) fn abrir_escritor(alvo: &Alvo) -> Resultado<Connection> {
    let c = abrir(
        alvo,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
    )?;
    let pragmas: &[(&str, &str)] = if alvo.memoria {
        &[("foreign_keys", "ON"), ("busy_timeout", "5000")]
    } else {
        &[
            ("journal_mode", "wal"),
            ("synchronous", "FULL"),
            ("foreign_keys", "ON"),
            ("busy_timeout", "5000"),
            ("cache_size", "-16384"),
            ("mmap_size", "268435456"),
            ("wal_autocheckpoint", "4000"),
            ("journal_size_limit", "67108864"),
            ("temp_store", "MEMORY"),
            ("trusted_schema", "OFF"),
            ("auto_vacuum", "INCREMENTAL"),
        ]
    };
    aplicar_pragmas(&c, pragmas)?;
    Ok(c)
}

/// Abre e configura uma conexão de leitura (`query_only`).
pub(crate) fn abrir_leitor(alvo: &Alvo) -> Resultado<Connection> {
    let c = abrir(alvo, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    let pragmas: &[(&str, &str)] = if alvo.memoria {
        &[("foreign_keys", "ON"), ("query_only", "ON")]
    } else {
        &[
            ("journal_mode", "wal"),
            ("synchronous", "NORMAL"),
            ("query_only", "ON"),
            ("foreign_keys", "ON"),
            ("cache_size", "-8192"),
            ("mmap_size", "268435456"),
        ]
    };
    aplicar_pragmas(&c, pragmas)?;
    Ok(c)
}

fn aplicar_pragmas(c: &Connection, pragmas: &[(&str, &str)]) -> Resultado<()> {
    for (chave, valor) in pragmas {
        c.pragma_update(None, chave, valor)
            .map_err(|e| ErroArmazenamento::Abertura(format!("pragma {chave}={valor}: {e}")))?;
    }
    Ok(())
}
