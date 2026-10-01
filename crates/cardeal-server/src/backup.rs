//! Backup: um snapshot consistente de cada base (empresas + diretório), comprimido, com
//! retenção — pronto para um `rclone sync` levar ao Cloudflare R2 (ver
//! `packaging/servidor/compose.yaml`).
//!
//! - **Consistente com a base em uso.** `VACUUM INTO` numa conexão só de leitura: o snapshot
//!   é uma transação de leitura do WAL — a empresa continua vendendo enquanto ele é tirado,
//!   e o arquivo sai compacto (sem páginas livres), pronto para abrir.
//! - **Só o que mudou.** A assinatura (tamanho + data de modificação da base e do `-wal`) da
//!   última cópia fica ao lado dela; base sem escrita desde então é pulada — com centenas de
//!   empresas pequenas, a maioria dos ciclos não copia quase nada.
//! - **zstd** (nível 9): uma base de ERP comprime ~5–10×.
//! - **Retenção**: as `reter` cópias mais novas de cada base; as mais velhas são apagadas.
//!
//! Restaurar: parar o servidor (ou esperar a empresa sair da memória), `zstd -d` a cópia para
//! `empresas/<id>.db` e apagar `-wal`/`-shm` velhos.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use cardeal_kernel::{CodigoErro, Erro, Resultado};
use rusqlite::{Connection, OpenFlags};

use crate::config::ConfigServidor;

/// O resultado de uma rodada.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RelatorioBackup {
    /// Bases copiadas nesta rodada.
    pub copiadas: usize,
    /// Bases puladas por não terem mudado desde a última cópia.
    pub sem_mudanca: usize,
    /// Bases que falharam (o erro vai para o log; as outras seguem).
    pub falhas: usize,
    /// Bytes gravados (já comprimidos).
    pub bytes: u64,
}

fn disco(e: &std::io::Error, onde: &Path) -> Erro {
    Erro::novo(
        CodigoErro::FALHA_DE_DISCO,
        format!("{}: {e}", onde.display()),
    )
}

/// Faz uma rodada de backup de todas as bases. **Bloqueante.**
///
/// # Errors
/// Só se a pasta de backup não puder ser criada; falhas de uma base não param as outras.
pub fn executar(config: &ConfigServidor, reter: usize) -> Resultado<RelatorioBackup> {
    let destino = config.pasta_backup();
    fs::create_dir_all(&destino).map_err(|e| disco(&e, &destino))?;

    let mut bases = vec![("diretorio".to_owned(), config.caminho_diretorio())];
    if let Ok(entradas) = fs::read_dir(config.pasta_empresas()) {
        for e in entradas.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "db") {
                if let Some(nome) = p.file_stem().and_then(|s| s.to_str()) {
                    bases.push((nome.to_owned(), p.clone()));
                }
            }
        }
    }

    let mut rel = RelatorioBackup::default();
    for (nome, base) in bases {
        match copiar(&base, &destino.join(&nome), reter) {
            Ok(Some(bytes)) => {
                rel.copiadas += 1;
                rel.bytes += bytes;
            }
            Ok(None) => rel.sem_mudanca += 1,
            Err(e) => {
                rel.falhas += 1;
                tracing::error!(base = %base.display(), erro = %e.mensagem, "backup falhou");
            }
        }
    }
    if rel.copiadas > 0 || rel.falhas > 0 {
        tracing::info!(
            copiadas = rel.copiadas,
            sem_mudanca = rel.sem_mudanca,
            falhas = rel.falhas,
            kib = rel.bytes / 1024,
            "backup"
        );
    }
    Ok(rel)
}

/// Assinatura de mudança: tamanho + mtime da base e do `-wal`.
fn assinatura(base: &Path) -> String {
    let parte = |p: &Path| {
        fs::metadata(p).map_or_else(
            |_| "-".to_owned(),
            |m| {
                let t = m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_nanos());
                format!("{}:{t}", m.len())
            },
        )
    };
    let mut wal = base.as_os_str().to_owned();
    wal.push("-wal");
    let wal = Path::new(&wal);
    // Abrir uma base WAL fechada — até só para ler, como o próprio snapshot faz — cria um
    // `-wal` vazio: ausente e vazio são o mesmo estado, senão toda base fechada pareceria
    // "mudada" na rodada seguinte. A assinatura é sempre tirada ANTES do snapshot: uma
    // escrita que chegue durante a cópia muda o `-wal` e entra na próxima rodada.
    let parte_wal = if fs::metadata(wal).map_or(true, |m| m.len() == 0) {
        "-".to_owned()
    } else {
        parte(wal)
    };
    format!("{}|{parte_wal}", parte(base))
}

/// Copia uma base se ela mudou. `Some(bytes)` quando copiou.
fn copiar(base: &Path, pasta: &Path, reter: usize) -> Resultado<Option<u64>> {
    fs::create_dir_all(pasta).map_err(|e| disco(&e, pasta))?;
    let marcador = pasta.join("ultima-assinatura");
    let atual = assinatura(base);
    if fs::read_to_string(&marcador).is_ok_and(|a| a == atual) {
        return Ok(None);
    }

    let carimbo = carimbo_utc();
    let cru = pasta.join(format!(".{carimbo}.db.tmp"));
    let final_ = pasta.join(format!("{carimbo}.db.zst"));
    let _ = fs::remove_file(&cru);

    let conn = Connection::open_with_flags(
        base,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| Erro::novo(CodigoErro::BANCO_INDISPONIVEL, e.to_string()))?;
    conn.execute("VACUUM INTO ?1", [cru.to_string_lossy()])
        .map_err(|e| Erro::novo(CodigoErro::BANCO_INDISPONIVEL, format!("VACUUM INTO: {e}")))?;
    drop(conn);

    let bytes = comprimir(&cru, &final_)?;
    let _ = fs::remove_file(&cru);
    fs::write(&marcador, atual).map_err(|e| disco(&e, &marcador))?;
    aplicar_retencao(pasta, reter);
    Ok(Some(bytes))
}

fn comprimir(origem: &Path, destino: &Path) -> Resultado<u64> {
    let tmp: PathBuf = destino.with_extension("zst.tmp");
    let mut entrada = fs::File::open(origem).map_err(|e| disco(&e, origem))?;
    let saida = fs::File::create(&tmp).map_err(|e| disco(&e, &tmp))?;
    let mut z = zstd::Encoder::new(saida, 9).map_err(|e| disco(&e, &tmp))?;
    std::io::copy(&mut entrada, &mut z).map_err(|e| disco(&e, &tmp))?;
    let mut saida = z.finish().map_err(|e| disco(&e, &tmp))?;
    saida.flush().map_err(|e| disco(&e, &tmp))?;
    saida.sync_all().map_err(|e| disco(&e, &tmp))?;
    // Renomear por último: um arquivo `.db.zst` nunca existe pela metade.
    fs::rename(&tmp, destino).map_err(|e| disco(&e, destino))?;
    Ok(fs::metadata(destino).map_or(0, |m| m.len()))
}

fn aplicar_retencao(pasta: &Path, reter: usize) {
    let Ok(entradas) = fs::read_dir(pasta) else {
        return;
    };
    let mut copias: Vec<PathBuf> = entradas
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(".db.zst"))
        .collect();
    // O carimbo UTC no nome ordena cronologicamente.
    copias.sort();
    let excedentes = copias.len().saturating_sub(reter.max(1));
    for velha in copias.into_iter().take(excedentes) {
        let _ = fs::remove_file(velha);
    }
}

/// `AAAAMMDDTHHMMSSmmmZ` — ordena como texto e não colide em rodadas no mesmo segundo.
fn carimbo_utc() -> String {
    let agora = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let segs = i64::try_from(agora.as_secs()).unwrap_or(0);
    let (dias, resto) = (segs.div_euclid(86_400), segs.rem_euclid(86_400));
    let (a, m, d) = civil_de_dias(dias);
    format!(
        "{a:04}{m:02}{d:02}T{:02}{:02}{:02}{:03}Z",
        resto / 3600,
        (resto % 3600) / 60,
        resto % 60,
        agora.subsec_millis()
    )
}

/// Dias desde 1970-01-01 → (ano, mês, dia), pelo algoritmo de Howard Hinnant.
fn civil_de_dias(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let a = yoe + era * 400 + i64::from(m <= 2);
    (a, m, d)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn calendario_de_dias() {
        assert_eq!(civil_de_dias(0), (1970, 1, 1));
        assert_eq!(civil_de_dias(19_723), (2024, 1, 1));
        assert_eq!(civil_de_dias(20_727), (2026, 10, 1));
    }

    #[test]
    fn retencao_fica_com_as_mais_novas() {
        let p = tempfile::tempdir().unwrap();
        for n in [
            "20260101T000000000Z",
            "20260102T000000000Z",
            "20260103T000000000Z",
        ] {
            fs::write(p.path().join(format!("{n}.db.zst")), b"x").unwrap();
        }
        aplicar_retencao(p.path(), 2);
        let mut sobrou: Vec<_> = fs::read_dir(p.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        sobrou.sort();
        assert_eq!(
            sobrou,
            ["20260102T000000000Z.db.zst", "20260103T000000000Z.db.zst"]
        );
    }
}
