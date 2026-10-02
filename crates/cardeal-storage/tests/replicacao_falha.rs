//! A falha da replicação é visível (para alerta) e se recupera sozinha. Arquivo próprio: o
//! estado de saúde é do processo, e os outros testes de replicação não podem interferir.

use std::fs;

use cardeal_kernel::Id;
use cardeal_storage::{
    restaurar, saude_replicacao, Armazenamento, ConfigArmazenamento, ConfigReplicacao,
    ContextoEscrita, ErroArmazenamento,
};

fn gravar(arm: &Armazenamento, sql: &'static str) {
    arm.escritor()
        .executar(
            ContextoEscrita::novo(Id::novo(), Id::novo(), Id::novo(), Id::novo()),
            move |uow| {
                uow.conexao()
                    .execute_batch(sql)
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            },
        )
        .unwrap();
}

#[test]
fn falha_da_replica_aparece_na_saude_e_some_quando_recupera() {
    let pasta = tempfile::tempdir().unwrap();
    let (db, replica) = (pasta.path().join("e.db"), pasta.path().join("replica"));
    let mut cfg = ConfigReplicacao::em(&replica);
    cfg.idade_segmento = std::time::Duration::ZERO; // um segmento novo por commit
    let arm = Armazenamento::abrir(ConfigArmazenamento::arquivo(&db).com_replicacao(cfg)).unwrap();
    gravar(
        &arm,
        "CREATE TABLE t (v INTEGER NOT NULL) STRICT; INSERT INTO t VALUES (1);",
    );
    assert_eq!(saude_replicacao().bases_em_falha, 0);

    // O disco da réplica "some": a pasta vira um arquivo.
    let geracao = fs::read_to_string(replica.join("geracao-atual")).unwrap();
    fs::rename(&replica, pasta.path().join("replica-velha")).unwrap();
    fs::write(&replica, b"nao sou pasta").unwrap();
    gravar(&arm, "INSERT INTO t VALUES (2);");
    let s = saude_replicacao();
    assert_eq!(s.bases_em_falha, 1, "{s:?} (geração {geracao})");
    assert!(s.falhas_total >= 1);
    gravar(&arm, "INSERT INTO t VALUES (3);");
    assert_eq!(
        saude_replicacao().bases_em_falha,
        1,
        "a mesma base conta uma vez"
    );

    // O disco volta: o próximo commit começa uma geração nova e o alerta some.
    fs::remove_file(&replica).unwrap();
    gravar(&arm, "INSERT INTO t VALUES (4);");
    assert_eq!(saude_replicacao().bases_em_falha, 0);
    gravar(&arm, "INSERT INTO t VALUES (5);");
    arm.fechar().unwrap();

    // E a réplica nova está completa.
    let destino = pasta.path().join("restaurada.db");
    restaurar(&replica, &destino).unwrap();
    let n: i64 = rusqlite::Connection::open(&destino)
        .unwrap()
        .query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 5);
}
