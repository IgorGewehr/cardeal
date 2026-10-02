//! Replicação contínua do WAL — perda máxima de dados = o último commit (ADR-0016,
//! disponibilidade).
//!
//! O escritor único sabe exatamente quando cada transação termina e é o único que faz
//! checkpoint. Isso torna a replicação física simples e exata:
//!
//! - **Após cada commit**, os bytes novos do arquivo `-wal` (quadros completos, já com
//!   `fsync` pelo próprio SQLite) são copiados para um segmento da réplica.
//! - **Os checkpoints são do replicador** (`wal_autocheckpoint = 0`; `TRUNCATE` a cada
//!   [`ConfigReplicacao::limite_wal`]): entre dois checkpoints o WAL só cresce, então cada
//!   byte é copiado exatamente uma vez, na ordem.
//!
//! ```text
//! <pasta>/geracao-atual                    o id da geração viva
//! <pasta>/<geração>/base.db                cópia exata da base no começo da geração
//! <pasta>/<geração>/<índice>/<offset>.wal  os bytes do WAL do índice, a partir do offset
//! <pasta>/<geração>/fechamento             assinatura da base num fechamento limpo
//! ```
//!
//! Um **índice** é a vida de um WAL entre dois checkpoints; restaurar é partir da `base.db` e
//! reaplicar cada índice em ordem (escrever os bytes como `-wal`, abrir, `checkpoint`). O
//! offset no nome do segmento prova a **contiguidade**: uma lacuna é recusada na restauração,
//! nunca reaplicada pela metade em silêncio.
//!
//! Nenhuma falha de replicação derruba um commit: ela vai para o log e a próxima tentativa
//! começa uma **geração nova** (cópia limpa da base) — a réplica se conserta sozinha.
//!
//! Segmentos não recebem `fsync` próprio: num crash da máquina a base continua íntegra (o
//! SQLite fez o `fsync` do WAL) e a reabertura, sem assinatura de fechamento, começa uma
//! geração nova a partir dela. A cópia para fora da máquina (R2) lê segmentos que pararam de
//! crescer — ver `packaging/servidor/compose.yaml`.

use std::fs::{self, File, OpenOptions};
use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OpenFlags};

use crate::erros::ErroArmazenamento;

type Resultado<T> = Result<T, ErroArmazenamento>;

/// Parâmetros da replicação de uma base.
#[derive(Debug, Clone)]
pub struct ConfigReplicacao {
    /// A pasta da réplica **desta base** (uma por base).
    pub pasta: PathBuf,
    /// Tamanho do WAL que dispara um checkpoint (e um índice novo). Padrão: 4 MiB.
    pub limite_wal: u64,
    /// Um segmento é fechado depois deste tempo aberto (ele para de crescer, e a cópia para
    /// fora da máquina pode levá-lo). Padrão: 2 s.
    pub idade_segmento: Duration,
    /// WAL acumulado desde a `base.db` que pede uma geração nova (restauração não fica
    /// refazendo histórico demais). O efetivo é o maior entre isto e 2× a base. Padrão: 64 MiB.
    pub wal_por_geracao: u64,
    /// Quantas gerações completas manter na pasta (a viva + as anteriores). Padrão: 2.
    pub geracoes: usize,
}

impl ConfigReplicacao {
    /// Os padrões, na pasta dada.
    #[must_use]
    pub fn em(pasta: impl Into<PathBuf>) -> Self {
        Self {
            pasta: pasta.into(),
            limite_wal: 4 * 1024 * 1024,
            idade_segmento: Duration::from_secs(2),
            wal_por_geracao: 64 * 1024 * 1024,
            geracoes: 2,
        }
    }
}

fn io(e: &std::io::Error, onde: &Path) -> ErroArmazenamento {
    ErroArmazenamento::Sqlite(format!("replicação: {}: {e}", onde.display()))
}

fn caminho_wal(db: &Path) -> PathBuf {
    let mut s = db.as_os_str().to_owned();
    s.push("-wal");
    PathBuf::from(s)
}

/// Tamanho + data de modificação: identifica a base num fechamento limpo.
fn assinatura(db: &Path) -> String {
    fs::metadata(db).map_or_else(
        |_| "-".into(),
        |m| {
            let t = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_nanos());
            format!("{}:{t}", m.len())
        },
    )
}

/// Checkpoint `TRUNCATE`. `Ok(true)` quando o WAL foi zerado; `Ok(false)` quando um leitor
/// ainda segurava o WAL (tenta de novo no próximo commit).
fn checkpoint(conn: &Connection) -> Resultado<bool> {
    let ocupado: i64 = conn
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0))
        .map_err(ErroArmazenamento::sqlite)?;
    Ok(ocupado == 0)
}

fn gravar_atomico(destino: &Path, conteudo: &[u8]) -> Resultado<()> {
    let tmp = destino.with_extension("tmp");
    let mut f = File::create(&tmp).map_err(|e| io(&e, &tmp))?;
    f.write_all(conteudo).map_err(|e| io(&e, &tmp))?;
    f.sync_all().map_err(|e| io(&e, &tmp))?;
    fs::rename(&tmp, destino).map_err(|e| io(&e, destino))
}

fn nova_id_geracao() -> String {
    let agora = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    // Ordena por tempo e não colide entre processos.
    format!(
        "{:016x}{:08x}",
        agora.as_micros(),
        cardeal_kernel::Id::novo().em_bytes()[12..16]
            .iter()
            .fold(0u32, |a, b| (a << 8) | u32::from(*b))
    )
}

struct Segmento {
    arquivo: File,
    aberto_em: Instant,
}

/// O replicador de uma base. Dentro do `Armazenamento` vive na thread do escritor; uma base
/// com conexão própria (o diretório do servidor) o usa direto: [`Replicador::abrir`] depois
/// de abrir, [`Replicador::apos_commit`] depois de cada escrita, [`Replicador::fechar`] no fim.
/// Quem o usa tem de ser o **único** escritor da base.
pub struct Replicador {
    cfg: ConfigReplicacao,
    db: PathBuf,
    wal: PathBuf,
    geracao: String,
    indice: u64,
    /// Bytes do WAL atual já copiados.
    offset: u64,
    segmento: Option<Segmento>,
    wal_desde_base: u64,
    /// Uma falha deixou a réplica incerta: a próxima chance começa uma geração nova.
    precisa_nova_geracao: bool,
}

impl Replicador {
    /// Prepara a replicação sobre a conexão do escritor: dobra qualquer WAL pendente na base,
    /// e continua a geração anterior (fechamento limpo, base intacta) ou começa uma nova.
    ///
    /// # Errors
    /// Falha de disco na pasta da réplica, ou o checkpoint inicial não zerou o WAL (outra
    /// conexão escrevendo — violação do contrato de escritor único).
    pub fn abrir(conn: &Connection, db: &Path, cfg: ConfigReplicacao) -> Resultado<Self> {
        conn.pragma_update(None, "wal_autocheckpoint", 0)
            .map_err(ErroArmazenamento::sqlite)?;
        fs::create_dir_all(&cfg.pasta).map_err(|e| io(&e, &cfg.pasta))?;
        let mut r = Self {
            wal: caminho_wal(db),
            db: db.to_path_buf(),
            cfg,
            geracao: String::new(),
            indice: 0,
            offset: 0,
            segmento: None,
            wal_desde_base: 0,
            precisa_nova_geracao: false,
        };
        if !checkpoint(conn)? {
            // Ninguém mais tem a base aberta neste momento; ocupado aqui é anomalia.
            return Err(ErroArmazenamento::Sqlite(
                "replicação: checkpoint inicial não conseguiu zerar o WAL".into(),
            ));
        }
        let anterior = fs::read_to_string(r.cfg.pasta.join("geracao-atual")).ok();
        let continua = anterior.as_ref().is_some_and(|g| {
            fs::read_to_string(r.cfg.pasta.join(g.trim()).join("fechamento"))
                .is_ok_and(|a| a == assinatura(db))
        });
        match (continua, anterior) {
            (true, Some(g)) => {
                g.trim().clone_into(&mut r.geracao);
                let pasta = r.cfg.pasta.join(&r.geracao);
                let _ = fs::remove_file(pasta.join("fechamento"));
                r.indice = indices(&pasta).last().map_or(0, |i| i + 1);
                tracing::debug!(geracao = %r.geracao, indice = r.indice, "replicação continua");
            }
            _ => r.nova_geracao()?,
        }
        Ok(r)
    }

    /// Um replicador que não conseguiu começar: tenta uma geração nova no primeiro commit.
    pub fn degradado(db: &Path, cfg: ConfigReplicacao) -> Self {
        Self {
            wal: caminho_wal(db),
            db: db.to_path_buf(),
            cfg,
            geracao: String::new(),
            indice: 0,
            offset: 0,
            segmento: None,
            wal_desde_base: 0,
            precisa_nova_geracao: true,
        }
    }

    /// Copia a base (WAL já zerado — o chamador garante) e aponta `geracao-atual` para ela.
    fn nova_geracao(&mut self) -> Resultado<()> {
        self.segmento = None;
        let id = nova_id_geracao();
        let pasta = self.cfg.pasta.join(&id);
        fs::create_dir_all(&pasta).map_err(|e| io(&e, &pasta))?;
        let tmp = pasta.join("base.db.tmp");
        fs::copy(&self.db, &tmp).map_err(|e| io(&e, &tmp))?;
        File::open(&tmp)
            .and_then(|f| f.sync_all())
            .map_err(|e| io(&e, &tmp))?;
        fs::rename(&tmp, pasta.join("base.db")).map_err(|e| io(&e, &pasta))?;
        gravar_atomico(&self.cfg.pasta.join("geracao-atual"), id.as_bytes())?;
        self.geracao = id;
        self.indice = 0;
        self.offset = 0;
        self.wal_desde_base = 0;
        self.precisa_nova_geracao = false;
        self.podar_geracoes();
        tracing::debug!(geracao = %self.geracao, db = %self.db.display(), "geração nova da réplica");
        Ok(())
    }

    fn podar_geracoes(&self) {
        let Ok(entradas) = fs::read_dir(&self.cfg.pasta) else {
            return;
        };
        let mut geracoes: Vec<String> = entradas
            .flatten()
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        geracoes.sort();
        let excedentes = geracoes.len().saturating_sub(self.cfg.geracoes.max(1));
        for velha in geracoes.into_iter().take(excedentes) {
            if velha != self.geracao {
                let _ = fs::remove_dir_all(self.cfg.pasta.join(velha));
            }
        }
    }

    /// Chamado logo depois de cada `COMMIT` bem-sucedido. Nunca falha para quem chamou: um
    /// erro vira log e uma geração nova na próxima chance.
    pub fn apos_commit(&mut self, conn: &Connection) {
        if let Err(e) = self.tentar_apos_commit(conn) {
            tracing::error!(db = %self.db.display(), erro = %e, "replicação falhou — nova geração na próxima chance");
            self.segmento = None;
            self.precisa_nova_geracao = true;
        }
    }

    fn tentar_apos_commit(&mut self, conn: &Connection) -> Resultado<()> {
        if self.precisa_nova_geracao {
            if checkpoint(conn)? {
                self.nova_geracao()?;
            }
            return Ok(());
        }
        self.copiar_novos_bytes()?;
        if self.offset >= self.cfg.limite_wal {
            self.segmento = None;
            if checkpoint(conn)? {
                self.indice += 1;
                self.offset = 0;
                let base = fs::metadata(&self.db).map_or(0, |m| m.len());
                if self.wal_desde_base >= self.cfg.wal_por_geracao.max(base.saturating_mul(2)) {
                    self.nova_geracao()?;
                }
            }
        }
        Ok(())
    }

    fn copiar_novos_bytes(&mut self) -> Resultado<()> {
        let tamanho = fs::metadata(&self.wal).map_or(0, |m| m.len());
        if tamanho < self.offset {
            // O WAL encolheu sem checkpoint nosso: alguém mexeu na base por fora.
            return Err(ErroArmazenamento::Sqlite(
                "replicação: o WAL encolheu fora do replicador".into(),
            ));
        }
        if tamanho == self.offset {
            return Ok(());
        }
        let mut wal = File::open(&self.wal).map_err(|e| io(&e, &self.wal))?;
        wal.seek(SeekFrom::Start(self.offset))
            .map_err(|e| io(&e, &self.wal))?;
        let mut novos = Vec::with_capacity(usize::try_from(tamanho - self.offset).unwrap_or(0));
        wal.take(tamanho - self.offset)
            .read_to_end(&mut novos)
            .map_err(|e| io(&e, &self.wal))?;

        let expirou = self
            .segmento
            .as_ref()
            .is_none_or(|s| s.aberto_em.elapsed() >= self.cfg.idade_segmento);
        if expirou {
            let pasta = self
                .cfg
                .pasta
                .join(&self.geracao)
                .join(format!("{:08}", self.indice));
            fs::create_dir_all(&pasta).map_err(|e| io(&e, &pasta))?;
            let caminho = pasta.join(format!("{:012}.wal", self.offset));
            let arquivo = OpenOptions::new()
                .create_new(true)
                .append(true)
                .open(&caminho)
                .map_err(|e| io(&e, &caminho))?;
            self.segmento = Some(Segmento {
                arquivo,
                aberto_em: Instant::now(),
            });
        }
        if let Some(s) = self.segmento.as_mut() {
            s.arquivo.write_all(&novos).map_err(|e| io(&e, &self.wal))?;
        }
        self.offset = tamanho;
        self.wal_desde_base += novos.len() as u64;
        Ok(())
    }

    /// Fechamento limpo: copia o resto, dobra o WAL na base e grava a assinatura que deixa a
    /// próxima abertura continuar esta geração.
    pub fn fechar(&mut self, conn: &Connection) {
        let r = (|| -> Resultado<()> {
            if self.precisa_nova_geracao {
                return Ok(());
            }
            self.copiar_novos_bytes()?;
            self.segmento = None;
            if checkpoint(conn)? {
                // O checkpoint dobrou o índice atual na base: a próxima abertura continua no
                // índice seguinte, a partir da base exatamente como ficou agora.
                gravar_atomico(
                    &self.cfg.pasta.join(&self.geracao).join("fechamento"),
                    assinatura(&self.db).as_bytes(),
                )?;
            }
            Ok(())
        })();
        if let Err(e) = r {
            tracing::error!(db = %self.db.display(), erro = %e, "replicação: fechamento sujo — geração nova na reabertura");
        }
    }
}

/// Os índices (subpastas numéricas) de uma geração, em ordem.
fn indices(pasta_geracao: &Path) -> Vec<u64> {
    let mut v: Vec<u64> = fs::read_dir(pasta_geracao)
        .map(|it| {
            it.flatten()
                .filter(|e| e.path().is_dir())
                .filter_map(|e| e.file_name().to_str()?.parse().ok())
                .collect()
        })
        .unwrap_or_default();
    v.sort_unstable();
    v
}

/// O resultado de uma restauração.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Restauracao {
    /// A geração usada.
    pub geracao: String,
    /// Quantos índices (vidas de WAL) foram reaplicados.
    pub indices: usize,
    /// Bytes de WAL reaplicados.
    pub bytes_wal: u64,
}

/// Reconstrói a base a partir da réplica em `pasta`, gravando-a em `destino` (que não pode
/// existir). Confere a contiguidade de cada índice e termina com `PRAGMA integrity_check`.
///
/// # Errors
/// Réplica sem geração, lacuna num índice, base resultante corrompida, ou `destino` já existe.
pub fn restaurar(pasta: &Path, destino: &Path) -> Resultado<Restauracao> {
    if destino.exists() {
        return Err(ErroArmazenamento::Sqlite(format!(
            "restauração: {} já existe — não sobrescrevo uma base",
            destino.display()
        )));
    }
    let geracao = fs::read_to_string(pasta.join("geracao-atual"))
        .map_err(|e| io(&e, pasta))?
        .trim()
        .to_owned();
    let pasta_geracao = pasta.join(&geracao);
    let tmp = destino.with_extension("restaurando");
    let _ = fs::remove_file(&tmp);
    let _ = fs::remove_file(caminho_wal(&tmp));
    fs::copy(pasta_geracao.join("base.db"), &tmp).map_err(|e| io(&e, &tmp))?;

    let mut bytes_wal = 0;
    let lista = indices(&pasta_geracao);
    for indice in &lista {
        let pasta_indice = pasta_geracao.join(format!("{indice:08}"));
        let mut segmentos: Vec<(u64, PathBuf)> = fs::read_dir(&pasta_indice)
            .map_err(|e| io(&e, &pasta_indice))?
            .flatten()
            .filter_map(|e| {
                let nome = e.file_name().into_string().ok()?;
                let offset = nome.strip_suffix(".wal")?.parse().ok()?;
                Some((offset, e.path()))
            })
            .collect();
        segmentos.sort_unstable_by_key(|(o, _)| *o);
        let mut wal = Vec::new();
        for (offset, caminho) in segmentos {
            if offset != wal.len() as u64 {
                return Err(ErroArmazenamento::Sqlite(format!(
                    "restauração: lacuna no índice {indice} — esperava o offset {}, veio {offset}",
                    wal.len()
                )));
            }
            let mut f = File::open(&caminho).map_err(|e| io(&e, &caminho))?;
            f.read_to_end(&mut wal).map_err(|e| io(&e, &caminho))?;
        }
        if wal.is_empty() {
            continue;
        }
        bytes_wal += wal.len() as u64;
        fs::write(caminho_wal(&tmp), &wal).map_err(|e| io(&e, &tmp))?;
        let conn = Connection::open_with_flags(&tmp, OpenFlags::SQLITE_OPEN_READ_WRITE)
            .map_err(ErroArmazenamento::sqlite)?;
        conn.pragma_update(None, "journal_mode", "wal")
            .map_err(ErroArmazenamento::sqlite)?;
        if !checkpoint(&conn)? {
            return Err(ErroArmazenamento::Sqlite(
                "restauração: checkpoint do índice não completou".into(),
            ));
        }
        drop(conn);
    }

    let conn = Connection::open(&tmp).map_err(ErroArmazenamento::sqlite)?;
    let integridade: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(ErroArmazenamento::sqlite)?;
    drop(conn);
    if integridade != "ok" {
        return Err(ErroArmazenamento::Sqlite(format!(
            "restauração: a base reconstruída não passou no integrity_check ({integridade})"
        )));
    }
    let _ = fs::remove_file(caminho_wal(&tmp));
    let mut shm = tmp.as_os_str().to_owned();
    shm.push("-shm");
    let _ = fs::remove_file(PathBuf::from(shm));
    fs::rename(&tmp, destino).map_err(|e| io(&e, destino))?;
    Ok(Restauracao {
        geracao,
        indices: lista.len(),
        bytes_wal,
    })
}
