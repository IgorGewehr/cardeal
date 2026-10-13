//! SQL do inventário: o cabeçalho (`estoque_inventario`), os itens da contagem
//! (`estoque_contagem_item`), a foto do saldo de um local e a trava que o inventário em
//! contagem impõe ao local. Funções livres sobre `&Connection`.

use cardeal_kernel::{CodigoErro, Erro, Id, Instante, Quantidade, Resultado};
use rusqlite::{params, Connection, OptionalExtension};

use crate::erros::ErroEstoque;
use crate::inventario::{ContagemItem, EstadoInventario, Inventario};
use crate::repositorio::{blob, id_de, persist};

fn estado_txt(e: EstadoInventario) -> &'static str {
    e.rotulo()
}

fn estado_de(t: &str) -> EstadoInventario {
    match t {
        "Planejado" => EstadoInventario::Planejado,
        "EmContagem" => EstadoInventario::EmContagem,
        "Congelado" => EstadoInventario::Congelado,
        "Conferido" => EstadoInventario::Conferido,
        "Encerrado" => EstadoInventario::Encerrado,
        _ => EstadoInventario::Cancelado,
    }
}

/// O cabeçalho que não está no domínio: o recorte da contagem e quando foi criado.
#[derive(Debug, Clone, Copy)]
pub struct Recorte {
    /// O grupo da estrutura (a subárvore inteira entra); `None` = o local inteiro.
    pub grupo: Option<Id>,
    /// Quando o inventário foi criado.
    pub criado_em: Instante,
}

/// Recusa movimento num local com inventário em contagem (§11.4: a contagem não persegue um
/// alvo que se move).
///
/// # Errors
/// [`ErroEstoque::LocalCongelado`] (`RECURSO_TRAVADO`), ou erro do SQLite.
pub fn exigir_local_livre(conn: &Connection, local: Id) -> Resultado<()> {
    let travado = conn
        .query_row(
            "SELECT 1 FROM estoque_inventario WHERE local = ?1 AND estado = 'Congelado' LIMIT 1",
            [blob(local)],
            |_| Ok(()),
        )
        .optional()
        .map_err(persist)?
        .is_some();
    if travado {
        return Err(Erro::de_dominio(&ErroEstoque::LocalCongelado));
    }
    Ok(())
}

/// O inventário em aberto (não encerrado nem cancelado) do local, se houver.
///
/// # Errors
/// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn aberto_no_local(conn: &Connection, local: Id) -> Resultado<Option<Id>> {
    conn.query_row(
        "SELECT id FROM estoque_inventario
         WHERE local = ?1 AND estado NOT IN ('Encerrado','Cancelado') LIMIT 1",
        [blob(local)],
        |r| r.get::<_, Vec<u8>>(0),
    )
    .optional()
    .map(|o| o.map(id_de))
    .map_err(persist)
}

/// Grava um inventário recém-criado (ainda sem itens).
///
/// # Errors
/// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn inserir(conn: &Connection, inv: &Inventario, recorte: Recorte) -> Resultado<()> {
    conn.execute(
        "INSERT INTO estoque_inventario
            (id, empresa, local, descricao, grupo, estado, criado_por, criado_em)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![
            blob(inv.id),
            blob(inv.empresa),
            blob(inv.local),
            inv.descricao,
            recorte.grupo.map(blob),
            estado_txt(inv.estado),
            blob(inv.criado_por),
            recorte.criado_em.em_micros(),
        ],
    )
    .map_err(persist)?;
    Ok(())
}

/// Regrava o estado e as datas do cabeçalho.
///
/// # Errors
/// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn atualizar_cabecalho(conn: &Connection, inv: &Inventario) -> Resultado<()> {
    conn.execute(
        "UPDATE estoque_inventario
         SET estado = ?2, iniciado_em = ?3, encerrado_em = ?4, versao = versao + 1
         WHERE id = ?1",
        params![
            blob(inv.id),
            estado_txt(inv.estado),
            inv.iniciado_em.map(Instante::em_micros),
            inv.encerrado_em.map(Instante::em_micros),
        ],
    )
    .map_err(persist)?;
    Ok(())
}

/// Grava um item da contagem.
///
/// # Errors
/// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn inserir_item(conn: &Connection, inventario: Id, i: &ContagemItem) -> Resultado<()> {
    conn.execute(
        "INSERT INTO estoque_contagem_item
            (id, inventario, produto, quantidade_sistema, quantidade_contada, divergencia,
             contado_por, contado_em)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![
            blob(i.id),
            blob(inventario),
            blob(i.produto),
            i.quantidade_sistema.unidades_internas(),
            i.quantidade_contada.map(Quantidade::unidades_internas),
            i.divergencia.map(Quantidade::unidades_internas),
            i.contado_por.map(blob),
            i.contado_em.map(Instante::em_micros),
        ],
    )
    .map_err(persist)?;
    Ok(())
}

/// Regrava a contagem de um item.
///
/// # Errors
/// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn atualizar_item(conn: &Connection, i: &ContagemItem) -> Resultado<()> {
    conn.execute(
        "UPDATE estoque_contagem_item
         SET quantidade_contada = ?2, divergencia = ?3, contado_por = ?4, contado_em = ?5
         WHERE id = ?1",
        params![
            blob(i.id),
            i.quantidade_contada.map(Quantidade::unidades_internas),
            i.divergencia.map(Quantidade::unidades_internas),
            i.contado_por.map(blob),
            i.contado_em.map(Instante::em_micros),
        ],
    )
    .map_err(persist)?;
    Ok(())
}

/// Guarda o valor do ajuste (ao custo médio) do item de um produto, no encerramento.
///
/// # Errors
/// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn gravar_valor_ajuste(
    conn: &Connection,
    inventario: Id,
    produto: Id,
    valor: cardeal_kernel::Dinheiro,
) -> Resultado<()> {
    conn.execute(
        "UPDATE estoque_contagem_item SET valor_ajuste = ?3 WHERE inventario = ?1 AND produto = ?2",
        params![blob(inventario), blob(produto), valor.em_centavos()],
    )
    .map_err(persist)?;
    Ok(())
}

/// O inventário inteiro (cabeçalho e itens) e o recorte; `None` se não é desta empresa.
///
/// # Errors
/// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn buscar(conn: &Connection, empresa: Id, id: Id) -> Resultado<Option<(Inventario, Recorte)>> {
    let cabecalho = conn
        .query_row(
            "SELECT local, descricao, grupo, estado, criado_por, criado_em, iniciado_em,
                    encerrado_em
             FROM estoque_inventario WHERE empresa = ?1 AND id = ?2",
            params![blob(empresa), blob(id)],
            |r| {
                Ok((
                    Inventario {
                        id,
                        empresa,
                        local: id_de(r.get::<_, Vec<u8>>(0)?),
                        descricao: r.get(1)?,
                        estado: estado_de(&r.get::<_, String>(3)?),
                        iniciado_em: r.get::<_, Option<i64>>(6)?.map(Instante::de_micros),
                        encerrado_em: r.get::<_, Option<i64>>(7)?.map(Instante::de_micros),
                        criado_por: id_de(r.get::<_, Vec<u8>>(4)?),
                        itens: Vec::new(),
                    },
                    Recorte {
                        grupo: r.get::<_, Option<Vec<u8>>>(2)?.map(id_de),
                        criado_em: Instante::de_micros(r.get(5)?),
                    },
                ))
            },
        )
        .optional()
        .map_err(persist)?;
    let Some((mut inv, recorte)) = cabecalho else {
        return Ok(None);
    };
    let mut stmt = conn
        .prepare(
            "SELECT id, produto, quantidade_sistema, quantidade_contada, divergencia,
                    contado_por, contado_em
             FROM estoque_contagem_item WHERE inventario = ?1 ORDER BY rowid",
        )
        .map_err(persist)?;
    inv.itens = stmt
        .query_map([blob(id)], |r| {
            Ok(ContagemItem {
                id: id_de(r.get::<_, Vec<u8>>(0)?),
                produto: id_de(r.get::<_, Vec<u8>>(1)?),
                variacao: None,
                lote: None,
                quantidade_sistema: Quantidade::interna(r.get(2)?),
                quantidade_contada: r.get::<_, Option<i64>>(3)?.map(Quantidade::interna),
                divergencia: r.get::<_, Option<i64>>(4)?.map(Quantidade::interna),
                contado_por: r.get::<_, Option<Vec<u8>>>(5)?.map(id_de),
                contado_em: r.get::<_, Option<i64>>(6)?.map(Instante::de_micros),
            })
        })
        .map_err(persist)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)?;
    Ok(Some((inv, recorte)))
}

/// A foto do saldo do local: cada produto ativo com saldo registrado ali (mesmo zerado),
/// restrito aos `produtos` do recorte quando informado. Ordem por nome.
///
/// # Errors
/// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn foto_do_local(
    conn: &Connection,
    organizacao: Id,
    local: Id,
    grupos: Option<&[Id]>,
) -> Resultado<Vec<(Id, Quantidade)>> {
    let mut stmt = conn
        .prepare(
            "SELECT s.produto, s.quantidade_disponivel, p.grupo_produto
             FROM estoque_saldo_local s
             JOIN estoque_produto p ON p.id = s.produto
             WHERE s.local = ?1 AND s.variacao = X'' AND p.empresa = ?2 AND p.ativo = 1
             ORDER BY p.nome ASC, p.id ASC",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map(params![blob(local), blob(organizacao)], |r| {
            Ok((
                id_de(r.get::<_, Vec<u8>>(0)?),
                Quantidade::interna(r.get(1)?),
                id_de(r.get::<_, Vec<u8>>(2)?),
            ))
        })
        .map_err(persist)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)?;
    Ok(linhas
        .into_iter()
        .filter(|(_, _, g)| grupos.is_none_or(|gs| gs.contains(g)))
        .map(|(p, q, _)| (p, q))
        .collect())
}

/// O disponível de um produto no local agora (zero sem saldo registrado).
///
/// # Errors
/// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn disponivel(conn: &Connection, produto: Id, local: Id) -> Resultado<Quantidade> {
    conn.query_row(
        "SELECT quantidade_disponivel FROM estoque_saldo_local
         WHERE produto = ?1 AND local = ?2 AND variacao = X''",
        params![blob(produto), blob(local)],
        |r| r.get::<_, i64>(0),
    )
    .optional()
    .map(|o| Quantidade::interna(o.unwrap_or(0)))
    .map_err(persist)
}

/// Uma linha da lista de inventários.
#[derive(Debug, Clone)]
pub struct LinhaInventario {
    /// O inventário.
    pub id: Id,
    /// O local.
    pub local: Id,
    /// O nome do local.
    pub nome_local: String,
    /// A descrição.
    pub descricao: String,
    /// O estado.
    pub estado: EstadoInventario,
    /// Quantos itens a contar.
    pub itens: u32,
    /// Quantos já contados.
    pub contados: u32,
    /// Criação, início e fim.
    pub criado_em: Instante,
    /// Quando a contagem começou.
    pub iniciado_em: Option<Instante>,
    /// Quando terminou (encerrado ou cancelado).
    pub encerrado_em: Option<Instante>,
}

/// Os inventários da empresa, do mais recente para o mais antigo.
///
/// # Errors
/// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn listar(conn: &Connection, empresa: Id, limite: u32) -> Resultado<Vec<LinhaInventario>> {
    let mut stmt = conn
        .prepare(
            "SELECT i.id, i.local, COALESCE(l.nome, ''), i.descricao, i.estado,
                    (SELECT COUNT(*) FROM estoque_contagem_item c WHERE c.inventario = i.id),
                    (SELECT COUNT(*) FROM estoque_contagem_item c
                      WHERE c.inventario = i.id AND c.quantidade_contada IS NOT NULL),
                    i.criado_em, i.iniciado_em, i.encerrado_em
             FROM estoque_inventario i LEFT JOIN estoque_local l ON l.id = i.local
             WHERE i.empresa = ?1
             ORDER BY i.criado_em DESC, i.rowid DESC LIMIT ?2",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map(params![blob(empresa), limite], |r| {
            Ok(LinhaInventario {
                id: id_de(r.get::<_, Vec<u8>>(0)?),
                local: id_de(r.get::<_, Vec<u8>>(1)?),
                nome_local: r.get(2)?,
                descricao: r.get(3)?,
                estado: estado_de(&r.get::<_, String>(4)?),
                itens: u32::try_from(r.get::<_, i64>(5)?).unwrap_or(0),
                contados: u32::try_from(r.get::<_, i64>(6)?).unwrap_or(0),
                criado_em: Instante::de_micros(r.get(7)?),
                iniciado_em: r.get::<_, Option<i64>>(8)?.map(Instante::de_micros),
                encerrado_em: r.get::<_, Option<i64>>(9)?.map(Instante::de_micros),
            })
        })
        .map_err(persist)?;
    linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)
}

/// O local existe e é desta empresa (CNPJ)?
///
/// # Errors
/// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn local_da_empresa(conn: &Connection, empresa: Id, local: Id) -> Resultado<bool> {
    conn.query_row(
        "SELECT 1 FROM estoque_local WHERE id = ?1 AND empresa = ?2 AND ativo = 1",
        params![blob(local), blob(empresa)],
        |_| Ok(()),
    )
    .optional()
    .map(|o| o.is_some())
    .map_err(persist)
}

/// Um inventário pelo id, ou `NAO_ENCONTRADO`.
///
/// # Errors
/// [`CodigoErro::NAO_ENCONTRADO`], ou erro do SQLite.
pub fn carregar(conn: &Connection, empresa: Id, id: Id) -> Resultado<(Inventario, Recorte)> {
    buscar(conn, empresa, id)?
        .ok_or_else(|| Erro::novo(CodigoErro::NAO_ENCONTRADO, "inventário não encontrado"))
}
