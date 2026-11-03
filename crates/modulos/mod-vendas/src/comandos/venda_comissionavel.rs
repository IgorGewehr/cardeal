//! A **venda comissionável** de outro módulo — a venda do PDV com vendedor. O PDV a registra ao
//! finalizar (com o percentual de comissão de cada produto congelado ali, como o faturamento do
//! pedido faz) e a estorna junto com a venda; a apuração do mês, o pagamento e as comissões do mês
//! tratam ela como um pedido (`vendas_comissao.origem = 'pdv'`).
//!
//! Estornada antes da apuração, ela sai da apuração; depois, a comissão é estornada (D Comissões
//! a pagar / C Despesas comerciais) e descontada no próximo pagamento — o mesmo caminho da
//! devolução de venda.

use cardeal_kernel::{Data, Dinheiro, Id};
use cardeal_kernel::{Erro, Resultado};
use cardeal_modkit::Ctx;
use cardeal_storage::UnidadeDeTrabalho;

use crate::repositorio::{blob, dias, id_de, persist};

/// Registra uma venda comissionável: `itens` são (produto, total do item), os cancelados já
/// fora. Venda sem itens não registra nada.
///
/// # Errors
/// Os do SQLite e da consulta dos percentuais.
pub fn registrar_venda_comissionavel_comum(
    origem: &'static str,
    origem_id: Id,
    vendedor: Id,
    data: Data,
    itens: &[(Id, Dinheiro)],
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<()> {
    if itens.is_empty() {
        return Ok(());
    }
    let produtos: Vec<Id> = itens.iter().map(|(p, _)| *p).collect();
    let sugestoes = mod_estoque::sugerir_precos(uow.conexao(), ctx, None, Some(&produtos))?;
    let c = uow.conexao();
    let id = Id::novo();
    c.execute(
        "INSERT INTO vendas_venda_comissionavel
           (id, empresa, origem, origem_id, vendedor, data, estado, criado_em)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'Valida', ?7)",
        rusqlite::params![
            blob(id),
            blob(ctx.empresa),
            origem,
            blob(origem_id),
            blob(vendedor),
            dias(data),
            ctx.agora.em_micros()
        ],
    )
    .map_err(persist)?;
    for (produto, total) in itens {
        let percentual = sugestoes
            .iter()
            .find(|s| s.produto == *produto)
            .map(|s| s.comissao.unidades_internas());
        c.execute(
            "INSERT INTO vendas_venda_comissionavel_item (venda, produto, total, comissao_percentual)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![blob(id), blob(*produto), total.em_centavos(), percentual],
        )
        .map_err(persist)?;
    }
    Ok(())
}

/// Estorna a venda comissionável: fora da apuração, ou, já apurada, a comissão estornada
/// (descontada no próximo pagamento do vendedor). Devolve a comissão estornada (zero se ainda
/// não tinha sido apurada, ou se a venda não era comissionável).
///
/// # Errors
/// Os do SQLite e do Razão.
pub fn estornar_venda_comissionavel_comum(
    origem: &'static str,
    origem_id: Id,
    data: Data,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<Dinheiro> {
    use rusqlite::OptionalExtension;
    let c = uow.conexao();
    let mudou = c
        .execute(
            "UPDATE vendas_venda_comissionavel SET estado = 'Estornada'
             WHERE empresa = ?1 AND origem = ?2 AND origem_id = ?3 AND estado = 'Valida'",
            rusqlite::params![blob(ctx.empresa), origem, blob(origem_id)],
        )
        .map_err(persist)?;
    if mudou == 0 {
        return Ok(Dinheiro::ZERO);
    }
    let apurada: Option<(Vec<u8>, i64, i64)> = c
        .query_row(
            "SELECT vendedor, base_calculo, valor FROM vendas_comissao
             WHERE pedido = ?1 AND estado <> 'Cancelada'",
            [blob(origem_id)],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(persist)?;
    let Some((vendedor, base, valor)) = apurada else {
        return Ok(Dinheiro::ZERO);
    };
    let (vendedor, valor) = (id_de(vendedor), Dinheiro::centavos(valor));
    if !valor.e_positivo() {
        return Ok(Dinheiro::ZERO);
    }
    let contas = super::contas_de_comissao(ctx, uow)?;
    let lanc = crate::receituario::estornar_comissao(
        ctx.empresa,
        origem_id,
        vendedor,
        valor,
        data,
        contas,
        super::autoria_de(ctx),
    )
    .map_err(|e| Erro::de_dominio(&e))?;
    let lancamento = {
        let mut repo = cardeal_ledger::RepositorioRazao::novo(uow);
        cardeal_ledger::Razao::registrar(&mut repo, lanc).map_err(|e| Erro::de_dominio(&e))?
    };
    // O "pedido" e o que estornou (a coluna `devolucao`) são o próprio cupom.
    uow.conexao()
        .execute(
            "INSERT INTO vendas_comissao_ajuste
               (id, empresa, vendedor, pedido, devolucao, base, valor, data, estado,
                lancamento_apuracao, criado_em)
             VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?6, ?7, 'Apurada', ?8, ?9)",
            rusqlite::params![
                blob(Id::novo()),
                blob(ctx.empresa),
                blob(vendedor),
                blob(origem_id),
                base,
                valor.em_centavos(),
                dias(data),
                blob(lancamento),
                ctx.agora.em_micros()
            ],
        )
        .map_err(persist)?;
    Ok(valor)
}
