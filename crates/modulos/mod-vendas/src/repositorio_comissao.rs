//! A persistência da comissão do vendedor (`vendas_comissao`) e o cálculo do que falta apurar
//! num mês — o mesmo cálculo serve ao comando que apura e à consulta que mostra a prévia, para
//! a prévia nunca divergir do que vai ser lançado.

use std::collections::HashMap;

use cardeal_kernel::{Competencia, Data, Dinheiro, Id, Instante, Percentual, Resultado, Versao};
use cardeal_modkit::Ctx;
use rusqlite::{params, Connection};

use crate::comissao::{Comissao, EstadoComissao};
use crate::repositorio::{blob, data_de, dias, id_de, id_opt_de, persist};

/// A data em que a apuração de `competencia` é lançada: o último dia do mês, ou hoje se o mês
/// ainda não acabou — sempre dentro do mês, para a despesa cair na competência da venda.
#[must_use]
pub(crate) fn data_da_apuracao(competencia: Competencia, hoje: Data) -> Data {
    hoje.min(competencia.ultimo_dia())
        .max(competencia.primeiro_dia())
}

/// Uma linha do pedido para a apuração: (pedido, vendedor, produto, total do item, percentual
/// congelado no faturamento).
type LinhaApuracao = (Id, Id, Id, Data, i64, Option<i64>);

/// As comissões dos pedidos faturados em `competencia` (pela data do pedido, a mesma do
/// lançamento da receita) que ainda não foram apuradas, com a data de cada pedido. Item a
/// item: o percentual congelado no faturamento ou, nos pedidos faturados antes de existir o
/// congelamento, o que vale hoje para o produto. Pedido sem nada a pagar (tudo a 0%) fica de
/// fora.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub(crate) fn a_apurar(
    conn: &Connection,
    ctx: &Ctx,
    competencia: Competencia,
    hoje: Data,
) -> Resultado<Vec<(Comissao, Data)>> {
    let mut stmt = conn
        .prepare(
            "SELECT p.id, p.vendedor, i.produto, p.data,
                    i.total_item - COALESCE((
                        SELECT SUM(di.valor_estornado) FROM vendas_devolucao_item di
                        JOIN vendas_devolucao d ON d.id = di.devolucao
                        WHERE di.item_pedido = i.id AND d.estado = 'Concluida'), 0),
                    i.comissao_percentual
             FROM vendas_pedido p JOIN vendas_item_pedido i ON i.pedido = p.id
             WHERE p.empresa = ?1 AND p.estado IN ('Faturado','EmEntrega','Concluido')
               AND p.data BETWEEN ?2 AND ?3
               AND NOT EXISTS (SELECT 1 FROM vendas_comissao c WHERE c.pedido = p.id)
             UNION ALL
             SELECT v.origem_id, v.vendedor, vi.produto, v.data, vi.total, vi.comissao_percentual
             FROM vendas_venda_comissionavel v
             JOIN vendas_venda_comissionavel_item vi ON vi.venda = v.id
             WHERE v.empresa = ?1 AND v.estado = 'Valida' AND v.data BETWEEN ?2 AND ?3
               AND NOT EXISTS (SELECT 1 FROM vendas_comissao c WHERE c.pedido = v.origem_id)
             ORDER BY 4, 1",
        )
        .map_err(persist)?;
    let linhas: Vec<LinhaApuracao> = stmt
        .query_map(
            params![
                blob(ctx.empresa),
                dias(competencia.primeiro_dia()),
                dias(competencia.ultimo_dia())
            ],
            |r| {
                Ok((
                    id_de(r.get(0)?),
                    id_de(r.get(1)?),
                    id_de(r.get(2)?),
                    data_de(r.get(3)?),
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .map_err(persist)?
        .collect::<rusqlite::Result<_>>()
        .map_err(persist)?;

    let sem_congelado: Vec<Id> = linhas
        .iter()
        .filter(|l| l.5.is_none())
        .map(|l| l.2)
        .collect();
    let vigentes: HashMap<Id, Percentual> = if sem_congelado.is_empty() {
        HashMap::new()
    } else {
        mod_estoque::sugerir_precos(conn, ctx, None, Some(&sem_congelado))?
            .into_iter()
            .map(|s| (s.produto, s.comissao))
            .collect()
    };

    let data = data_da_apuracao(competencia, hoje);
    let mut saida = Vec::new();
    for pedido in linhas.chunk_by(|a, b| a.0 == b.0) {
        let (id, vendedor, _, data_pedido, _, _) = pedido[0];
        let itens: Vec<(Dinheiro, Percentual)> = pedido
            .iter()
            .map(|&(_, _, produto, _, total, congelado)| {
                let percentual = congelado.map_or_else(
                    || vigentes.get(&produto).copied().unwrap_or(Percentual::ZERO),
                    Percentual::unidades,
                );
                (Dinheiro::centavos(total), percentual)
            })
            .collect();
        if let Some(c) = Comissao::de_itens(ctx.empresa, vendedor, id, &itens, data) {
            saida.push((c, data_pedido));
        }
    }
    Ok(saida)
}

fn estado_txt(e: EstadoComissao) -> &'static str {
    match e {
        EstadoComissao::Apurada => "Apurada",
        EstadoComissao::Paga => "Paga",
        EstadoComissao::Cancelada => "Cancelada",
    }
}

fn estado_de(s: &str) -> EstadoComissao {
    match s {
        "Paga" => EstadoComissao::Paga,
        "Cancelada" => EstadoComissao::Cancelada,
        _ => EstadoComissao::Apurada,
    }
}

/// Grava uma comissão recém-apurada, com o lançamento da apuração.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite (inclusive pedido já
/// apurado — a coluna é única).
pub(crate) fn inserir(
    conn: &Connection,
    c: &Comissao,
    lancamento_apuracao: Id,
    agora: Instante,
) -> Resultado<()> {
    conn.execute(
        "INSERT INTO vendas_comissao
           (id, empresa, vendedor, pedido, base_calculo, percentual, valor, competencia, estado,
            lancamento_apuracao, lancamento, criado_em, origem)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,NULL,?11,
                 CASE WHEN EXISTS (SELECT 1 FROM vendas_pedido WHERE id = ?4)
                      THEN 'pedido' ELSE 'pdv' END)",
        params![
            blob(c.id),
            blob(c.empresa),
            blob(c.vendedor),
            blob(c.pedido),
            c.base_calculo.em_centavos(),
            c.percentual.unidades_internas(),
            c.valor.em_centavos(),
            dias(c.competencia),
            estado_txt(c.estado),
            blob(lancamento_apuracao),
            agora.em_micros(),
        ],
    )
    .map_err(persist)?;
    Ok(())
}

/// Regrava o estado e o lançamento de pagamento de uma comissão.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub(crate) fn atualizar_estado(conn: &Connection, c: &Comissao) -> Resultado<()> {
    conn.execute(
        "UPDATE vendas_comissao SET estado = ?2, lancamento = ?3 WHERE id = ?1",
        params![blob(c.id), estado_txt(c.estado), c.lancamento.map(blob)],
    )
    .map_err(persist)?;
    Ok(())
}

/// Um estorno de comissão por devolução: (id, vendedor, pedido, devolução, base, valor, data,
/// paga).
pub(crate) type Ajuste = (Id, Id, Id, Id, Dinheiro, Dinheiro, Data, bool);

/// Estorna a comissão da parte devolvida de um pedido cuja comissão já foi apurada: um ajuste
/// (D Comissões a pagar / C Despesas comerciais) que o próximo pagamento do vendedor desconta.
/// Pedido ainda não apurado não precisa: a apuração já sai sem a parte devolvida. Devolve o
/// valor estornado.
///
/// # Errors
/// Os do SQLite e do Razão.
pub(crate) fn estornar_da_devolucao(
    d: &crate::devolucao::Devolucao,
    pedido: &crate::pedido::Pedido,
    ctx: &Ctx,
    uow: &mut cardeal_storage::UnidadeDeTrabalho,
) -> Resultado<Dinheiro> {
    use rusqlite::OptionalExtension;
    let apurada: Option<(Vec<u8>, i64)> = uow
        .conexao()
        .query_row(
            "SELECT vendedor, percentual FROM vendas_comissao
             WHERE pedido = ?1 AND estado <> 'Cancelada'",
            [blob(pedido.id)],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(persist)?;
    let Some((vendedor, efetivo)) = apurada else {
        return Ok(Dinheiro::ZERO);
    };
    let vendedor = id_de(vendedor);
    // O percentual congelado de cada item (o efetivo da comissão, se não houver).
    let mut valor = Dinheiro::ZERO;
    for i in &d.itens {
        let congelado: Option<i64> = uow
            .conexao()
            .query_row(
                "SELECT comissao_percentual FROM vendas_item_pedido WHERE id = ?1",
                [blob(i.item_pedido)],
                |r| r.get(0),
            )
            .optional()
            .map_err(persist)?
            .flatten();
        let p = Percentual::unidades(congelado.unwrap_or(efetivo));
        valor += i
            .valor_estornado
            .aplicar(p, cardeal_kernel::Arredondamento::MeioAcima);
    }
    if !valor.e_positivo() {
        return Ok(Dinheiro::ZERO);
    }
    let contas = crate::comandos::contas_de_comissao(ctx, uow)?;
    let lanc = crate::receituario::estornar_comissao(
        ctx.empresa,
        d.id,
        vendedor,
        valor,
        d.data,
        contas,
        crate::comandos::autoria_de(ctx),
    )
    .map_err(|e| cardeal_kernel::Erro::de_dominio(&e))?;
    let lancamento = {
        let mut repo = cardeal_ledger::RepositorioRazao::novo(uow);
        cardeal_ledger::Razao::registrar(&mut repo, lanc)
            .map_err(|e| cardeal_kernel::Erro::de_dominio(&e))?
    };
    uow.conexao()
        .execute(
            "INSERT INTO vendas_comissao_ajuste
               (id, empresa, vendedor, pedido, devolucao, base, valor, data, estado,
                lancamento_apuracao, criado_em)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'Apurada', ?9, ?10)",
            params![
                blob(Id::novo()),
                blob(ctx.empresa),
                blob(vendedor),
                blob(pedido.id),
                blob(d.id),
                d.valor_total.em_centavos(),
                valor.em_centavos(),
                dias(d.data),
                blob(lancamento),
                ctx.agora.em_micros()
            ],
        )
        .map_err(persist)?;
    Ok(valor)
}

/// Os estornos de comissão por devolução: os do mês (pela data da devolução) e, com
/// `pendentes_ate`, também os ainda não descontados até aquele dia.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub(crate) fn ajustes(
    conn: &Connection,
    empresa: Id,
    competencia: Competencia,
    vendedor: Option<Id>,
    so_pendentes: bool,
) -> Resultado<Vec<Ajuste>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, vendedor, pedido, devolucao, base, valor, data, estado
             FROM vendas_comissao_ajuste
             WHERE empresa = ?1 AND (?4 IS NULL OR vendedor = ?4)
               AND CASE WHEN ?5 THEN estado = 'Apurada' AND data <= ?3
                        ELSE data BETWEEN ?2 AND ?3 END
             ORDER BY data, rowid",
        )
        .map_err(persist)?;
    let v = stmt
        .query_map(
            params![
                blob(empresa),
                dias(competencia.primeiro_dia()),
                dias(competencia.ultimo_dia()),
                vendedor.map(blob),
                so_pendentes
            ],
            |r| {
                Ok((
                    id_de(r.get(0)?),
                    id_de(r.get(1)?),
                    id_de(r.get(2)?),
                    id_de(r.get(3)?),
                    Dinheiro::centavos(r.get(4)?),
                    Dinheiro::centavos(r.get(5)?),
                    data_de(r.get(6)?),
                    r.get::<_, String>(7)? == "Paga",
                ))
            },
        )
        .map_err(persist)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist);
    v
}

/// Marca os estornos como descontados no pagamento.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub(crate) fn descontar_ajustes(conn: &Connection, ids: &[Id], lancamento: Id) -> Resultado<()> {
    for id in ids {
        conn.execute(
            "UPDATE vendas_comissao_ajuste SET estado = 'Paga', lancamento = ?2 WHERE id = ?1",
            params![blob(*id), blob(lancamento)],
        )
        .map_err(persist)?;
    }
    Ok(())
}

/// As comissões já apuradas de `competencia` (de um vendedor, ou de todos), com a data de cada
/// pedido, na ordem dos pedidos.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub(crate) fn do_mes(
    conn: &Connection,
    empresa: Id,
    competencia: Competencia,
    vendedor: Option<Id>,
) -> Resultado<Vec<(Comissao, Data)>> {
    let mut stmt = conn
        .prepare(
            "SELECT c.id, c.vendedor, c.pedido, c.base_calculo, c.percentual, c.valor,
                    c.competencia, c.estado, c.lancamento, COALESCE(p.data, v.data, c.competencia)
             FROM vendas_comissao c
             LEFT JOIN vendas_pedido p ON p.id = c.pedido
             LEFT JOIN vendas_venda_comissionavel v ON v.origem_id = c.pedido
             WHERE c.empresa = ?1 AND c.competencia BETWEEN ?2 AND ?3
               AND (?4 IS NULL OR c.vendedor = ?4)
             ORDER BY 10, c.rowid",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map(
            params![
                blob(empresa),
                dias(competencia.primeiro_dia()),
                dias(competencia.ultimo_dia()),
                vendedor.map(blob),
            ],
            |r| {
                let estado: String = r.get(7)?;
                Ok((
                    Comissao {
                        id: id_de(r.get(0)?),
                        empresa,
                        vendedor: id_de(r.get(1)?),
                        pedido: id_de(r.get(2)?),
                        base_calculo: Dinheiro::centavos(r.get(3)?),
                        percentual: Percentual::unidades(r.get(4)?),
                        valor: Dinheiro::centavos(r.get(5)?),
                        competencia: data_de(r.get(6)?),
                        estado: estado_de(&estado),
                        lancamento: id_opt_de(r.get(8)?),
                        versao: Versao::INICIAL,
                    },
                    data_de(r.get(9)?),
                ))
            },
        )
        .map_err(persist)?;
    linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn apuracao_cai_sempre_dentro_do_mes() {
        let set = Competencia::nova(2026, 9);
        // Mês já fechado: último dia.
        assert_eq!(
            data_da_apuracao(set, Data::de_ymd(2026, 10, 13).unwrap()),
            Data::de_ymd(2026, 9, 30).unwrap()
        );
        // Mês corrente: hoje.
        assert_eq!(
            data_da_apuracao(set, Data::de_ymd(2026, 9, 12).unwrap()),
            Data::de_ymd(2026, 9, 12).unwrap()
        );
        // Mês futuro (nada a apurar, mas a data não sai do mês).
        assert_eq!(
            data_da_apuracao(set, Data::de_ymd(2026, 8, 1).unwrap()),
            Data::de_ymd(2026, 9, 1).unwrap()
        );
    }
}
