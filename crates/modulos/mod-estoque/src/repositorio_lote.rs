//! SQL da alteração em lote: leitura/gravação dos campos que o lote mexe e o registro do
//! antes/depois para poder desfazer. Funções livres sobre `&Connection`.

use cardeal_kernel::{Id, Instante, Percentual, Resultado};
use rusqlite::{params, Connection, OptionalExtension};

use crate::alteracao_em_lote::{FiltroDeLote, ValoresProduto};
use crate::fiscal::CamposFiscais;
use crate::repositorio::{blob, id_de, persist};

/// Um produto com o que o lote enxerga dele.
#[derive(Debug, Clone)]
pub struct ProdutoDoLote {
    /// O produto.
    pub produto: Id,
    /// O nome.
    pub nome: String,
    /// Grupo, margem própria, situação, NCM e campos fiscais próprios.
    pub valores: ValoresProduto,
    /// O fabricante, quando cadastrado.
    pub fabricante: Option<String>,
    /// O maior custo médio em estoque (unidades internas de `Preco`); 0 = sem custo.
    pub custo: i64,
}

const COLUNAS_PRODUTO: &str =
    "id, nome, grupo_produto, margem_desejada, ativo, ncm, perfil_fiscal, fabricante";

fn de_linha(r: &rusqlite::Row<'_>) -> rusqlite::Result<ProdutoDoLote> {
    Ok(ProdutoDoLote {
        produto: id_de(r.get::<_, Vec<u8>>(0)?),
        nome: r.get(1)?,
        valores: ValoresProduto {
            grupo: id_de(r.get::<_, Vec<u8>>(2)?),
            margem: r.get::<_, Option<i64>>(3)?.map(Percentual::unidades),
            ativo: r.get::<_, i64>(4)? != 0,
            ncm: r.get(5)?,
            fiscal: CamposFiscais::de_blob(r.get::<_, Option<Vec<u8>>>(6)?.as_deref()),
        },
        fabricante: r.get::<_, Option<String>>(7)?,
        custo: r.get::<_, i64>(8)?,
    })
}

/// O produto como o lote o vê; `None` se não existe na organização.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn produto_do_lote(
    conn: &Connection,
    organizacao: Id,
    produto: Id,
) -> Resultado<Option<ProdutoDoLote>> {
    conn.query_row(
        &format!("SELECT {COLUNAS_PRODUTO}, 0 FROM estoque_produto WHERE empresa = ?1 AND id = ?2"),
        params![blob(organizacao), blob(produto)],
        de_linha,
    )
    .optional()
    .map_err(persist)
}

fn casa(f: &FiltroDeLote, p: &ProdutoDoLote) -> bool {
    let contem = |campo: &str, termo: &Option<String>| {
        termo
            .as_ref()
            .map(|t| t.trim().to_lowercase())
            .filter(|t| !t.is_empty())
            .is_none_or(|t| campo.to_lowercase().contains(&t))
    };
    let centavos = p.custo / 10_000; // `Preco` tem escala 1e-6; centavo = 1e-2
    f.grupos
        .as_ref()
        .is_none_or(|g| g.contains(&p.valores.grupo))
        && contem(&p.nome, &f.busca)
        && contem(p.fabricante.as_deref().unwrap_or(""), &f.fabricante)
        && f.ncm_prefixo
            .as_ref()
            .is_none_or(|n| p.valores.ncm.starts_with(n.trim()))
        && f.ativo.is_none_or(|a| a == p.valores.ativo)
        && (!f.sem_margem_propria || p.valores.margem.is_none())
        && (!f.sem_perfil_fiscal_proprio || p.valores.fiscal.e_vazio())
        && f.custo_minimo.is_none_or(|m| centavos >= m)
        && f.custo_maximo.is_none_or(|m| centavos <= m)
        && (!f.sem_custo || p.custo == 0)
}

/// Os produtos (ativos e inativos) que casam com o filtro, por nome.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn produtos_do_lote(
    conn: &Connection,
    organizacao: Id,
    empresa: Id,
    filtro: &FiltroDeLote,
    limite: usize,
) -> Resultado<Vec<ProdutoDoLote>> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {COLUNAS_PRODUTO},
                    COALESCE((SELECT MAX(s.custo_medio) FROM estoque_saldo_local s
                              WHERE s.produto = estoque_produto.id AND s.empresa = ?2), 0)
             FROM estoque_produto WHERE empresa = ?1 ORDER BY nome ASC, id ASC"
        ))
        .map_err(persist)?;
    let linhas = stmt
        .query_map(params![blob(organizacao), blob(empresa)], de_linha)
        .map_err(persist)?;
    let mut saida = Vec::new();
    for l in linhas {
        let p = l.map_err(persist)?;
        if !casa(filtro, &p) {
            continue;
        }
        saida.push(p);
        if saida.len() >= limite {
            break;
        }
    }
    Ok(saida)
}

/// Grava os campos do lote num produto.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn gravar_valores(
    conn: &Connection,
    organizacao: Id,
    produto: Id,
    v: &ValoresProduto,
) -> Resultado<()> {
    conn.execute(
        "UPDATE estoque_produto
         SET grupo_produto = ?3, margem_desejada = ?4, ativo = ?5, ncm = ?6, perfil_fiscal = ?7,
             versao = versao + 1
         WHERE empresa = ?1 AND id = ?2",
        params![
            blob(organizacao),
            blob(produto),
            blob(v.grupo),
            v.margem.map(Percentual::unidades_internas),
            i64::from(v.ativo),
            v.ncm.clone(),
            v.fiscal.para_blob(),
        ],
    )
    .map_err(persist)?;
    Ok(())
}

/// Abre o registro de uma alteração em lote.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
#[allow(clippy::too_many_arguments)]
pub fn inserir_lote(
    conn: &Connection,
    organizacao: Id,
    id: Id,
    usuario: Id,
    criado_em: Instante,
    descricao: &str,
    campos: &str,
    quantidade: usize,
) -> Resultado<()> {
    conn.execute(
        "INSERT INTO estoque_alteracao_lote
            (id, empresa, usuario, criado_em, descricao, campos, quantidade)
         VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![
            blob(id),
            blob(organizacao),
            blob(usuario),
            criado_em.em_micros(),
            descricao,
            campos,
            i64::try_from(quantidade).unwrap_or(i64::MAX),
        ],
    )
    .map_err(persist)?;
    Ok(())
}

/// Guarda o antes e o depois de um produto na alteração.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn inserir_item(
    conn: &Connection,
    lote: Id,
    produto: Id,
    antes: &ValoresProduto,
    depois: &ValoresProduto,
) -> Resultado<()> {
    conn.execute(
        "INSERT INTO estoque_alteracao_lote_item
            (lote, produto, antes_grupo, antes_margem, antes_ativo,
             depois_grupo, depois_margem, depois_ativo,
             antes_ncm, depois_ncm, antes_fiscal, depois_fiscal)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
        params![
            blob(lote),
            blob(produto),
            blob(antes.grupo),
            antes.margem.map(Percentual::unidades_internas),
            i64::from(antes.ativo),
            blob(depois.grupo),
            depois.margem.map(Percentual::unidades_internas),
            i64::from(depois.ativo),
            antes.ncm.clone(),
            depois.ncm.clone(),
            antes.fiscal.para_blob(),
            depois.fiscal.para_blob(),
        ],
    )
    .map_err(persist)?;
    Ok(())
}

/// O cabeçalho de uma alteração.
#[derive(Debug, Clone)]
pub struct LoteRegistrado {
    /// A alteração.
    pub id: Id,
    /// Quem aplicou.
    pub usuario: Id,
    /// Quando.
    pub criado_em: Instante,
    /// O que o usuário escreveu.
    pub descricao: String,
    /// Os campos mexidos.
    pub campos: String,
    /// Quantos produtos mudaram.
    pub quantidade: u32,
    /// Quando foi desfeita, se foi.
    pub desfeito_em: Option<Instante>,
}

fn lote_de_linha(r: &rusqlite::Row<'_>) -> rusqlite::Result<LoteRegistrado> {
    Ok(LoteRegistrado {
        id: id_de(r.get::<_, Vec<u8>>(0)?),
        usuario: id_de(r.get::<_, Vec<u8>>(1)?),
        criado_em: Instante::de_micros(r.get(2)?),
        descricao: r.get(3)?,
        campos: r.get(4)?,
        quantidade: u32::try_from(r.get::<_, i64>(5)?).unwrap_or(0),
        desfeito_em: r.get::<_, Option<i64>>(6)?.map(Instante::de_micros),
    })
}

const COLUNAS_LOTE: &str = "id, usuario, criado_em, descricao, campos, quantidade, desfeito_em";

/// As alterações mais recentes, da mais nova para a mais antiga.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn lotes_recentes(
    conn: &Connection,
    organizacao: Id,
    limite: u32,
) -> Resultado<Vec<LoteRegistrado>> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {COLUNAS_LOTE} FROM estoque_alteracao_lote
             WHERE empresa = ?1 ORDER BY criado_em DESC, rowid DESC LIMIT ?2"
        ))
        .map_err(persist)?;
    let linhas = stmt
        .query_map(params![blob(organizacao), limite], lote_de_linha)
        .map_err(persist)?;
    linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)
}

/// Uma alteração pelo id.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn lote_por_id(
    conn: &Connection,
    organizacao: Id,
    id: Id,
) -> Resultado<Option<LoteRegistrado>> {
    conn.query_row(
        &format!(
            "SELECT {COLUNAS_LOTE} FROM estoque_alteracao_lote WHERE empresa = ?1 AND id = ?2"
        ),
        params![blob(organizacao), blob(id)],
        lote_de_linha,
    )
    .optional()
    .map_err(persist)
}

/// A última alteração que ainda não foi desfeita.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn ultimo_lote_nao_desfeito(conn: &Connection, organizacao: Id) -> Resultado<Option<Id>> {
    conn.query_row(
        "SELECT id FROM estoque_alteracao_lote
         WHERE empresa = ?1 AND desfeito_em IS NULL ORDER BY criado_em DESC, rowid DESC LIMIT 1",
        [blob(organizacao)],
        |r| r.get::<_, Vec<u8>>(0),
    )
    .optional()
    .map(|o| o.map(id_de))
    .map_err(persist)
}

/// (produto, antes, depois) de cada produto da alteração.
pub type ItemDoLote = (Id, ValoresProduto, ValoresProduto);

/// O antes e o depois de cada produto da alteração.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn itens_do_lote(conn: &Connection, lote: Id) -> Resultado<Vec<ItemDoLote>> {
    let mut stmt = conn
        .prepare(
            "SELECT produto, antes_grupo, antes_margem, antes_ativo,
                    depois_grupo, depois_margem, depois_ativo,
                    antes_ncm, depois_ncm, antes_fiscal, depois_fiscal
             FROM estoque_alteracao_lote_item WHERE lote = ?1",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map([blob(lote)], |r| {
            Ok((
                id_de(r.get::<_, Vec<u8>>(0)?),
                ValoresProduto {
                    grupo: id_de(r.get::<_, Vec<u8>>(1)?),
                    margem: r.get::<_, Option<i64>>(2)?.map(Percentual::unidades),
                    ativo: r.get::<_, i64>(3)? != 0,
                    ncm: r.get::<_, Option<String>>(7)?.unwrap_or_default(),
                    fiscal: CamposFiscais::de_blob(r.get::<_, Option<Vec<u8>>>(9)?.as_deref()),
                },
                ValoresProduto {
                    grupo: id_de(r.get::<_, Vec<u8>>(4)?),
                    margem: r.get::<_, Option<i64>>(5)?.map(Percentual::unidades),
                    ativo: r.get::<_, i64>(6)? != 0,
                    ncm: r.get::<_, Option<String>>(8)?.unwrap_or_default(),
                    fiscal: CamposFiscais::de_blob(r.get::<_, Option<Vec<u8>>>(10)?.as_deref()),
                },
            ))
        })
        .map_err(persist)?;
    linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)
}

/// Marca a alteração como desfeita.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn marcar_desfeito(conn: &Connection, lote: Id, em: Instante) -> Resultado<()> {
    conn.execute(
        "UPDATE estoque_alteracao_lote SET desfeito_em = ?2 WHERE id = ?1",
        params![blob(lote), em.em_micros()],
    )
    .map_err(persist)?;
    Ok(())
}
