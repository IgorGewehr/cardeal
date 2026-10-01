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
    /// Cache de páginas do escritor, em KiB (`PRAGMA cache_size` negativo).
    pub cache_escritor_kib: u32,
    /// Cache de páginas de **cada** leitor, em KiB.
    pub cache_leitor_kib: u32,
    /// Janela de `mmap`, em bytes. Páginas mapeadas vivem no page cache do SO — não contam
    /// como memória anônima do processo e o kernel as reclama sob pressão.
    pub mmap_bytes: u64,
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
            cache_escritor_kib: 16 * 1024,
            cache_leitor_kib: 8 * 1024,
            mmap_bytes: 256 * 1024 * 1024,
        }
    }

    /// Perfil do servidor multi-tenant (ADR-0016): **muitas** bases abertas no mesmo processo,
    /// cada uma de uma empresa pequena. Um leitor só (consultas concorrentes na mesma empresa
    /// são raras), cache de 2 MiB no escritor e 1 MiB no leitor — o grosso da leitura vem do
    /// `mmap`, que é page cache do SO, compartilhado e reclamável. Orçamento: < 4 MB de RSS por
    /// empresa aberta, contra até ~48 MB do perfil de desktop.
    #[must_use]
    pub fn servidor(caminho: impl Into<PathBuf>) -> Self {
        Self {
            leitores: 1,
            cache_escritor_kib: 2 * 1024,
            cache_leitor_kib: 1024,
            mmap_bytes: 64 * 1024 * 1024,
            ..Self::arquivo(caminho)
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
            cache_escritor_kib: 2 * 1024,
            cache_leitor_kib: 1024,
            mmap_bytes: 0,
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
