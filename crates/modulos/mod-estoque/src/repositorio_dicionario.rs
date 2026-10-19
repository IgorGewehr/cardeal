//! SQL do dicionário DE/PARA de nomes e dos padrões de quem chega sem grupo/unidade (o grupo
//! "Sem grupo" e a unidade `UN`, os mesmos da importação de produtos por planilha). Funções
//! livres.

use cardeal_kernel::texto::chave_busca;
use cardeal_kernel::{Id, Instante, Resultado};
use cardeal_storage::UnidadeDeTrabalho;
use rusqlite::{params, Connection, OptionalExtension};

use crate::padronizacao::{Dicionario, TermoDicionario};
use crate::produto::Unidade;
use crate::repositorio::{blob, id_de, persist, RepositorioEstoque};

/// Grava (ou troca) um termo. `de` já normalizado.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn gravar_termo(
    conn: &Connection,
    organizacao: Id,
    de: &str,
    para: &str,
    agora: Instante,
) -> Resultado<()> {
    conn.execute(
        "INSERT INTO estoque_dicionario_nome (empresa, de, para, atualizado_em)
         VALUES (?1,?2,?3,?4)
         ON CONFLICT (empresa, de) DO UPDATE SET para = excluded.para,
                                                 atualizado_em = excluded.atualizado_em",
        params![blob(organizacao), de, para, agora.em_micros()],
    )
    .map_err(persist)?;
    Ok(())
}

/// Tira um termo.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn remover_termo(conn: &Connection, organizacao: Id, de: &str) -> Resultado<()> {
    conn.execute(
        "DELETE FROM estoque_dicionario_nome WHERE empresa = ?1 AND de = ?2",
        params![blob(organizacao), de],
    )
    .map_err(persist)?;
    Ok(())
}

/// Os termos, em ordem alfabética.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn termos(conn: &Connection, organizacao: Id) -> Resultado<Vec<TermoDicionario>> {
    let mut stmt = conn
        .prepare("SELECT de, para FROM estoque_dicionario_nome WHERE empresa = ?1 ORDER BY de ASC")
        .map_err(persist)?;
    let linhas = stmt
        .query_map([blob(organizacao)], |r| {
            Ok(TermoDicionario {
                de: r.get(0)?,
                para: r.get(1)?,
            })
        })
        .map_err(persist)?;
    linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)
}

/// O dicionário pronto para sugerir.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn dicionario(conn: &Connection, organizacao: Id) -> Resultado<Dicionario> {
    Ok(Dicionario::de_termos(&termos(conn, organizacao)?))
}

/// O grupo "Sem grupo" na raiz (criado na primeira vez, com o mesmo código da importação).
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn grupo_sem_grupo(uow: &mut UnidadeDeTrabalho, organizacao: Id) -> Resultado<Id> {
    let alvo = chave_busca(crate::importacao_produtos::GRUPO_SEM_GRUPO);
    let raizes: Vec<(Id, String, String)> = {
        let mut stmt = uow
            .conexao()
            .prepare(
                "SELECT id, nome, codigo FROM estoque_grupo_produto
                 WHERE empresa = ?1 AND pai IS NULL",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map([blob(organizacao)], |r| {
                Ok((id_de(r.get::<_, Vec<u8>>(0)?), r.get(1)?, r.get(2)?))
            })
            .map_err(persist)?;
        linhas
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)?
    };
    if let Some((id, _, _)) = raizes.iter().find(|(_, nome, _)| chave_busca(nome) == alvo) {
        return Ok(*id);
    }
    let mut codigo = "SEMGRU".to_owned();
    let mut n = 2;
    while uow
        .conexao()
        .query_row(
            "SELECT 1 FROM estoque_grupo_produto WHERE empresa = ?1 AND codigo = ?2",
            params![blob(organizacao), codigo],
            |_| Ok(()),
        )
        .optional()
        .map_err(persist)?
        .is_some()
    {
        codigo = format!("SEMGRU{n}");
        n += 1;
    }
    let id = Id::novo();
    RepositorioEstoque::novo(uow).inserir_grupo_produto(
        organizacao,
        id,
        &codigo,
        crate::importacao_produtos::GRUPO_SEM_GRUPO,
        None,
    )?;
    Ok(id)
}

/// A unidade `UN` (criada na primeira vez).
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn unidade_un(uow: &mut UnidadeDeTrabalho, organizacao: Id) -> Resultado<Id> {
    let existente: Option<Vec<u8>> = uow
        .conexao()
        .query_row(
            "SELECT id FROM estoque_unidade WHERE empresa = ?1 AND UPPER(sigla) = 'UN'",
            [blob(organizacao)],
            |r| r.get(0),
        )
        .optional()
        .map_err(persist)?;
    if let Some(id) = existente {
        return Ok(id_de(id));
    }
    let unidade = Unidade {
        id: Id::novo(),
        empresa: organizacao,
        sigla: crate::importacao_produtos::UNIDADE_PADRAO.to_owned(),
        nome: "Unidade".to_owned(),
        fracionavel: false,
    };
    RepositorioEstoque::novo(uow).inserir_unidade(&unidade)?;
    Ok(unidade.id)
}
