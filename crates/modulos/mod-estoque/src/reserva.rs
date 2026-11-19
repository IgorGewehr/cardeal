//! **Reserva por origem**: uma quantidade que está no local, mas já tem dono — a peça que chegou
//! pela nota do pedido para uma OS. Sai do disponível (o balcão e o PDV não a veem para vender)
//! e fica em `quantidade_reservada` do saldo, com a linha em `estoque_reserva` dizendo de quem é.
//!
//! Quem reserva é outro módulo, na mesma transação (`mod-compras` na confirmação da nota); quem
//! consome é o dono (`mod-os` ao aplicar: libera e dá a saída no mesmo local). Cada reserva e
//! cada liberação deixam um movimento (`Reserva`/`LiberacaoReserva`) com a origem — o rastro.
//! Movimentos de reserva não mexem no custo médio nem contam como consumo.

#[cfg(feature = "sqlite")]
use cardeal_kernel::{Erro, Resultado};
use cardeal_kernel::{Id, Quantidade};
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
#[cfg(feature = "sqlite")]
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// O que reservar, e para quem.
#[derive(Debug, Clone, Copy)]
pub struct DadosReserva {
    /// O produto.
    pub produto: Id,
    /// O local onde a mercadoria está.
    pub local: Id,
    /// Quanto reservar (reserva-se até o disponível do local).
    pub quantidade: Quantidade,
    /// O dono da reserva (`"os_peca"`: o item de peça de uma OS).
    pub origem_modulo: &'static str,
    /// O agregado dono.
    pub origem_id: Id,
}

/// Uma reserva em vigor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReservaAtiva {
    /// O agregado dono.
    pub origem_id: Id,
    /// O produto.
    pub produto: Id,
    /// O local.
    pub local: Id,
    /// Quanto está reservado.
    pub quantidade: Quantidade,
}

/// Reserva para a origem o que couber no disponível do local (nunca deixa o disponível
/// negativo). Devolve quanto reservou — zero se o local não tem disponível.
///
/// # Errors
/// Local travado por inventário em contagem; erro do SQLite.
#[cfg(feature = "sqlite")]
pub fn reservar_para_origem_comum(
    dados: DadosReserva,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<Quantidade> {
    use crate::repositorio::{blob, persist, RepositorioEstoque};
    use crate::saldo::TipoMovimento;

    if !dados.quantidade.e_positiva() {
        return Ok(Quantidade::ZERO);
    }
    crate::repositorio_inventario::exigir_local_livre(uow.conexao(), dados.local)?;
    let mut repo = RepositorioEstoque::novo(uow);
    let Some(mut saldo) = repo.buscar_saldo(dados.produto, dados.local)? else {
        return Ok(Quantidade::ZERO);
    };
    let quantidade = dados.quantidade.min(saldo.quantidade_disponivel);
    if !quantidade.e_positiva() {
        return Ok(Quantidade::ZERO);
    }
    saldo
        .reservar(quantidade, ctx.agora)
        .map_err(|e| Erro::de_dominio(&e))?;
    repo.atualizar_saldo(&saldo)?;
    movimento(&mut repo, &dados, quantidade, TipoMovimento::Reserva, ctx)?;
    uow.conexao()
        .execute(
            "INSERT INTO estoque_reserva
               (empresa, produto, local, origem_modulo, origem_id, quantidade, criada_em)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT (origem_modulo, origem_id, produto, local)
             DO UPDATE SET quantidade = quantidade + excluded.quantidade",
            params![
                blob(ctx.empresa),
                blob(dados.produto),
                blob(dados.local),
                dados.origem_modulo,
                blob(dados.origem_id),
                quantidade.unidades_internas(),
                ctx.agora.em_micros(),
            ],
        )
        .map_err(persist)?;
    Ok(quantidade)
}

/// Desfaz toda reserva da origem: volta ao disponível do local. Devolve o que liberou (vazio se
/// não havia reserva).
///
/// # Errors
/// Local travado por inventário em contagem; erro do SQLite.
#[cfg(feature = "sqlite")]
pub fn liberar_reservas_da_origem_comum(
    origem_modulo: &'static str,
    origem_id: Id,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<Vec<ReservaAtiva>> {
    use crate::repositorio::{blob, persist, RepositorioEstoque};
    use crate::saldo::TipoMovimento;

    let reservas = reservas_das_origens(uow.conexao(), ctx.empresa, origem_modulo, &[origem_id])?;
    for r in &reservas {
        crate::repositorio_inventario::exigir_local_livre(uow.conexao(), r.local)?;
        let mut repo = RepositorioEstoque::novo(uow);
        let Some(mut saldo) = repo.buscar_saldo(r.produto, r.local)? else {
            continue;
        };
        // Defesa contra base inconsistente: nunca libera mais do que o saldo tem reservado.
        let quantidade = r.quantidade.min(saldo.quantidade_reservada);
        if quantidade.e_positiva() {
            saldo
                .liberar_reserva(quantidade, ctx.agora)
                .map_err(|e| Erro::de_dominio(&e))?;
            repo.atualizar_saldo(&saldo)?;
            let dados = DadosReserva {
                produto: r.produto,
                local: r.local,
                quantidade,
                origem_modulo,
                origem_id,
            };
            movimento(
                &mut repo,
                &dados,
                quantidade,
                TipoMovimento::LiberacaoReserva,
                ctx,
            )?;
        }
    }
    uow.conexao()
        .execute(
            "DELETE FROM estoque_reserva
             WHERE empresa = ?1 AND origem_modulo = ?2 AND origem_id = ?3",
            params![blob(ctx.empresa), origem_modulo, blob(origem_id)],
        )
        .map_err(persist)?;
    Ok(reservas)
}

/// As reservas em vigor das origens (`origem_modulo`, um de `origens`).
///
/// # Errors
/// Erro do SQLite.
#[cfg(feature = "sqlite")]
pub fn reservas_das_origens(
    conexao: &Connection,
    empresa: Id,
    origem_modulo: &str,
    origens: &[Id],
) -> Resultado<Vec<ReservaAtiva>> {
    use crate::repositorio::{blob, id_de, persist};

    if origens.is_empty() {
        return Ok(Vec::new());
    }
    let marcas = vec!["?"; origens.len()].join(",");
    let sql = format!(
        "SELECT origem_id, produto, local, quantidade FROM estoque_reserva
         WHERE empresa = ? AND origem_modulo = ? AND origem_id IN ({marcas})
         ORDER BY criada_em"
    );
    let mut valores: Vec<Box<dyn rusqlite::ToSql>> =
        vec![Box::new(blob(empresa)), Box::new(origem_modulo.to_owned())];
    valores.extend(
        origens
            .iter()
            .map(|&o| Box::new(blob(o)) as Box<dyn rusqlite::ToSql>),
    );
    let mut st = conexao.prepare(&sql).map_err(persist)?;
    let v = st
        .query_map(rusqlite::params_from_iter(valores.iter()), |r| {
            Ok(ReservaAtiva {
                origem_id: id_de(r.get(0)?),
                produto: id_de(r.get(1)?),
                local: id_de(r.get(2)?),
                quantidade: Quantidade::interna(r.get(3)?),
            })
        })
        .map_err(persist)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist);
    v
}

#[cfg(feature = "sqlite")]
fn movimento(
    repo: &mut crate::repositorio::RepositorioEstoque<'_, '_>,
    dados: &DadosReserva,
    quantidade: Quantidade,
    tipo: crate::saldo::TipoMovimento,
    ctx: &Ctx,
) -> Resultado<()> {
    repo.inserir_movimento(&crate::saldo::Movimento {
        id: Id::novo(),
        empresa: ctx.empresa,
        produto: dados.produto,
        variacao: None,
        local: dados.local,
        tipo,
        quantidade,
        custo_unitario: None,
        lote: None,
        origem_modulo: dados.origem_modulo.to_owned(),
        origem_id: Some(dados.origem_id),
        lancamento: None,
        criado_em: ctx.agora,
        criado_por: ctx.usuario,
    })
}
