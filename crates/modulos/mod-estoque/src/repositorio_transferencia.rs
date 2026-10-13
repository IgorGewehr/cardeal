//! SQL da transferência entre locais: o documento (`estoque_transferencia` e seus itens, para
//! o histórico), a checagem de lote ativo e o saldo de um produto em cada local. Funções
//! livres sobre `&Connection`.

use cardeal_kernel::{Arredondamento, Dinheiro, Id, Instante, Preco, Quantidade, Resultado};
use rusqlite::{params, Connection, OptionalExtension};

use crate::repositorio::{blob, id_de, persist};
use crate::transferencia::Transferencia;

/// Grava o cabeçalho da transferência.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn inserir(
    conn: &Connection,
    t: &Transferencia,
    criado_por: Id,
    criado_em: Instante,
) -> Resultado<()> {
    conn.execute(
        "INSERT INTO estoque_transferencia
            (id, empresa, origem, destino, observacao, criado_por, criado_em)
         VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![
            blob(t.id),
            blob(t.empresa),
            blob(t.origem),
            blob(t.destino),
            t.observacao,
            blob(criado_por),
            criado_em.em_micros(),
        ],
    )
    .map_err(persist)?;
    Ok(())
}

/// Grava um item, com o custo unitário que ele levou da origem.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn inserir_item(
    conn: &Connection,
    transferencia: Id,
    produto: Id,
    quantidade: Quantidade,
    custo_unitario: Preco,
) -> Resultado<()> {
    conn.execute(
        "INSERT INTO estoque_transferencia_item (transferencia, produto, quantidade, custo_unitario)
         VALUES (?1,?2,?3,?4)",
        params![
            blob(transferencia),
            blob(produto),
            quantidade.unidades_internas(),
            custo_unitario.unidades_internas(),
        ],
    )
    .map_err(persist)?;
    Ok(())
}

/// O produto tem peça com lote ativo neste local?
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn tem_lote_ativo(conn: &Connection, produto: Id, local: Id) -> Resultado<bool> {
    conn.query_row(
        "SELECT 1 FROM estoque_lote WHERE produto = ?1 AND local = ?2 AND estado = 'Ativo' LIMIT 1",
        params![blob(produto), blob(local)],
        |_| Ok(()),
    )
    .optional()
    .map(|o| o.is_some())
    .map_err(persist)
}

/// Um item de uma transferência registrada.
#[derive(Debug, Clone)]
pub struct LinhaItem {
    /// O produto.
    pub produto: Id,
    /// O nome.
    pub nome: String,
    /// A sigla da unidade.
    pub unidade: String,
    /// A quantidade.
    pub quantidade: Quantidade,
    /// O valor ao custo da origem.
    pub valor: Dinheiro,
}

/// Uma transferência registrada.
#[derive(Debug, Clone)]
pub struct LinhaTransferencia {
    /// A transferência.
    pub id: Id,
    /// Nome do local de origem.
    pub origem: String,
    /// Nome do local de destino.
    pub destino: String,
    /// A observação.
    pub observacao: Option<String>,
    /// Quando.
    pub criado_em: Instante,
    /// Os itens.
    pub itens: Vec<LinhaItem>,
}

/// As transferências da empresa, da mais recente para a mais antiga.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn listar(conn: &Connection, empresa: Id, limite: u32) -> Resultado<Vec<LinhaTransferencia>> {
    let mut stmt = conn
        .prepare(
            "SELECT t.id, COALESCE(o.nome, ''), COALESCE(d.nome, ''), t.observacao, t.criado_em
             FROM estoque_transferencia t
             LEFT JOIN estoque_local o ON o.id = t.origem
             LEFT JOIN estoque_local d ON d.id = t.destino
             WHERE t.empresa = ?1
             ORDER BY t.criado_em DESC, t.rowid DESC LIMIT ?2",
        )
        .map_err(persist)?;
    let mut saida = stmt
        .query_map(params![blob(empresa), limite], |r| {
            Ok(LinhaTransferencia {
                id: id_de(r.get::<_, Vec<u8>>(0)?),
                origem: r.get(1)?,
                destino: r.get(2)?,
                observacao: r.get(3)?,
                criado_em: Instante::de_micros(r.get(4)?),
                itens: Vec::new(),
            })
        })
        .map_err(persist)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)?;
    let mut itens = conn
        .prepare(
            "SELECT i.produto, p.nome, COALESCE(u.sigla, ''), i.quantidade, i.custo_unitario
             FROM estoque_transferencia_item i
             JOIN estoque_produto p ON p.id = i.produto
             LEFT JOIN estoque_unidade u ON u.id = p.unidade_padrao
             WHERE i.transferencia = ?1 ORDER BY i.rowid",
        )
        .map_err(persist)?;
    for t in &mut saida {
        t.itens = itens
            .query_map([blob(t.id)], |r| {
                let quantidade = Quantidade::interna(r.get(3)?);
                let custo = Preco::interna(r.get(4)?);
                Ok(LinhaItem {
                    produto: id_de(r.get::<_, Vec<u8>>(0)?),
                    nome: r.get(1)?,
                    unidade: r.get(2)?,
                    quantidade,
                    valor: Dinheiro::de_total(quantidade, custo, Arredondamento::MeioAcima),
                })
            })
            .map_err(persist)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)?;
    }
    Ok(saida)
}

/// O disponível de um produto em cada local ativo da empresa (zero onde não há saldo).
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn saldos_por_local(
    conn: &Connection,
    empresa: Id,
    produto: Id,
) -> Resultado<Vec<(Id, String, Quantidade)>> {
    let mut stmt = conn
        .prepare(
            "SELECT l.id, l.nome, COALESCE(s.quantidade_disponivel, 0)
             FROM estoque_local l
             LEFT JOIN estoque_saldo_local s
                    ON s.local = l.id AND s.produto = ?2 AND s.variacao = X''
             WHERE l.empresa = ?1 AND l.ativo = 1
             ORDER BY l.nome ASC, l.id ASC",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map(params![blob(empresa), blob(produto)], |r| {
            Ok((
                id_de(r.get::<_, Vec<u8>>(0)?),
                r.get::<_, String>(1)?,
                Quantidade::interna(r.get(2)?),
            ))
        })
        .map_err(persist)?;
    linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)
}
