//! Migrações de esquema — um passo por versão, imutável depois de publicado.
//!
//! `docs/06-modelo-de-dados.md` §4. Regras: migração publicada é imutável (o `hash` em
//! `nucleo_migracao` detecta alteração e o motor recusa subir); tudo roda em transação;
//! ordem é núcleo → módulos em ordem topológica.

use std::collections::{HashMap, HashSet};

use rusqlite::Connection;

use crate::erros::ErroArmazenamento;

type Resultado<T> = Result<T, ErroArmazenamento>;

/// O que uma migração muda — informativo, para a UI de progresso.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoMigracao {
    /// `CREATE TABLE`, `ALTER TABLE`.
    Esquema,
    /// Transformação de dados em massa.
    Dados,
    /// Criação de índice (adiada para o fim).
    Indice,
}

/// Um passo de migração.
#[derive(Debug, Clone, Copy)]
pub struct Migracao {
    /// A versão, monotônica dentro do módulo.
    pub versao: u32,
    /// Um nome curto e legível.
    pub nome: &'static str,
    /// O SQL, executado com `execute_batch`.
    pub sql: &'static str,
    /// O tipo.
    pub tipo: TipoMigracao,
}

/// O conjunto de migrações de um módulo, com suas dependências.
#[derive(Debug, Clone, Copy)]
pub struct ConjuntoMigracoes {
    /// O id do módulo (`"nucleo"`, `"financeiro"`…).
    pub modulo: &'static str,
    /// Módulos cujas migrações precisam rodar antes.
    pub depende_de: &'static [&'static str],
    /// Os passos, em ordem de versão.
    pub migracoes: &'static [Migracao],
}

/// O que a rodada de migração aplicou.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RelatorioMigracao {
    /// `(modulo, versao, duração em ms)` de cada migração aplicada nesta rodada.
    pub aplicadas: Vec<(String, u32, u64)>,
    /// Duração total, em ms.
    pub duracao_ms: u64,
}

impl RelatorioMigracao {
    /// Quantas migrações rodaram.
    #[must_use]
    pub fn total(&self) -> usize {
        self.aplicadas.len()
    }
}

fn hash_sql(sql: &str) -> [u8; 32] {
    *blake3::hash(sql.as_bytes()).as_bytes()
}

/// Ordena os conjuntos topologicamente por `depende_de`. `nucleo` sempre primeiro.
fn ordenar(conjuntos: &[ConjuntoMigracoes]) -> Resultado<Vec<ConjuntoMigracoes>> {
    let por_nome: HashMap<&str, ConjuntoMigracoes> =
        conjuntos.iter().map(|c| (c.modulo, *c)).collect();

    for c in conjuntos {
        for dep in c.depende_de {
            if !por_nome.contains_key(dep) {
                return Err(ErroArmazenamento::DependenciaDeMigracaoAusente {
                    modulo: c.modulo.to_string(),
                    dependencia: (*dep).to_string(),
                });
            }
        }
    }

    let mut ordem = Vec::with_capacity(conjuntos.len());
    let mut feitos: HashSet<&str> = HashSet::new();
    let mut visitando: HashSet<&str> = HashSet::new();

    // `nucleo` primeiro, se presente.
    if por_nome.contains_key("nucleo") {
        visitar("nucleo", &por_nome, &mut feitos, &mut visitando, &mut ordem)?;
    }
    let mut nomes: Vec<&str> = por_nome.keys().copied().collect();
    nomes.sort_unstable();
    for nome in nomes {
        visitar(nome, &por_nome, &mut feitos, &mut visitando, &mut ordem)?;
    }
    Ok(ordem)
}

fn visitar<'c>(
    nome: &'c str,
    por_nome: &HashMap<&'c str, ConjuntoMigracoes>,
    feitos: &mut HashSet<&'c str>,
    visitando: &mut HashSet<&'c str>,
    ordem: &mut Vec<ConjuntoMigracoes>,
) -> Resultado<()> {
    if feitos.contains(nome) {
        return Ok(());
    }
    if !visitando.insert(nome) {
        return Err(ErroArmazenamento::DependenciaDeMigracaoAusente {
            modulo: nome.to_string(),
            dependencia: "ciclo de dependências".to_string(),
        });
    }
    let c = por_nome[nome];
    for dep in c.depende_de {
        visitar(dep, por_nome, feitos, visitando, ordem)?;
    }
    visitando.remove(nome);
    feitos.insert(nome);
    ordem.push(c);
    Ok(())
}

/// Aplica os conjuntos numa transação própria — usado na abertura da base, antes de a
/// thread do escritor existir.
///
/// Verifica o hash de cada migração já aplicada; se mudou, recusa
/// (`docs/06-modelo-de-dados.md` §4, regra 1). Falhou, nada é aplicado.
pub(crate) fn aplicar(
    conn: &mut Connection,
    conjuntos: &[ConjuntoMigracoes],
) -> Resultado<RelatorioMigracao> {
    let tx = conn.transaction().map_err(ErroArmazenamento::sqlite)?;
    let relatorio = aplicar_passos(&tx, conjuntos)?;
    tx.commit().map_err(ErroArmazenamento::sqlite)?;
    Ok(relatorio)
}

/// Aplica os conjuntos na conexão dada, **sem** controlar transação — o chamador já está
/// dentro de um `SAVEPOINT` (é o caso de `Armazenamento::migrar`, que passa pelo escritor).
pub(crate) fn aplicar_passos(
    conn: &Connection,
    conjuntos: &[ConjuntoMigracoes],
) -> Resultado<RelatorioMigracao> {
    let ordem = ordenar(conjuntos)?;
    let inicio = std::time::Instant::now();
    let mut relatorio = RelatorioMigracao::default();
    let tx = conn;

    for conjunto in ordem {
        let mut passos: Vec<&Migracao> = conjunto.migracoes.iter().collect();
        // Índices depois de esquema/dados, preservando a ordem de versão dentro de cada grupo.
        passos.sort_by_key(|m| (matches!(m.tipo, TipoMigracao::Indice), m.versao));

        for m in passos {
            let hash = hash_sql(m.sql);
            let ja: Option<Vec<u8>> = tx
                .query_row(
                    "SELECT hash FROM nucleo_migracao WHERE modulo = ?1 AND versao = ?2",
                    rusqlite::params![conjunto.modulo, m.versao],
                    |row| row.get(0),
                )
                .ok();

            if let Some(hash_registrado) = ja {
                if hash_registrado != hash {
                    return Err(ErroArmazenamento::MigracaoAlterada {
                        modulo: conjunto.modulo.to_string(),
                        versao: m.versao,
                    });
                }
                continue;
            }

            let t0 = std::time::Instant::now();
            tx.execute_batch(m.sql)
                .map_err(|e| ErroArmazenamento::MigracaoFalhou {
                    modulo: conjunto.modulo.to_string(),
                    versao: m.versao,
                    nome: m.nome.to_string(),
                    causa: e.to_string(),
                })?;
            let dur = u64::try_from(t0.elapsed().as_millis()).unwrap_or(u64::MAX);

            tx.execute(
                "INSERT INTO nucleo_migracao (modulo, versao, nome, hash, aplicada_em, duracao_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    conjunto.modulo,
                    m.versao,
                    m.nome,
                    hash.as_slice(),
                    cardeal_kernel::Instante::agora().em_micros(),
                    dur,
                ],
            )
            .map_err(ErroArmazenamento::sqlite)?;

            relatorio
                .aplicadas
                .push((conjunto.modulo.to_string(), m.versao, dur));
        }
    }

    relatorio.duracao_ms = u64::try_from(inicio.elapsed().as_millis()).unwrap_or(u64::MAX);
    Ok(relatorio)
}
