//! Testes de integração do armazenamento — base real em arquivo temporário, WAL de verdade.

use std::sync::Arc;

use cardeal_kernel::Id;
use serde::Serialize;
use tempfile::TempDir;

use crate::{
    Armazenamento, ConfigArmazenamento, ConjuntoMigracoes, ContextoEscrita, ErroArmazenamento,
    EventoDominio, Migracao, RegistroAuditoria, TipoMigracao,
};

const TESTE_SQL: &str = "CREATE TABLE t_item (
    id      BLOB PRIMARY KEY,
    empresa BLOB    NOT NULL,
    nome    TEXT    NOT NULL,
    numero  INTEGER NOT NULL DEFAULT 0
) STRICT;";

const TESTE: &[Migracao] = &[Migracao {
    versao: 1,
    nome: "cria_t_item",
    sql: TESTE_SQL,
    tipo: TipoMigracao::Esquema,
}];

fn conjunto_teste() -> ConjuntoMigracoes {
    ConjuntoMigracoes {
        modulo: "teste",
        depende_de: &["nucleo"],
        migracoes: TESTE,
    }
}

fn base() -> (TempDir, Arc<Armazenamento>) {
    let dir = tempfile::tempdir().unwrap();
    let arm =
        Armazenamento::abrir(ConfigArmazenamento::arquivo(dir.path().join("cardeal.db"))).unwrap();
    arm.migrar(&[conjunto_teste()]).unwrap();
    (dir, Arc::new(arm))
}

fn ctx(empresa: Id) -> ContextoEscrita {
    ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo())
}

fn inserir_item(arm: &Armazenamento, empresa: Id, nome: &str) -> Id {
    let nome = nome.to_string();
    arm.escritor()
        .executar(ctx(empresa), move |uow| {
            let id = Id::novo();
            let numero = uow.proximo_numero("item")?;
            uow.conexao()
                .execute(
                    "INSERT INTO t_item (id, empresa, nome, numero) VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![
                        id.em_bytes().as_slice(),
                        empresa.em_bytes().as_slice(),
                        nome,
                        numero
                    ],
                )
                .map_err(ErroArmazenamento::sqlite)?;
            Ok(id)
        })
        .unwrap()
        .valor
}

fn contar(arm: &Armazenamento) -> i64 {
    arm.leitor()
        .consultar(|c| {
            c.query_row("SELECT COUNT(*) FROM t_item", [], |r| r.get(0))
                .map_err(ErroArmazenamento::sqlite)
        })
        .unwrap()
}

fn numero_do_item(arm: &Armazenamento, id: Id) -> i64 {
    arm.leitor()
        .consultar(|c| {
            c.query_row(
                "SELECT numero FROM t_item WHERE id = ?1",
                [id.em_bytes().as_slice()],
                |r| r.get(0),
            )
            .map_err(ErroArmazenamento::sqlite)
        })
        .unwrap()
}

#[test]
fn abre_migra_e_le_o_que_escreveu() {
    let (_dir, arm) = base();
    let empresa = Id::novo();
    let id = inserir_item(&arm, empresa, "Refrigerante");
    assert_eq!(contar(&arm), 1);

    let lido: String = arm
        .leitor()
        .consultar(|c| {
            c.query_row(
                "SELECT nome FROM t_item WHERE id = ?1",
                [id.em_bytes().as_slice()],
                |r| r.get(0),
            )
            .map_err(ErroArmazenamento::sqlite)
        })
        .unwrap();
    assert_eq!(lido, "Refrigerante");
    assert!(arm.versao_atual().numero() >= 1);
}

#[test]
fn proximo_numero_e_sequencial_por_empresa() {
    let (_dir, arm) = base();
    let a = Id::novo();
    let b = Id::novo();
    inserir_item(&arm, a, "x");
    inserir_item(&arm, a, "y");
    let terceiro = inserir_item(&arm, a, "z");
    let primeiro_de_b = inserir_item(&arm, b, "w");

    assert_eq!(numero_do_item(&arm, terceiro), 3);
    assert_eq!(numero_do_item(&arm, primeiro_de_b), 1);
}

#[derive(Serialize)]
struct ItemCriado {
    item: Id,
    nome: String,
}
impl EventoDominio for ItemCriado {
    const TIPO: &'static str = "teste.item_criado.v1";
    fn agregado(&self) -> Option<Id> {
        Some(self.item)
    }
}

#[test]
fn evento_publicado_vai_para_o_outbox_na_mesma_transacao() {
    let (_dir, arm) = base();
    let empresa = Id::novo();
    let item = Id::novo();
    arm.escritor()
        .executar(ctx(empresa), move |uow| {
            uow.conexao()
                .execute(
                    "INSERT INTO t_item (id, empresa, nome) VALUES (?1, ?2, 'Cafe')",
                    rusqlite::params![item.em_bytes().as_slice(), empresa.em_bytes().as_slice()],
                )
                .map_err(ErroArmazenamento::sqlite)?;
            uow.publicar(ItemCriado {
                item,
                nome: "Cafe".into(),
            })?;
            Ok(())
        })
        .unwrap();

    let (tipo, n): (String, i64) = arm
        .leitor()
        .consultar(|c| {
            c.query_row(
                "SELECT tipo, COUNT(*) FROM nucleo_outbox GROUP BY tipo",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(ErroArmazenamento::sqlite)
        })
        .unwrap();
    assert_eq!(tipo, "teste.item_criado.v1");
    assert_eq!(n, 1);
}

#[test]
fn auditoria_encadeia_o_hash() {
    let (_dir, arm) = base();
    let empresa = Id::novo();
    let alvo = Id::novo();
    for acao in ["criar", "editar", "cancelar"] {
        arm.escritor()
            .executar(ctx(empresa), move |uow| {
                uow.auditar(RegistroAuditoria::nova(
                    format!("teste.{acao}"),
                    "t_item",
                    alvo,
                ))
            })
            .unwrap();
    }

    let cadeia_ok: bool = arm
        .leitor()
        .consultar(|c| {
            let mut stmt = c
                .prepare("SELECT hash_anterior, hash FROM nucleo_auditoria ORDER BY seq")
                .map_err(ErroArmazenamento::sqlite)?;
            let linhas: Vec<(Vec<u8>, Vec<u8>)> = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(ErroArmazenamento::sqlite)?
                .collect::<Result<_, _>>()
                .map_err(ErroArmazenamento::sqlite)?;
            let mut anterior = vec![0u8; 32];
            for (ha, h) in &linhas {
                if *ha != anterior {
                    return Ok(false);
                }
                anterior.clone_from(h);
            }
            Ok(linhas.len() == 3)
        })
        .unwrap();
    assert!(
        cadeia_ok,
        "a cadeia de auditoria deve encadear hash_anterior → hash"
    );
}

#[test]
fn tarefa_que_falha_ou_entra_em_panico_nao_derruba_o_lote() {
    let (_dir, arm) = base();
    let empresa = Id::novo();

    let mut handles = Vec::new();
    for i in 0..24 {
        let arm = Arc::clone(&arm);
        handles.push(std::thread::spawn(move || {
            arm.escritor()
                .executar(ctx(empresa), move |uow| {
                    uow.conexao()
                        .execute(
                            "INSERT INTO t_item (id, empresa, nome) VALUES (?1, ?2, ?3)",
                            rusqlite::params![
                                Id::novo().em_bytes().as_slice(),
                                empresa.em_bytes().as_slice(),
                                format!("linha {i}")
                            ],
                        )
                        .map_err(ErroArmazenamento::sqlite)?;
                    match i % 3 {
                        0 => Ok(()),
                        1 => Err(ErroArmazenamento::VersaoDesatualizada),
                        _ => panic!("panico proposital na tarefa {i}"),
                    }
                })
                .is_ok()
        }));
    }
    let sucessos = handles
        .into_iter()
        .map(|h| h.join().unwrap_or(false))
        .filter(|ok| *ok)
        .count();
    assert_eq!(sucessos, 8, "só os i % 3 == 0 devem confirmar");
    assert_eq!(
        contar(&arm),
        8,
        "falha e pânico devem ser desfeitos no savepoint"
    );

    inserir_item(&arm, empresa, "depois");
    assert_eq!(contar(&arm), 9, "o escritor segue vivo depois dos pânicos");
}

const TESTE_ALTERADO: &[Migracao] = &[Migracao {
    versao: 1,
    nome: "cria_t_item",
    sql: "CREATE TABLE t_item (id BLOB PRIMARY KEY) STRICT;", // SQL diferente, mesma versão
    tipo: TipoMigracao::Esquema,
}];

const ORFA: &[Migracao] = &[Migracao {
    versao: 1,
    nome: "x",
    sql: "CREATE TABLE t_orfa (id BLOB PRIMARY KEY) STRICT;",
    tipo: TipoMigracao::Esquema,
}];

#[test]
fn migracao_alterada_depois_de_publicada_e_recusada() {
    let (_dir, arm) = base();
    let conjunto = ConjuntoMigracoes {
        modulo: "teste",
        depende_de: &["nucleo"],
        migracoes: TESTE_ALTERADO,
    };
    assert!(matches!(
        arm.migrar(&[conjunto]).unwrap_err(),
        ErroArmazenamento::MigracaoAlterada { .. }
    ));
}

#[test]
fn dependencia_de_migracao_ausente_e_erro() {
    let (_dir, arm) = base();
    let conjunto = ConjuntoMigracoes {
        modulo: "orfa",
        depende_de: &["modulo_inexistente"],
        migracoes: ORFA,
    };
    assert!(matches!(
        arm.migrar(&[conjunto]).unwrap_err(),
        ErroArmazenamento::DependenciaDeMigracaoAusente { .. }
    ));
}

#[test]
fn trava_pessimista_barra_outra_sessao() {
    let (_dir, arm) = base();
    let empresa = Id::novo();
    let sessao_a = ctx(empresa);
    let sessao_b = ctx(empresa);
    let recurso = "caixa:1";

    arm.escritor()
        .executar(sessao_a, move |uow| uow.travar(recurso, 60).map(|_| ()))
        .unwrap();

    let barrado = arm
        .escritor()
        .executar(sessao_b, move |uow| uow.travar(recurso, 60).map(|_| ()));
    assert!(matches!(
        barrado,
        Err(ErroArmazenamento::RecursoTravado { .. })
    ));

    // A mesma sessão A retoma a trava.
    let ok = arm
        .escritor()
        .executar(sessao_a, move |uow| uow.travar(recurso, 60).map(|_| ()));
    assert!(ok.is_ok());
}
