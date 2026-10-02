//! A replicação contínua do WAL contra bases reais: a réplica restaura idêntica — com vários
//! checkpoints no meio, entre aberturas, depois de fechamento sujo — e uma lacuna é recusada.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use cardeal_kernel::Id;
use cardeal_storage::{
    restaurar, Armazenamento, ConfigArmazenamento, ConfigReplicacao, ContextoEscrita,
    ErroArmazenamento,
};
use rusqlite::Connection;

/// Limite de WAL pequeno: força muitos checkpoints (índices) num teste curto.
fn cfg_replicacao(pasta: &Path) -> ConfigReplicacao {
    let mut c = ConfigReplicacao::em(pasta);
    c.limite_wal = 32 * 1024;
    c.idade_segmento = Duration::from_millis(1);
    c
}

fn abrir(db: &Path, replica: &Path) -> Armazenamento {
    let arm = Armazenamento::abrir(
        ConfigArmazenamento::arquivo(db).com_replicacao(cfg_replicacao(replica)),
    )
    .unwrap();
    arm.escritor()
        .executar(ctx(), |uow| {
            uow.conexao()
                .execute_batch(
                    "CREATE TABLE IF NOT EXISTS item (id INTEGER PRIMARY KEY, nome TEXT NOT NULL, valor INTEGER NOT NULL) STRICT;",
                )
                .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
        })
        .unwrap();
    arm
}

fn ctx() -> ContextoEscrita {
    ContextoEscrita::novo(Id::novo(), Id::novo(), Id::novo(), Id::novo())
}

fn inserir(arm: &Armazenamento, de: i64, ate: i64) {
    for i in de..ate {
        arm.escritor()
            .executar(ctx(), move |uow| {
                uow.conexao()
                    .execute(
                        "INSERT INTO item (id, nome, valor) VALUES (?1, ?2, ?3)",
                        rusqlite::params![i, format!("item {i} {}", "x".repeat(200)), i * 7],
                    )
                    .map(|_| ())
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            })
            .unwrap();
    }
}

/// (quantidade, soma dos valores) — a "impressão" do conteúdo.
fn conteudo(db: &Path) -> (i64, i64) {
    let c = Connection::open(db).unwrap();
    c.query_row(
        "SELECT COUNT(*), COALESCE(SUM(valor), 0) FROM item",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .unwrap()
}

fn geracao(replica: &Path) -> String {
    fs::read_to_string(replica.join("geracao-atual")).unwrap()
}

fn indices(replica: &Path) -> usize {
    fs::read_dir(replica.join(geracao(replica)))
        .unwrap()
        .flatten()
        .filter(|e| e.path().is_dir())
        .count()
}

struct Cenario {
    _pasta: tempfile::TempDir,
    db: PathBuf,
    replica: PathBuf,
    destino: PathBuf,
}

fn cenario() -> Cenario {
    let pasta = tempfile::tempdir().unwrap();
    Cenario {
        db: pasta.path().join("empresa.db"),
        replica: pasta.path().join("replica"),
        destino: pasta.path().join("restaurada.db"),
        _pasta: pasta,
    }
}

#[test]
fn restaura_identico_atravessando_varios_checkpoints() {
    let c = cenario();
    let arm = abrir(&c.db, &c.replica);
    inserir(&arm, 0, 600);
    arm.fechar().unwrap();

    assert!(
        indices(&c.replica) > 3,
        "o limite pequeno tem de ter gerado vários índices"
    );
    let r = restaurar(&c.replica, &c.destino).unwrap();
    assert!(r.bytes_wal > 0);
    assert_eq!(conteudo(&c.destino), conteudo(&c.db));
    assert_eq!(conteudo(&c.destino).0, 600);
}

#[test]
fn todo_commit_confirmado_ja_esta_na_replica_mesmo_com_a_base_aberta() {
    let c = cenario();
    let arm = abrir(&c.db, &c.replica);
    inserir(&arm, 0, 250);
    // Sem fechar: a réplica tem de cobrir tudo o que já foi confirmado a quem chamou.
    restaurar(&c.replica, &c.destino).unwrap();
    assert_eq!(conteudo(&c.destino).0, 250);
    drop(arm);
}

#[test]
fn fechamento_limpo_continua_a_mesma_geracao_sem_recopiar_a_base() {
    let c = cenario();
    let arm = abrir(&c.db, &c.replica);
    inserir(&arm, 0, 100);
    arm.fechar().unwrap();
    let g1 = geracao(&c.replica);

    let arm = abrir(&c.db, &c.replica);
    inserir(&arm, 100, 300);
    arm.fechar().unwrap();
    assert_eq!(
        geracao(&c.replica),
        g1,
        "reabrir depois de fechar limpo continua a geração"
    );

    restaurar(&c.replica, &c.destino).unwrap();
    assert_eq!(conteudo(&c.destino), conteudo(&c.db));
}

#[test]
fn sem_fechamento_limpo_comeca_geracao_nova_e_ainda_restaura_tudo() {
    let c = cenario();
    let arm = abrir(&c.db, &c.replica);
    inserir(&arm, 0, 120);
    arm.fechar().unwrap();
    let g1 = geracao(&c.replica);
    // Simula um crash: nenhuma assinatura de fechamento.
    fs::remove_file(c.replica.join(&g1).join("fechamento")).unwrap();

    let arm = abrir(&c.db, &c.replica);
    inserir(&arm, 120, 200);
    arm.fechar().unwrap();
    assert_ne!(
        geracao(&c.replica),
        g1,
        "sem prova de fechamento limpo, geração nova"
    );

    restaurar(&c.replica, &c.destino).unwrap();
    assert_eq!(conteudo(&c.destino), conteudo(&c.db));
    assert_eq!(conteudo(&c.destino).0, 200);
}

#[test]
fn lacuna_num_indice_e_recusada_e_destino_existente_nao_e_sobrescrito() {
    let c = cenario();
    let arm = abrir(&c.db, &c.replica);
    inserir(&arm, 0, 200);
    arm.fechar().unwrap();

    let pasta_indice = c.replica.join(geracao(&c.replica)).join("00000000");
    let mut segmentos: Vec<_> = fs::read_dir(&pasta_indice)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    segmentos.sort();
    assert!(
        segmentos.len() >= 3,
        "precisa de segmentos no meio para tirar um"
    );
    fs::remove_file(&segmentos[1]).unwrap();

    let erro = restaurar(&c.replica, &c.destino).unwrap_err();
    assert!(erro.to_string().contains("lacuna"), "{erro}");
    assert!(!c.destino.exists(), "nada pela metade no destino");

    let erro = restaurar(&c.replica, &c.db).unwrap_err();
    assert!(erro.to_string().contains("já existe"), "{erro}");
}

#[test]
fn leitores_constantes_durante_checkpoints_nao_quebram_a_replica() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let c = cenario();
    let arm = Arc::new(abrir(&c.db, &c.replica));
    let parar = Arc::new(AtomicBool::new(false));
    let leitores: Vec<_> = (0..3)
        .map(|_| {
            let arm = Arc::clone(&arm);
            let parar = Arc::clone(&parar);
            std::thread::spawn(move || {
                let mut leituras = 0u64;
                while !parar.load(Ordering::Relaxed) {
                    arm.leitor()
                        .consultar(|c| {
                            c.query_row("SELECT COUNT(*) FROM item", [], |r| r.get::<_, i64>(0))
                                .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
                        })
                        .unwrap();
                    leituras += 1;
                }
                leituras
            })
        })
        .collect();

    inserir(&arm, 0, 500);
    parar.store(true, Ordering::Relaxed);
    let leituras: u64 = leitores.into_iter().map(|t| t.join().unwrap()).sum();
    assert!(leituras > 0);
    Arc::try_unwrap(arm).ok().unwrap().fechar().unwrap();

    restaurar(&c.replica, &c.destino).unwrap();
    assert_eq!(conteudo(&c.destino), conteudo(&c.db));
    assert_eq!(conteudo(&c.destino).0, 500);
}

/// Custo da replicação no caminho de escrita (`cargo test --release -- --ignored --nocapture`).
#[test]
#[ignore = "medição, não asserção"]
fn medir_custo_da_replicacao_por_commit() {
    for replicar in [false, true] {
        let c = cenario();
        let cfg = ConfigArmazenamento::arquivo(&c.db);
        let cfg = if replicar {
            cfg.com_replicacao(ConfigReplicacao::em(&c.replica))
        } else {
            cfg
        };
        let arm = Armazenamento::abrir(cfg).unwrap();
        arm.escritor()
            .executar(ctx(), |uow| {
                uow.conexao()
                    .execute_batch("CREATE TABLE item (id INTEGER PRIMARY KEY, nome TEXT NOT NULL, valor INTEGER NOT NULL) STRICT;")
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            })
            .unwrap();
        let inicio = std::time::Instant::now();
        inserir(&arm, 0, 2000);
        let por_commit = inicio.elapsed() / 2000;
        arm.fechar().unwrap();
        println!(
            "replicação {}: {:?} por commit",
            if replicar { "ligada   " } else { "desligada" },
            por_commit
        );
    }
}
