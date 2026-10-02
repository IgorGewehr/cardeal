//! Reconstruir uma pasta de dados inteira a partir das réplicas contínuas — a troca de
//! servidor (o disco morreu, a VPS sumiu) ou o ensaio de recuperação.
//!
//! ```text
//! rclone copy cifrado:replica /novo/replica     # trazer do R2 para a máquina nova
//! cardeal-server --dados /novo/dados restaurar --de /novo/replica
//! cardeal-server --dados /novo/dados servir
//! ```
//!
//! Cada base é reconstruída (base da geração + WAL de cada índice, contiguidade conferida) e
//! passa por `PRAGMA integrity_check`. Nenhuma base existente é sobrescrita. Uma falha numa
//! empresa não impede as outras — o relatório diz exatamente qual falhou.

use std::fs;
use std::path::Path;

use cardeal_kernel::{CodigoErro, Erro, Id, Resultado};
use cardeal_storage::restaurar;

use crate::config::ConfigServidor;

/// O resultado de uma restauração completa.
#[derive(Debug, Default)]
pub struct RelatorioRestauracao {
    /// Bases reconstruídas (diretório + empresas).
    pub restauradas: usize,
    /// As que falharam, com o motivo.
    pub falhas: Vec<(String, String)>,
}

/// Reconstrói em `destino` tudo o que há em `replica`. **Bloqueante.**
///
/// # Errors
/// Só se a pasta da réplica não puder ser lida ou o destino não puder ser criado; falhas de
/// uma base vão para o relatório.
pub fn restaurar_tudo(replica: &Path, destino: &ConfigServidor) -> Resultado<RelatorioRestauracao> {
    let disco = |e: std::io::Error, p: &Path| {
        Erro::novo(CodigoErro::FALHA_DE_DISCO, format!("{}: {e}", p.display()))
    };
    fs::create_dir_all(destino.pasta_empresas())
        .map_err(|e| disco(e, &destino.pasta_empresas()))?;
    let mut rel = RelatorioRestauracao::default();
    for entrada in fs::read_dir(replica)
        .map_err(|e| disco(e, replica))?
        .flatten()
    {
        let nome = entrada.file_name().to_string_lossy().into_owned();
        let alvo = if nome == "diretorio" {
            destino.caminho_diretorio()
        } else if let Ok(empresa) = nome.parse::<Id>() {
            destino.caminho_empresa(empresa)
        } else {
            continue;
        };
        match restaurar(&entrada.path(), &alvo) {
            Ok(r) => {
                tracing::info!(base = %nome, geracao = %r.geracao, indices = r.indices, "restaurada");
                rel.restauradas += 1;
            }
            Err(e) => rel.falhas.push((nome, e.to_string())),
        }
    }
    Ok(rel)
}
