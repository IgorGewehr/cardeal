//! Configuração de abertura da base.

use std::path::PathBuf;

use crate::segredo::Segredo;

/// Parâmetros de abertura de um [`Armazenamento`](crate::Armazenamento).
/// Ver `docs/07-persistencia-sqlite.md` §2 e §5.
#[derive(Debug)]
pub struct ConfigArmazenamento {
    /// Caminho do arquivo `.db`. Use [`Self::memoria`] para testes.
    pub caminho: PathBuf,
    /// Abre sem escritor (só leitura). Nenhuma migração é aplicada.
    pub somente_leitura: bool,
    /// Tamanho do pool de conexões de leitura (padrão: `min(4, paralelismo)`).
    pub leitores: usize,
    /// Janela de coalescência do group commit, em milissegundos (padrão: 2).
    pub janela_lote_ms: u64,
    /// Máximo de tarefas por lote do group commit (padrão: 64).
    pub maximo_lote: usize,
    /// Chave de criptografia (`SQLCipher`), quando a base é cifrada — hoje reservada.
    pub chave_cripto: Option<Segredo<String>>,
}

impl ConfigArmazenamento {
    /// Configuração para um arquivo em disco, com os padrões do doc 07.
    #[must_use]
    pub fn arquivo(caminho: impl Into<PathBuf>) -> Self {
        Self {
            caminho: caminho.into(),
            somente_leitura: false,
            leitores: leitores_padrao(),
            janela_lote_ms: 2,
            maximo_lote: 64,
            chave_cripto: None,
        }
    }

    /// Configuração para uma base em memória (`:memory:`), para testes. Um único leitor,
    /// porque `:memory:` não é compartilhado entre conexões.
    #[must_use]
    pub fn memoria() -> Self {
        Self {
            caminho: PathBuf::from(":memory:"),
            somente_leitura: false,
            leitores: 1,
            janela_lote_ms: 1,
            maximo_lote: 16,
            chave_cripto: None,
        }
    }

    /// Verdadeiro se a base é a `:memory:`.
    #[must_use]
    pub fn e_memoria(&self) -> bool {
        self.caminho.as_os_str() == ":memory:"
    }
}

fn leitores_padrao() -> usize {
    std::thread::available_parallelism()
        .map_or(2, std::num::NonZeroUsize::get)
        .min(4)
}
