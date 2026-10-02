//! O diretório global: quem são as contas, quais empresas existem, quem entra em qual, e as
//! sessões abertas. É a única base compartilhada entre empresas — e não guarda nada de negócio.
//!
//! Uma conexão só, atrás de um `Mutex`: o diretório é tocado no login, no logout e numa falta
//! do cache de sessões ([`crate::sessoes`]) — nunca no caminho quente de um comando.

mod esquema;

use std::path::Path;

use cardeal_auth::{Bloqueio, HashDeSenha};
use cardeal_kernel::{CodigoErro, Erro, Id, Instante, Resultado};
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension};

use crate::token::HashToken;

/// Uma conta: a pessoa que faz login (e-mail + senha).
#[derive(Debug, Clone)]
pub struct Conta {
    /// Identidade.
    pub id: Id,
    /// E-mail normalizado (minúsculo, sem espaço nas bordas).
    pub email: String,
    /// Nome exibido.
    pub nome: String,
    /// Hash Argon2id.
    pub senha: HashDeSenha,
    /// Tentativas e bloqueio progressivo.
    pub bloqueio: Bloqueio,
}

/// Uma empresa em que uma conta entra, e como quem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vinculo {
    /// A empresa.
    pub empresa: Id,
    /// Nome da empresa, para o seletor.
    pub nome: String,
    /// O usuário da conta **dentro** da base da empresa.
    pub usuario: Id,
}

/// Uma sessão persistida.
#[derive(Debug, Clone, Copy)]
pub struct SessaoPersistida {
    /// Identidade da sessão — é o "dispositivo" nas auditorias da empresa.
    pub id: Id,
    /// A conta dona.
    pub conta: Id,
    /// Quando expira.
    pub expira_em: Instante,
}

/// O diretório aberto.
pub struct Diretorio {
    conn: Mutex<Connection>,
}

fn falha(e: &rusqlite::Error) -> Erro {
    Erro::novo(CodigoErro::BANCO_INDISPONIVEL, format!("diretório: {e}"))
}

fn id_de(bytes: &[u8]) -> rusqlite::Result<Id> {
    <[u8; 16]>::try_from(bytes).map(Id::de_bytes).map_err(|_| {
        rusqlite::Error::InvalidColumnType(0, "id".into(), rusqlite::types::Type::Blob)
    })
}

/// E-mail como chave: minúsculo e sem espaço nas bordas.
#[must_use]
pub fn normalizar_email(email: &str) -> String {
    email.trim().to_lowercase()
}

impl Diretorio {
    /// Abre (criando se preciso) e migra o diretório.
    ///
    /// # Errors
    /// `BANCO_INDISPONIVEL` se o arquivo não abre ou a migração falha.
    pub fn abrir(caminho: &Path) -> Resultado<Self> {
        if let Some(pasta) = caminho.parent() {
            std::fs::create_dir_all(pasta).map_err(|e| {
                Erro::novo(
                    CodigoErro::FALHA_DE_DISCO,
                    format!("criar {}: {e}", pasta.display()),
                )
            })?;
        }
        let conn = Connection::open(caminho).map_err(|e| falha(&e))?;
        for (chave, valor) in [
            ("journal_mode", "wal"),
            ("synchronous", "FULL"),
            ("foreign_keys", "ON"),
            ("busy_timeout", "5000"),
            ("cache_size", "-512"),
            ("trusted_schema", "OFF"),
        ] {
            conn.pragma_update(None, chave, valor)
                .map_err(|e| falha(&e))?;
        }
        migrar(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    // ── contas ─────────────────────────────────────────────────────────────

    /// A conta deste e-mail, se existir.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn conta_por_email(&self, email: &str) -> Resultado<Option<Conta>> {
        let email = normalizar_email(email);
        self.conn
            .lock()
            .query_row(
                "SELECT id, email, nome, senha_hash, tentativas, bloqueado_ate
                 FROM conta WHERE email = ?1",
                [&email],
                |r| {
                    Ok(Conta {
                        id: id_de(&r.get::<_, Vec<u8>>(0)?)?,
                        email: r.get(1)?,
                        nome: r.get(2)?,
                        senha: HashDeSenha::de_phc(r.get(3)?),
                        bloqueio: Bloqueio {
                            tentativas: r.get(4)?,
                            bloqueado_ate: r.get::<_, Option<i64>>(5)?.map(Instante::de_micros),
                        },
                    })
                },
            )
            .optional()
            .map_err(|e| falha(&e))
    }

    /// O nome de exibição de uma conta.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn nome_da_conta(&self, conta: Id) -> Resultado<Option<String>> {
        self.conn
            .lock()
            .query_row(
                "SELECT nome FROM conta WHERE id = ?1",
                [conta.em_bytes().as_slice()],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| falha(&e))
    }

    /// Cria uma conta.
    ///
    /// # Errors
    /// `DUPLICADO` se o e-mail já tem conta; falha do SQLite.
    pub fn criar_conta(&self, email: &str, nome: &str, senha: &HashDeSenha) -> Resultado<Id> {
        let id = Id::novo();
        self.conn
            .lock()
            .execute(
                "INSERT INTO conta (id, email, nome, senha_hash, criada_em) VALUES (?1,?2,?3,?4,?5)",
                params![
                    id.em_bytes().as_slice(),
                    normalizar_email(email),
                    nome.trim(),
                    senha.como_phc(),
                    Instante::agora().em_micros(),
                ],
            )
            .map_err(|e| match e {
                rusqlite::Error::SqliteFailure(f, _)
                    if f.code == rusqlite::ErrorCode::ConstraintViolation =>
                {
                    Erro::novo(CodigoErro::DUPLICADO, "já existe uma conta com este e-mail")
                        .no_campo("email")
                }
                outro => falha(&outro),
            })?;
        Ok(id)
    }

    /// A conta pelo id (para conferir a senha atual numa troca).
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn conta_por_id(&self, conta: Id) -> Resultado<Option<Conta>> {
        let email: Option<String> = self
            .conn
            .lock()
            .query_row(
                "SELECT email FROM conta WHERE id = ?1",
                [conta.em_bytes().as_slice()],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| falha(&e))?;
        match email {
            Some(e) => self.conta_por_email(&e),
            None => Ok(None),
        }
    }

    /// Grava a senha nova de uma conta e zera tentativas/bloqueio.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn gravar_senha(&self, conta: Id, senha: &HashDeSenha) -> Resultado<()> {
        self.conn
            .lock()
            .execute(
                "UPDATE conta SET senha_hash = ?2, tentativas = 0, bloqueado_ate = NULL
                 WHERE id = ?1",
                params![conta.em_bytes().as_slice(), senha.como_phc()],
            )
            .map(|_| ())
            .map_err(|e| falha(&e))
    }

    /// Encerra todas as sessões da conta, menos a do `manter`. Devolve os hashes encerrados
    /// (para tirar do cache).
    ///
    /// # Errors
    /// Falha do SQLite.
    pub(crate) fn encerrar_outras_sessoes(
        &self,
        conta: Id,
        manter: &HashToken,
    ) -> Resultado<Vec<HashToken>> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "DELETE FROM sessao WHERE conta = ?1 AND token_hash <> ?2 RETURNING token_hash",
            )
            .map_err(|e| falha(&e))?;
        let linhas = stmt
            .query_map(
                params![conta.em_bytes().as_slice(), manter.as_slice()],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .map_err(|e| falha(&e))?
            .filter_map(Result::ok)
            .filter_map(|v| <[u8; 32]>::try_from(v).ok())
            .collect();
        Ok(linhas)
    }

    /// Persiste o estado de tentativas/bloqueio de uma conta.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn gravar_bloqueio(&self, conta: Id, bloqueio: Bloqueio) -> Resultado<()> {
        self.conn
            .lock()
            .execute(
                "UPDATE conta SET tentativas = ?2, bloqueado_ate = ?3 WHERE id = ?1",
                params![
                    conta.em_bytes().as_slice(),
                    bloqueio.tentativas,
                    bloqueio.bloqueado_ate.map(Instante::em_micros),
                ],
            )
            .map(|_| ())
            .map_err(|e| falha(&e))
    }

    // ── empresas e vínculos ────────────────────────────────────────────────

    /// Registra uma empresa já provisionada em disco.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn registrar_empresa(&self, empresa: Id, nome: &str) -> Resultado<()> {
        self.conn
            .lock()
            .execute(
                "INSERT INTO empresa (id, nome, criada_em) VALUES (?1, ?2, ?3)",
                params![
                    empresa.em_bytes().as_slice(),
                    nome.trim(),
                    Instante::agora().em_micros()
                ],
            )
            .map(|_| ())
            .map_err(|e| falha(&e))
    }

    /// Dá a uma conta acesso a uma empresa, como um usuário daquela base.
    ///
    /// # Errors
    /// Falha do SQLite (inclusive conta/empresa inexistente).
    pub fn vincular(&self, conta: Id, empresa: Id, usuario: Id) -> Resultado<()> {
        self.conn
            .lock()
            .execute(
                "INSERT INTO vinculo (conta, empresa, usuario) VALUES (?1, ?2, ?3)
                 ON CONFLICT (conta, empresa) DO UPDATE SET usuario = excluded.usuario",
                params![
                    conta.em_bytes().as_slice(),
                    empresa.em_bytes().as_slice(),
                    usuario.em_bytes().as_slice(),
                ],
            )
            .map(|_| ())
            .map_err(|e| falha(&e))
    }

    /// Desfaz o vínculo do `usuario` com a `empresa`. Devolve a conta que estava vinculada.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn desvincular(&self, empresa: Id, usuario: Id) -> Resultado<Option<Id>> {
        self.conn
            .lock()
            .query_row(
                "DELETE FROM vinculo WHERE empresa = ?1 AND usuario = ?2 RETURNING conta",
                params![empresa.em_bytes().as_slice(), usuario.em_bytes().as_slice()],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .optional()
            .map_err(|e| falha(&e))
            .map(|c| {
                c.and_then(|b| <[u8; 16]>::try_from(b).ok())
                    .map(Id::de_bytes)
            })
    }

    /// As empresas **ativas** em que a conta entra.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn vinculos(&self, conta: Id) -> Resultado<Vec<Vinculo>> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare_cached(
                "SELECT v.empresa, e.nome, v.usuario
                 FROM vinculo v JOIN empresa e ON e.id = v.empresa
                 WHERE v.conta = ?1 AND e.ativa = 1
                 ORDER BY e.nome",
            )
            .map_err(|e| falha(&e))?;
        let linhas = stmt
            .query_map([conta.em_bytes().as_slice()], |r| {
                Ok(Vinculo {
                    empresa: id_de(&r.get::<_, Vec<u8>>(0)?)?,
                    nome: r.get(1)?,
                    usuario: id_de(&r.get::<_, Vec<u8>>(2)?)?,
                })
            })
            .map_err(|e| falha(&e))?;
        linhas
            .collect::<rusqlite::Result<_>>()
            .map_err(|e| falha(&e))
    }

    // ── sessões ────────────────────────────────────────────────────────────

    /// Grava uma sessão nova.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub(crate) fn criar_sessao(&self, hash: &HashToken, sessao: SessaoPersistida) -> Resultado<()> {
        self.conn
            .lock()
            .execute(
                "INSERT INTO sessao (token_hash, id, conta, criada_em, expira_em)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    hash.as_slice(),
                    sessao.id.em_bytes().as_slice(),
                    sessao.conta.em_bytes().as_slice(),
                    Instante::agora().em_micros(),
                    sessao.expira_em.em_micros(),
                ],
            )
            .map(|_| ())
            .map_err(|e| falha(&e))
    }

    /// A sessão deste token, se existir e não tiver expirado.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub(crate) fn sessao(
        &self,
        hash: &HashToken,
        agora: Instante,
    ) -> Resultado<Option<SessaoPersistida>> {
        self.conn
            .lock()
            .query_row(
                "SELECT id, conta, expira_em FROM sessao WHERE token_hash = ?1 AND expira_em > ?2",
                params![hash.as_slice(), agora.em_micros()],
                |r| {
                    Ok(SessaoPersistida {
                        id: id_de(&r.get::<_, Vec<u8>>(0)?)?,
                        conta: id_de(&r.get::<_, Vec<u8>>(1)?)?,
                        expira_em: Instante::de_micros(r.get(2)?),
                    })
                },
            )
            .optional()
            .map_err(|e| falha(&e))
    }

    /// Encerra (apaga) uma sessão.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub(crate) fn encerrar_sessao(&self, hash: &HashToken) -> Resultado<()> {
        self.conn
            .lock()
            .execute(
                "DELETE FROM sessao WHERE token_hash = ?1",
                [hash.as_slice()],
            )
            .map(|_| ())
            .map_err(|e| falha(&e))
    }

    /// Apaga as sessões expiradas. Devolve quantas.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn limpar_sessoes_expiradas(&self, agora: Instante) -> Resultado<usize> {
        self.conn
            .lock()
            .execute(
                "DELETE FROM sessao WHERE expira_em <= ?1",
                [agora.em_micros()],
            )
            .map_err(|e| falha(&e))
    }
}

fn migrar(conn: &Connection) -> Resultado<()> {
    let atual: usize = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(|e| falha(&e))?;
    for (i, sql) in esquema::MIGRACOES.iter().enumerate().skip(atual) {
        let versao = i + 1;
        conn.execute_batch(&format!(
            "BEGIN; {sql}; PRAGMA user_version = {versao}; COMMIT;"
        ))
        .map_err(|e| falha(&e))?;
    }
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    fn diretorio() -> (tempfile::TempDir, Diretorio) {
        let pasta = tempfile::tempdir().unwrap();
        let dir = Diretorio::abrir(&pasta.path().join("diretorio.db")).unwrap();
        (pasta, dir)
    }

    #[test]
    fn email_e_chave_sem_diferenciar_maiusculas() {
        let (_p, dir) = diretorio();
        let hash = HashDeSenha::de_phc("x".into());
        dir.criar_conta("Ana@Exemplo.com ", "Ana", &hash).unwrap();
        assert!(dir.conta_por_email("ana@exemplo.com").unwrap().is_some());
        let erro = dir
            .criar_conta("ANA@exemplo.com", "Outra", &hash)
            .unwrap_err();
        assert_eq!(erro.codigo, CodigoErro::DUPLICADO);
    }

    #[test]
    fn reabrir_nao_reaplica_migracao() {
        let pasta = tempfile::tempdir().unwrap();
        let caminho = pasta.path().join("diretorio.db");
        drop(Diretorio::abrir(&caminho).unwrap());
        Diretorio::abrir(&caminho).unwrap();
    }

    #[test]
    fn so_empresas_ativas_aparecem_nos_vinculos() {
        let (_p, dir) = diretorio();
        let conta = dir
            .criar_conta("a@b.c", "A", &HashDeSenha::de_phc("x".into()))
            .unwrap();
        let (e1, e2) = (Id::novo(), Id::novo());
        dir.registrar_empresa(e1, "Loja B").unwrap();
        dir.registrar_empresa(e2, "Loja A").unwrap();
        dir.vincular(conta, e1, Id::novo()).unwrap();
        dir.vincular(conta, e2, Id::novo()).unwrap();
        dir.conn
            .lock()
            .execute(
                "UPDATE empresa SET ativa = 0 WHERE id = ?1",
                [e1.em_bytes().as_slice()],
            )
            .unwrap();
        let v = dir.vinculos(conta).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].nome, "Loja A");
    }

    #[test]
    fn sessao_expirada_nao_e_encontrada() {
        let (_p, dir) = diretorio();
        let conta = dir
            .criar_conta("a@b.c", "A", &HashDeSenha::de_phc("x".into()))
            .unwrap();
        let (_, hash) = crate::token::gerar();
        let agora = Instante::agora();
        dir.criar_sessao(
            &hash,
            SessaoPersistida {
                id: Id::novo(),
                conta,
                expira_em: agora.mais_segundos(60),
            },
        )
        .unwrap();
        assert!(dir.sessao(&hash, agora).unwrap().is_some());
        assert!(dir
            .sessao(&hash, agora.mais_segundos(61))
            .unwrap()
            .is_none());
        assert_eq!(
            dir.limpar_sessoes_expiradas(agora.mais_segundos(61))
                .unwrap(),
            1
        );
    }
}
