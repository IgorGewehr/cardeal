//! O adaptador SQLite da identidade: [`RepositorioAuth`] grava e lê `nucleo_usuario`,
//! `nucleo_papel`, `nucleo_papel_permissao`, `nucleo_papel_limite` e `nucleo_usuario_papel`
//! sobre a [`UnidadeDeTrabalho`](cardeal_storage::UnidadeDeTrabalho) do escritor único.
//!
//! `docs/06-modelo-de-dados.md` §2 e `docs/08-seguranca-permissoes.md` §2–§3. Toda escrita
//! roda dentro do `SAVEPOINT` da tarefa do escritor — se o comando falhar depois, o registro
//! é desfeito junto. Leituras usam o pool do [`Leitor`](cardeal_storage::Leitor) e vivem no
//! módulo [`consultas`].

use std::collections::{BTreeMap, BTreeSet};

use cardeal_kernel::{Dinheiro, Id, Instante, Percentual, Versao};
use cardeal_storage::UnidadeDeTrabalho;
use rusqlite::Connection;

use crate::bloqueio::Bloqueio;
use crate::erros::ErroAuth;
use crate::limite::ValorLimite;
use crate::papel::Papel;
use crate::senha::HashDeSenha;
use crate::usuario::Usuario;

type Resultado<T> = Result<T, ErroAuth>;

#[allow(clippy::needless_pass_by_value)] // usado como `.map_err(persist)`
fn persist(e: rusqlite::Error) -> ErroAuth {
    ErroAuth::FalhaDePersistencia(e.to_string())
}

fn blob(id: Id) -> Vec<u8> {
    id.em_bytes().to_vec()
}

fn blob_opt(id: Option<Id>) -> Option<Vec<u8>> {
    id.map(blob)
}

fn id_de(bytes: Vec<u8>) -> Id {
    <[u8; 16]>::try_from(bytes).map_or(Id::NULO, Id::de_bytes)
}

// ─── serialização de limite ─────────────────────────────────────────────────

fn limite_split(v: ValorLimite) -> (&'static str, i64) {
    match v {
        ValorLimite::Dinheiro(d) => ("Dinheiro", d.em_centavos()),
        ValorLimite::Percentual(p) => ("Percentual", p.unidades_internas()),
        ValorLimite::Contagem(n) => ("Contagem", i64::from(n)),
        ValorLimite::Dias(n) => ("Dias", i64::from(n)),
        ValorLimite::Ilimitado => ("Ilimitado", 0),
    }
}

fn limite_join(tipo: &str, valor: i64) -> ValorLimite {
    let n = || u32::try_from(valor).unwrap_or(0);
    match tipo {
        "Dinheiro" => ValorLimite::Dinheiro(Dinheiro::centavos(valor)),
        "Percentual" => ValorLimite::Percentual(Percentual::unidades(valor)),
        "Contagem" => ValorLimite::Contagem(n()),
        "Dias" => ValorLimite::Dias(n()),
        _ => ValorLimite::Ilimitado,
    }
}

// ─── leitura (módulo consultas) ─────────────────────────────────────────────

/// Consultas de identidade sobre uma conexão de leitura — sem tocar no escritor.
pub mod consultas {
    use super::{
        carregar_limites, carregar_permissoes, usuario_de_linha, Connection, Id, Papel, Resultado,
        Usuario,
    };
    use rusqlite::OptionalExtension;

    const COLUNAS_USUARIO: &str = "id, login, nome, email, senha_hash, senha_trocada_em, \
         exige_troca, mfa_segredo, ativo, bloqueado_ate, tentativas, versao";

    /// Busca um usuário pelo `id`. `Ok(None)` = não existe.
    ///
    /// # Errors
    /// [`crate::ErroAuth::FalhaDePersistencia`] em erro do SQLite.
    pub fn usuario_por_id(c: &Connection, id: Id) -> Resultado<Option<Usuario>> {
        let sql = format!("SELECT {COLUNAS_USUARIO} FROM nucleo_usuario WHERE id = ?1");
        c.query_row(&sql, [id.em_bytes().as_slice()], usuario_de_linha)
            .optional()
            .map_err(super::persist)
    }

    /// Busca um usuário pelo `login` (já normalizado: minúsculo, sem espaço nas pontas).
    ///
    /// # Errors
    /// [`crate::ErroAuth::FalhaDePersistencia`] em erro do SQLite.
    pub fn usuario_por_login(c: &Connection, login: &str) -> Resultado<Option<Usuario>> {
        let sql = format!("SELECT {COLUNAS_USUARIO} FROM nucleo_usuario WHERE login = ?1");
        c.query_row(&sql, [login.trim().to_lowercase()], usuario_de_linha)
            .optional()
            .map_err(super::persist)
    }

    /// Carrega um papel pelo `id`, com suas permissões e limites. `Ok(None)` = não existe.
    ///
    /// # Errors
    /// [`crate::ErroAuth::FalhaDePersistencia`] em erro do SQLite.
    pub fn papel_por_id(c: &Connection, id: Id) -> Resultado<Option<Papel>> {
        let cabecalho = c
            .query_row(
                "SELECT empresa, nome, descricao, sistema, versao
                 FROM nucleo_papel WHERE id = ?1",
                [id.em_bytes().as_slice()],
                |r| {
                    Ok((
                        r.get::<_, Option<Vec<u8>>>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Option<String>>(2)?,
                        r.get::<_, i64>(3)?,
                        r.get::<_, i64>(4)?,
                    ))
                },
            )
            .optional()
            .map_err(super::persist)?;

        let Some((empresa, nome, descricao, sistema, versao)) = cabecalho else {
            return Ok(None);
        };
        Ok(Some(Papel {
            id,
            empresa: empresa.map(super::id_de),
            nome,
            descricao: descricao.unwrap_or_default(),
            sistema: sistema != 0,
            permissoes: carregar_permissoes(c, id)?,
            limites: carregar_limites(c, id)?,
            versao: cardeal_kernel::Versao::nova(u64::try_from(versao).unwrap_or(1)),
        }))
    }

    /// Todos os papéis do usuário aplicáveis a `empresa` — os atribuídos nessa empresa e os
    /// globais (`empresa IS NULL` na atribuição). Cada papel vem com permissões e limites.
    ///
    /// # Errors
    /// [`crate::ErroAuth::FalhaDePersistencia`] em erro do SQLite.
    pub fn papeis_do_usuario(c: &Connection, usuario: Id, empresa: Id) -> Resultado<Vec<Papel>> {
        let ids: Vec<Id> = {
            let mut stmt = c
                .prepare(
                    "SELECT papel FROM nucleo_usuario_papel
                     WHERE usuario = ?1 AND (empresa = ?2 OR empresa IS NULL)",
                )
                .map_err(super::persist)?;
            let linhas = stmt
                .query_map(
                    rusqlite::params![usuario.em_bytes().as_slice(), empresa.em_bytes().as_slice()],
                    |r| r.get::<_, Vec<u8>>(0),
                )
                .map_err(super::persist)?;
            let mut ids = Vec::new();
            for l in linhas {
                ids.push(super::id_de(l.map_err(super::persist)?));
            }
            ids
        };

        let mut papeis = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(p) = papel_por_id(c, id)? {
                papeis.push(p);
            }
        }
        Ok(papeis)
    }
}

fn usuario_de_linha(r: &rusqlite::Row<'_>) -> rusqlite::Result<Usuario> {
    let bloqueado_ate: Option<i64> = r.get(9)?;
    Ok(Usuario {
        id: id_de(r.get::<_, Vec<u8>>(0)?),
        login: r.get(1)?,
        nome: r.get(2)?,
        email: r.get(3)?,
        hash_senha: HashDeSenha::de_phc(r.get::<_, String>(4)?),
        senha_trocada_em: Instante::de_micros(r.get(5)?),
        exige_troca_senha: r.get::<_, i64>(6)? != 0,
        mfa_habilitado: r.get::<_, Option<String>>(7)?.is_some(),
        ativo: r.get::<_, i64>(8)? != 0,
        bloqueio: Bloqueio {
            tentativas: u32::try_from(r.get::<_, i64>(10)?).unwrap_or(0),
            bloqueado_ate: bloqueado_ate.map(Instante::de_micros),
        },
        versao: Versao::nova(u64::try_from(r.get::<_, i64>(11)?).unwrap_or(1)),
    })
}

fn carregar_permissoes(c: &Connection, papel: Id) -> Resultado<BTreeSet<String>> {
    let mut stmt = c
        .prepare("SELECT permissao FROM nucleo_papel_permissao WHERE papel = ?1")
        .map_err(persist)?;
    let linhas = stmt
        .query_map([blob(papel)], |r| r.get::<_, String>(0))
        .map_err(persist)?;
    let mut perms = BTreeSet::new();
    for l in linhas {
        perms.insert(l.map_err(persist)?);
    }
    Ok(perms)
}

fn carregar_limites(c: &Connection, papel: Id) -> Resultado<BTreeMap<String, ValorLimite>> {
    let mut stmt = c
        .prepare("SELECT chave, tipo, valor FROM nucleo_papel_limite WHERE papel = ?1")
        .map_err(persist)?;
    let linhas = stmt
        .query_map([blob(papel)], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })
        .map_err(persist)?;
    let mut limites = BTreeMap::new();
    for l in linhas {
        let (chave, tipo, valor) = l.map_err(persist)?;
        limites.insert(chave, limite_join(&tipo, valor));
    }
    Ok(limites)
}

// ─── escrita (RepositorioAuth) ─────────────────────────────────────────────

/// Grava e lê a identidade sobre a unidade de trabalho corrente do escritor.
pub struct RepositorioAuth<'a, 'b> {
    uow: &'a mut UnidadeDeTrabalho<'b>,
}

impl<'a, 'b> RepositorioAuth<'a, 'b> {
    /// Cria o repositório sobre a unidade de trabalho corrente.
    pub fn novo(uow: &'a mut UnidadeDeTrabalho<'b>) -> Self {
        Self { uow }
    }

    fn conn(&self) -> &Connection {
        self.uow.conexao()
    }

    // ── usuário ───────────────────────────────────────────────────────────

    /// Insere um usuário novo. `criado_em` sai do relógio da unidade de trabalho e
    /// `criado_por` do usuário responsável por ela.
    ///
    /// # Errors
    /// [`crate::ErroAuth::FalhaDePersistencia`] em erro do SQLite (inclusive `login` duplicado).
    pub fn inserir_usuario(&mut self, u: &Usuario) -> Resultado<()> {
        let criado_por = self.uow.usuario();
        let criado_em = self.uow.agora().em_micros();
        self.conn()
            .execute(
                "INSERT INTO nucleo_usuario
                   (id, login, nome, email, senha_hash, senha_trocada_em, exige_troca,
                    mfa_segredo, ativo, bloqueado_ate, tentativas, versao, criado_em, criado_por)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,NULL,?8,?9,?10,?11,?12,?13)",
                rusqlite::params![
                    blob(u.id),
                    u.login,
                    u.nome,
                    u.email,
                    u.hash_senha.como_phc(),
                    u.senha_trocada_em.em_micros(),
                    i64::from(u.exige_troca_senha),
                    i64::from(u.ativo),
                    u.bloqueio.bloqueado_ate.map(Instante::em_micros),
                    i64::from(u.bloqueio.tentativas),
                    i64::try_from(u.versao.numero()).unwrap_or(i64::MAX),
                    criado_em,
                    blob_opt(criado_por_opt(criado_por)),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Regrava as credenciais e o estado de acesso de um usuário existente — o resultado de
    /// `autenticar`, `trocar_senha`, `redefinir_senha`, `desativar`/`reativar`,
    /// `desbloquear`. Não toca em `login`, `nome`, `email` nem `mfa`.
    ///
    /// # Errors
    /// [`crate::ErroAuth::FalhaDePersistencia`] em erro do SQLite.
    pub fn atualizar_credenciais(&mut self, u: &Usuario) -> Resultado<()> {
        self.conn()
            .execute(
                "UPDATE nucleo_usuario
                 SET senha_hash = ?2, senha_trocada_em = ?3, exige_troca = ?4, ativo = ?5,
                     bloqueado_ate = ?6, tentativas = ?7, versao = ?8
                 WHERE id = ?1",
                rusqlite::params![
                    blob(u.id),
                    u.hash_senha.como_phc(),
                    u.senha_trocada_em.em_micros(),
                    i64::from(u.exige_troca_senha),
                    i64::from(u.ativo),
                    u.bloqueio.bloqueado_ate.map(Instante::em_micros),
                    i64::from(u.bloqueio.tentativas),
                    i64::try_from(u.versao.numero()).unwrap_or(i64::MAX),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Busca um usuário pelo `login`. `Ok(None)` = não existe.
    ///
    /// # Errors
    /// [`crate::ErroAuth::FalhaDePersistencia`] em erro do SQLite.
    pub fn usuario_por_login(&self, login: &str) -> Resultado<Option<Usuario>> {
        consultas::usuario_por_login(self.conn(), login)
    }

    /// Busca um usuário pelo `id`. `Ok(None)` = não existe.
    ///
    /// # Errors
    /// [`crate::ErroAuth::FalhaDePersistencia`] em erro do SQLite.
    pub fn usuario_por_id(&self, id: Id) -> Resultado<Option<Usuario>> {
        consultas::usuario_por_id(self.conn(), id)
    }

    // ── papel ─────────────────────────────────────────────────────────────

    /// Insere um papel novo com todas as suas permissões e limites. Aceita papéis de
    /// fábrica (`sistema = 1`) — é assim que a instalação os semeia.
    ///
    /// # Errors
    /// [`crate::ErroAuth::FalhaDePersistencia`] em erro do SQLite.
    pub fn inserir_papel(&mut self, p: &Papel) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO nucleo_papel (id, empresa, nome, descricao, sistema, versao)
                 VALUES (?1,?2,?3,?4,?5,?6)",
                rusqlite::params![
                    blob(p.id),
                    blob_opt(p.empresa),
                    p.nome,
                    p.descricao,
                    i64::from(p.sistema),
                    i64::try_from(p.versao.numero()).unwrap_or(i64::MAX),
                ],
            )
            .map_err(persist)?;
        self.regravar_permissoes(p)?;
        self.regravar_limites(p)?;
        Ok(())
    }

    /// Regrava o conteúdo de um papel editável (nome, descrição, permissões, limites,
    /// versão). Recusa papel de fábrica.
    ///
    /// # Errors
    /// [`ErroAuth::PapelDoSistema`] se o papel é de fábrica;
    /// [`crate::ErroAuth::FalhaDePersistencia`] em erro do SQLite.
    pub fn atualizar_papel(&mut self, p: &Papel) -> Resultado<()> {
        p.garantir_editavel()?;
        self.conn()
            .execute(
                "UPDATE nucleo_papel SET nome = ?2, descricao = ?3, versao = ?4 WHERE id = ?1",
                rusqlite::params![
                    blob(p.id),
                    p.nome,
                    p.descricao,
                    i64::try_from(p.versao.numero()).unwrap_or(i64::MAX),
                ],
            )
            .map_err(persist)?;
        self.regravar_permissoes(p)?;
        self.regravar_limites(p)?;
        Ok(())
    }

    /// Carrega um papel pelo `id`, com permissões e limites.
    ///
    /// # Errors
    /// [`crate::ErroAuth::FalhaDePersistencia`] em erro do SQLite.
    pub fn papel_por_id(&self, id: Id) -> Resultado<Option<Papel>> {
        consultas::papel_por_id(self.conn(), id)
    }

    /// Os papéis do usuário aplicáveis a `empresa` (atribuídos nessa empresa + globais).
    ///
    /// # Errors
    /// [`crate::ErroAuth::FalhaDePersistencia`] em erro do SQLite.
    pub fn papeis_do_usuario(&self, usuario: Id, empresa: Id) -> Resultado<Vec<Papel>> {
        consultas::papeis_do_usuario(self.conn(), usuario, empresa)
    }

    /// Atribui um papel a um usuário numa empresa. Idempotente.
    ///
    /// # Errors
    /// [`crate::ErroAuth::FalhaDePersistencia`] em erro do SQLite.
    pub fn atribuir_papel(&mut self, usuario: Id, papel: Id, empresa: Id) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT OR IGNORE INTO nucleo_usuario_papel (usuario, papel, empresa)
                 VALUES (?1,?2,?3)",
                rusqlite::params![blob(usuario), blob(papel), blob(empresa)],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Remove a atribuição de um papel a um usuário numa empresa. Idempotente.
    ///
    /// # Errors
    /// [`crate::ErroAuth::FalhaDePersistencia`] em erro do SQLite.
    pub fn revogar_papel(&mut self, usuario: Id, papel: Id, empresa: Id) -> Resultado<()> {
        self.conn()
            .execute(
                "DELETE FROM nucleo_usuario_papel
                 WHERE usuario = ?1 AND papel = ?2 AND empresa = ?3",
                rusqlite::params![blob(usuario), blob(papel), blob(empresa)],
            )
            .map_err(persist)?;
        Ok(())
    }

    fn regravar_permissoes(&self, p: &Papel) -> Resultado<()> {
        self.conn()
            .execute(
                "DELETE FROM nucleo_papel_permissao WHERE papel = ?1",
                [blob(p.id)],
            )
            .map_err(persist)?;
        for chave in &p.permissoes {
            self.conn()
                .execute(
                    "INSERT INTO nucleo_papel_permissao (papel, permissao) VALUES (?1,?2)",
                    rusqlite::params![blob(p.id), chave],
                )
                .map_err(persist)?;
        }
        Ok(())
    }

    fn regravar_limites(&self, p: &Papel) -> Resultado<()> {
        self.conn()
            .execute(
                "DELETE FROM nucleo_papel_limite WHERE papel = ?1",
                [blob(p.id)],
            )
            .map_err(persist)?;
        for (chave, valor) in &p.limites {
            let (tipo, bruto) = limite_split(*valor);
            self.conn()
                .execute(
                    "INSERT INTO nucleo_papel_limite (papel, chave, tipo, valor)
                     VALUES (?1,?2,?3,?4)",
                    rusqlite::params![blob(p.id), chave, tipo, bruto],
                )
                .map_err(persist)?;
        }
        Ok(())
    }
}

/// `criado_por` é `NULL` quando a unidade de trabalho não tem um usuário real (semeadura na
/// instalação, migrações) — o `Id::NULO` do contexto vira `None`.
fn criado_por_opt(usuario: Id) -> Option<Id> {
    if usuario.e_nulo() {
        None
    } else {
        Some(usuario)
    }
}
