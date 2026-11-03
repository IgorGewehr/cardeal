//! A persistência da cotação e a decisão que gera os pedidos.

use cardeal_kernel::{CodigoErro, Data, Dinheiro, Erro, Id, Preco, Quantidade, Resultado};
use cardeal_modkit::Ctx;
use cardeal_storage::UnidadeDeTrabalho;
use rusqlite::{params, Connection, OptionalExtension};

use super::{
    CancelarCotacao, Cotacao, CotacaoSalva, DecidirCotacao, EstadoCotacao, ItemCotacao,
    PropostaFornecedor, RegistrarPropostaFornecedor, ResumoCotacao, SalvarCotacao,
};
use crate::erros::ErroCompras;
use crate::pedido::{ItemDoPedido, PedidoSalvo, SalvarPedidoCompra, MOTIVO_MINIMO_PEDIDO};

#[allow(clippy::needless_pass_by_value)]
fn falha(e: rusqlite::Error) -> Erro {
    Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string())
}

fn id_de(b: &[u8]) -> Id {
    <[u8; 16]>::try_from(b).map_or(Id::NULO, Id::de_bytes)
}

fn b(id: Id) -> Vec<u8> {
    id.em_bytes().to_vec()
}

fn data_de(d: i64) -> Data {
    Data::de_dias(i32::try_from(d).unwrap_or(0))
}

fn exigir_aberta(cot: &Cotacao) -> Resultado<()> {
    if cot.estado == EstadoCotacao::Aberta {
        return Ok(());
    }
    Err(Erro::de_dominio(&ErroCompras::CotacaoFechada))
}

/// Confere que a pessoa existe e é fornecedor.
fn exigir_fornecedor(c: &Connection, fornecedor: Id) -> Resultado<()> {
    let p = mod_clientes::pessoa_por_id(c, fornecedor)?
        .ok_or_else(|| Erro::nao_encontrado("fornecedor"))?;
    if !p.tem_papel(mod_clientes::Papel::Fornecedor) {
        return Err(Erro::novo(
            CodigoErro::ENTRADA_INVALIDA,
            format!("{} não está cadastrado como fornecedor", p.nome),
        ));
    }
    Ok(())
}

pub(super) fn salvar(
    cmd: &SalvarCotacao,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<CotacaoSalva> {
    let itens: Vec<_> = cmd
        .itens
        .iter()
        .filter(|i| i.quantidade.e_positiva())
        .copied()
        .collect();
    if itens.is_empty() {
        return Err(Erro::de_dominio(&ErroCompras::CotacaoSemItens));
    }
    let mut fornecedores: Vec<Id> = Vec::new();
    for f in &cmd.fornecedores {
        if !fornecedores.contains(f) {
            fornecedores.push(*f);
        }
    }
    if fornecedores.is_empty() {
        return Err(Erro::de_dominio(&ErroCompras::CotacaoSemFornecedores));
    }
    let c = uow.conexao();
    for f in &fornecedores {
        exigir_fornecedor(c, *f)?;
    }
    let ids: Vec<Id> = itens.iter().map(|i| i.produto).collect();
    let produtos = mod_estoque::produtos_para_nota(c, ctx.organizacao, &ids)?;
    for (n, i) in itens.iter().enumerate() {
        let Some(p) = produtos.get(&i.produto) else {
            return Err(Erro::nao_encontrado("produto da cotação"));
        };
        if itens[..n].iter().any(|o| o.produto == i.produto) {
            return Err(Erro::de_dominio(&ErroCompras::ProdutoRepetido(
                p.nome.clone(),
            )));
        }
    }
    let (id, numero, antigos) = cabecalho(cmd, ctx, c)?;
    gravar_itens(c, id, &itens, &produtos, antigos.as_ref())?;
    if let Some(a) = &antigos {
        for velho in a
            .propostas
            .iter()
            .filter(|p| !fornecedores.contains(&p.fornecedor))
        {
            remover_fornecedor(c, id, velho.fornecedor)?;
        }
    }
    for (ordem, f) in fornecedores.iter().enumerate() {
        c.execute(
            "INSERT INTO compras_cotacao_fornecedor (cotacao, fornecedor, ordem)
             VALUES (?1, ?2, ?3)
             ON CONFLICT (cotacao, fornecedor) DO UPDATE SET ordem = excluded.ordem",
            params![b(id), b(*f), i64::try_from(ordem).unwrap_or(0)],
        )
        .map_err(falha)?;
    }
    Ok(CotacaoSalva {
        cotacao: id,
        numero,
    })
}

/// O cabeçalho: a cotação nova, ou a aberta atualizada (com o que ela tinha).
fn cabecalho(
    cmd: &SalvarCotacao,
    ctx: &Ctx,
    c: &Connection,
) -> Resultado<(Id, u32, Option<Cotacao>)> {
    Ok(match cmd.cotacao {
        None => {
            let id = Id::novo();
            let numero: i64 = c
                .query_row(
                    "SELECT COALESCE(MAX(numero), 0) + 1 FROM compras_cotacao WHERE empresa = ?1",
                    [b(ctx.empresa)],
                    |r| r.get(0),
                )
                .map_err(falha)?;
            c.execute(
                "INSERT INTO compras_cotacao
                   (id, empresa, numero, descricao, data, prazo_resposta, estado, criada_em,
                    criada_por)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'Aberta', ?7, ?8)",
                params![
                    b(id),
                    b(ctx.empresa),
                    numero,
                    cmd.descricao.trim(),
                    ctx.hoje().em_dias(),
                    cmd.prazo_resposta.map(Data::em_dias),
                    ctx.agora.em_micros(),
                    b(ctx.usuario),
                ],
            )
            .map_err(falha)?;
            (id, u32::try_from(numero).unwrap_or(0), None)
        }
        Some(id) => {
            let cot =
                cotacao(c, ctx.empresa, id)?.ok_or_else(|| Erro::nao_encontrado("cotação"))?;
            exigir_aberta(&cot)?;
            c.execute(
                "UPDATE compras_cotacao SET descricao = ?2, prazo_resposta = ?3 WHERE id = ?1",
                params![
                    b(id),
                    cmd.descricao.trim(),
                    cmd.prazo_resposta.map(Data::em_dias)
                ],
            )
            .map_err(falha)?;
            (id, cot.numero, Some(cot))
        }
    })
}

/// Os itens: o produto que continua guarda o item (e os preços); o que sai leva os preços.
fn gravar_itens(
    c: &Connection,
    id: Id,
    itens: &[super::ItemDaCotacao],
    produtos: &std::collections::HashMap<Id, mod_estoque::ProdutoParaNota>,
    antigos: Option<&Cotacao>,
) -> Resultado<()> {
    let mut vivos: Vec<Id> = Vec::new();
    for (ordem, i) in itens.iter().enumerate() {
        let p = &produtos[&i.produto];
        let existente = antigos
            .and_then(|a| a.itens.iter().find(|x| x.produto == i.produto))
            .map(|x| x.id);
        let item = existente.unwrap_or_else(Id::novo);
        vivos.push(item);
        c.execute(
            "INSERT INTO compras_cotacao_item
               (id, cotacao, ordem, produto, descricao, unidade, quantidade)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT (id) DO UPDATE SET ordem = excluded.ordem,
                quantidade = excluded.quantidade",
            params![
                b(item),
                b(id),
                i64::try_from(ordem).unwrap_or(0),
                b(i.produto),
                p.nome,
                p.unidade,
                i.quantidade.unidades_internas(),
            ],
        )
        .map_err(falha)?;
    }
    if let Some(a) = antigos {
        for velho in a.itens.iter().filter(|x| !vivos.contains(&x.id)) {
            c.execute(
                "DELETE FROM compras_cotacao_preco WHERE item = ?1",
                [b(velho.id)],
            )
            .map_err(falha)?;
            c.execute(
                "DELETE FROM compras_cotacao_item WHERE id = ?1",
                [b(velho.id)],
            )
            .map_err(falha)?;
        }
    }
    Ok(())
}

fn remover_fornecedor(c: &Connection, cotacao: Id, fornecedor: Id) -> Resultado<()> {
    c.execute(
        "DELETE FROM compras_cotacao_preco WHERE fornecedor = ?2
           AND item IN (SELECT id FROM compras_cotacao_item WHERE cotacao = ?1)",
        params![b(cotacao), b(fornecedor)],
    )
    .map_err(falha)?;
    c.execute(
        "DELETE FROM compras_cotacao_fornecedor WHERE cotacao = ?1 AND fornecedor = ?2",
        params![b(cotacao), b(fornecedor)],
    )
    .map_err(falha)?;
    Ok(())
}

pub(super) fn registrar_proposta(
    cmd: &RegistrarPropostaFornecedor,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<()> {
    let c = uow.conexao();
    let cot =
        cotacao(c, ctx.empresa, cmd.cotacao)?.ok_or_else(|| Erro::nao_encontrado("cotação"))?;
    exigir_aberta(&cot)?;
    if cmd.frete.e_negativo() {
        return Err(Erro::novo(
            CodigoErro::ENTRADA_INVALIDA,
            "O frete não pode ser negativo",
        ));
    }
    for (item, preco) in &cmd.precos {
        if !cot.itens.iter().any(|i| i.id == *item) {
            return Err(Erro::nao_encontrado("item da cotação"));
        }
        if preco.unidades_internas() <= 0 {
            return Err(Erro::novo(
                CodigoErro::ENTRADA_INVALIDA,
                "O preço cotado precisa ser maior que zero (deixe em branco o que o fornecedor \
                 não tem)",
            ));
        }
    }
    // O fornecedor que não foi convidado e mandou proposta entra na cotação.
    if !cot.propostas.iter().any(|p| p.fornecedor == cmd.fornecedor) {
        exigir_fornecedor(c, cmd.fornecedor)?;
        c.execute(
            "INSERT INTO compras_cotacao_fornecedor (cotacao, fornecedor, ordem)
             VALUES (?1, ?2, ?3)",
            params![
                b(cmd.cotacao),
                b(cmd.fornecedor),
                i64::try_from(cot.propostas.len()).unwrap_or(0)
            ],
        )
        .map_err(falha)?;
    }
    c.execute(
        "UPDATE compras_cotacao_fornecedor
         SET respondeu = 1, prazo_entrega_dias = ?3, condicao_pagamento = ?4, frete = ?5,
             observacao = ?6, respondida_em = ?7
         WHERE cotacao = ?1 AND fornecedor = ?2",
        params![
            b(cmd.cotacao),
            b(cmd.fornecedor),
            cmd.prazo_entrega_dias.map(i64::from),
            cmd.condicao_pagamento.trim(),
            cmd.frete.em_centavos(),
            cmd.observacao.trim(),
            ctx.agora.em_micros(),
        ],
    )
    .map_err(falha)?;
    c.execute(
        "DELETE FROM compras_cotacao_preco WHERE fornecedor = ?2
           AND item IN (SELECT id FROM compras_cotacao_item WHERE cotacao = ?1)",
        params![b(cmd.cotacao), b(cmd.fornecedor)],
    )
    .map_err(falha)?;
    for (item, preco) in &cmd.precos {
        c.execute(
            "INSERT INTO compras_cotacao_preco (item, fornecedor, preco_unitario)
             VALUES (?1, ?2, ?3)
             ON CONFLICT (item, fornecedor) DO UPDATE SET preco_unitario = excluded.preco_unitario",
            params![b(*item), b(cmd.fornecedor), preco.unidades_internas()],
        )
        .map_err(falha)?;
    }
    Ok(())
}

pub(super) fn decidir(
    cmd: &DecidirCotacao,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<Vec<PedidoSalvo>> {
    let cot = cotacao(uow.conexao(), ctx.empresa, cmd.cotacao)?
        .ok_or_else(|| Erro::nao_encontrado("cotação"))?;
    exigir_aberta(&cot)?;
    if cmd.escolhas.is_empty() {
        return Err(Erro::de_dominio(&ErroCompras::DecisaoVazia));
    }
    if cmd.aprovar_pedidos && !ctx.concede("compras.pedido.aprovar") {
        return Err(Erro::novo(
            CodigoErro::SEM_PERMISSAO,
            "Gerar os pedidos já aprovados exige a permissão de aprovar pedido de compra",
        ));
    }
    // Agrupa por fornecedor, na ordem dos itens da cotação.
    let mut grupos: Vec<(Id, Vec<ItemDoPedido>)> = Vec::new();
    for item in &cot.itens {
        let Some(e) = cmd.escolhas.iter().find(|e| e.item == item.id) else {
            continue;
        };
        let proposta = cot
            .propostas
            .iter()
            .find(|p| p.fornecedor == e.fornecedor)
            .ok_or_else(|| Erro::nao_encontrado("fornecedor da cotação"))?;
        let preco = proposta.preco(item.id).ok_or_else(|| {
            Erro::de_dominio(&ErroCompras::EscolhaSemPreco(item.descricao.clone()))
        })?;
        let linha = ItemDoPedido {
            produto: item.produto,
            quantidade: item.quantidade,
            preco_unitario: preco,
        };
        match grupos.iter_mut().find(|(f, _)| *f == e.fornecedor) {
            Some((_, v)) => v.push(linha),
            None => grupos.push((e.fornecedor, vec![linha])),
        }
    }
    if cmd
        .escolhas
        .iter()
        .any(|e| !cot.itens.iter().any(|i| i.id == e.item))
    {
        return Err(Erro::nao_encontrado("item da cotação"));
    }
    let mut salvos = Vec::new();
    for (fornecedor, itens) in grupos {
        let proposta = cot
            .propostas
            .iter()
            .find(|p| p.fornecedor == fornecedor)
            .expect("o grupo só tem fornecedor da cotação");
        let pedido = SalvarPedidoCompra {
            pedido: None,
            fornecedor,
            previsao_entrega: proposta
                .prazo_entrega_dias
                .map(|d| ctx.hoje().mais_dias(i32::from(d))),
            condicao_pagamento: proposta.condicao_pagamento.clone(),
            frete: proposta.frete,
            observacao: format!("Cotação nº {}", cot.numero),
            itens,
        };
        let mut salvo = crate::pedido::sqlite::salvar(&pedido, ctx, uow)?;
        uow.conexao()
            .execute(
                "UPDATE compras_pedido SET cotacao = ?2 WHERE id = ?1",
                params![b(salvo.pedido), b(cot.id)],
            )
            .map_err(falha)?;
        if cmd.aprovar_pedidos {
            crate::pedido::sqlite::mudar(
                salvo.pedido,
                ctx,
                uow,
                crate::pedido::PedidoCompra::aprovar,
            )?;
            salvo.estado = crate::pedido::EstadoPedido::Enviado;
        }
        salvos.push(salvo);
    }
    let c = uow.conexao();
    for e in &cmd.escolhas {
        c.execute(
            "UPDATE compras_cotacao_item SET escolhido = ?2 WHERE id = ?1",
            params![b(e.item), b(e.fornecedor)],
        )
        .map_err(falha)?;
    }
    c.execute(
        "UPDATE compras_cotacao SET estado = 'Decidida', decidida_em = ?2 WHERE id = ?1",
        params![b(cot.id), ctx.agora.em_micros()],
    )
    .map_err(falha)?;
    Ok(salvos)
}

pub(super) fn cancelar(
    cmd: &CancelarCotacao,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<()> {
    let motivo = cmd.motivo.split_whitespace().collect::<Vec<_>>().join(" ");
    if motivo.chars().count() < MOTIVO_MINIMO_PEDIDO {
        return Err(Erro::de_dominio(&ErroCompras::MotivoDoPedidoCurto));
    }
    let c = uow.conexao();
    let cot =
        cotacao(c, ctx.empresa, cmd.cotacao)?.ok_or_else(|| Erro::nao_encontrado("cotação"))?;
    exigir_aberta(&cot)?;
    c.execute(
        "UPDATE compras_cotacao SET estado = 'Cancelada', motivo = ?2 WHERE id = ?1",
        params![b(cot.id), motivo],
    )
    .map_err(falha)?;
    Ok(())
}

pub(super) fn cotacoes(
    c: &Connection,
    empresa: Id,
    so_abertas: bool,
) -> Resultado<Vec<ResumoCotacao>> {
    let mut st = c
        .prepare(
            "SELECT c.id, c.numero, c.descricao, c.data, c.prazo_resposta, c.estado,
                    (SELECT COUNT(*) FROM compras_cotacao_item i WHERE i.cotacao = c.id),
                    (SELECT COUNT(*) FROM compras_cotacao_fornecedor f WHERE f.cotacao = c.id),
                    (SELECT COUNT(*) FROM compras_cotacao_fornecedor f
                      WHERE f.cotacao = c.id AND f.respondeu = 1)
             FROM compras_cotacao c
             WHERE c.empresa = ?1 AND (?2 = 0 OR c.estado = 'Aberta')
             ORDER BY c.numero DESC
             LIMIT 5000",
        )
        .map_err(falha)?;
    let v = st
        .query_map(params![b(empresa), i64::from(so_abertas)], |r| {
            Ok(ResumoCotacao {
                cotacao: id_de(&r.get::<_, Vec<u8>>(0)?),
                numero: u32::try_from(r.get::<_, i64>(1)?).unwrap_or(0),
                descricao: r.get(2)?,
                data: data_de(r.get(3)?),
                prazo_resposta: r.get::<_, Option<i64>>(4)?.map(data_de),
                estado: EstadoCotacao::de_codigo(&r.get::<_, String>(5)?)
                    .unwrap_or(EstadoCotacao::Aberta),
                itens: usize::try_from(r.get::<_, i64>(6)?).unwrap_or(0),
                fornecedores: usize::try_from(r.get::<_, i64>(7)?).unwrap_or(0),
                respostas: usize::try_from(r.get::<_, i64>(8)?).unwrap_or(0),
            })
        })
        .map_err(falha)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(falha);
    v
}

pub(crate) fn cotacao(c: &Connection, empresa: Id, id: Id) -> Resultado<Option<Cotacao>> {
    let Some(mut cot) = c
        .query_row(
            "SELECT numero, descricao, data, prazo_resposta, estado, motivo
             FROM compras_cotacao WHERE empresa = ?1 AND id = ?2",
            params![b(empresa), b(id)],
            |r| {
                Ok(Cotacao {
                    id,
                    numero: u32::try_from(r.get::<_, i64>(0)?).unwrap_or(0),
                    descricao: r.get(1)?,
                    data: data_de(r.get(2)?),
                    prazo_resposta: r.get::<_, Option<i64>>(3)?.map(data_de),
                    estado: EstadoCotacao::de_codigo(&r.get::<_, String>(4)?)
                        .unwrap_or(EstadoCotacao::Aberta),
                    motivo: r.get(5)?,
                    itens: Vec::new(),
                    propostas: Vec::new(),
                    pedidos: Vec::new(),
                })
            },
        )
        .optional()
        .map_err(falha)?
    else {
        return Ok(None);
    };
    let mut st = c
        .prepare(
            "SELECT id, produto, descricao, unidade, quantidade, escolhido
             FROM compras_cotacao_item WHERE cotacao = ?1 ORDER BY ordem",
        )
        .map_err(falha)?;
    cot.itens = st
        .query_map([b(id)], |r| {
            Ok(ItemCotacao {
                id: id_de(&r.get::<_, Vec<u8>>(0)?),
                produto: id_de(&r.get::<_, Vec<u8>>(1)?),
                descricao: r.get(2)?,
                unidade: r.get(3)?,
                quantidade: Quantidade::interna(r.get(4)?),
                escolhido: r.get::<_, Option<Vec<u8>>>(5)?.map(|x| id_de(&x)),
            })
        })
        .map_err(falha)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(falha)?;
    let mut st = c
        .prepare(
            "SELECT fornecedor, respondeu, prazo_entrega_dias, condicao_pagamento, frete,
                    observacao
             FROM compras_cotacao_fornecedor WHERE cotacao = ?1 ORDER BY ordem",
        )
        .map_err(falha)?;
    cot.propostas = st
        .query_map([b(id)], |r| {
            Ok(PropostaFornecedor {
                fornecedor: id_de(&r.get::<_, Vec<u8>>(0)?),
                fornecedor_nome: String::new(),
                respondeu: r.get::<_, i64>(1)? != 0,
                prazo_entrega_dias: r
                    .get::<_, Option<i64>>(2)?
                    .and_then(|d| u16::try_from(d).ok()),
                condicao_pagamento: r.get(3)?,
                frete: Dinheiro::centavos(r.get(4)?),
                observacao: r.get(5)?,
                precos: Vec::new(),
            })
        })
        .map_err(falha)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(falha)?;
    let ids: Vec<Id> = cot.propostas.iter().map(|p| p.fornecedor).collect();
    let nomes = mod_clientes::nomes_das_pessoas(c, &ids)?;
    let mut st = c
        .prepare(
            "SELECT p.item, p.preco_unitario FROM compras_cotacao_preco p
             JOIN compras_cotacao_item i ON i.id = p.item
             WHERE i.cotacao = ?1 AND p.fornecedor = ?2 ORDER BY i.ordem",
        )
        .map_err(falha)?;
    for p in &mut cot.propostas {
        p.fornecedor_nome = nomes.get(&p.fornecedor).cloned().unwrap_or_default();
        p.precos = st
            .query_map(params![b(id), b(p.fornecedor)], |r| {
                Ok((id_de(&r.get::<_, Vec<u8>>(0)?), Preco::interna(r.get(1)?)))
            })
            .map_err(falha)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(falha)?;
    }
    cot.pedidos = pedidos_da_cotacao(c, id)?;
    Ok(Some(cot))
}

/// Os pedidos que a decisão gerou: (pedido, número).
fn pedidos_da_cotacao(c: &Connection, id: Id) -> Resultado<Vec<(Id, u32)>> {
    let mut st = c
        .prepare("SELECT id, numero FROM compras_pedido WHERE cotacao = ?1 ORDER BY numero")
        .map_err(falha)?;
    let v = st
        .query_map([b(id)], |r| {
            Ok((
                id_de(&r.get::<_, Vec<u8>>(0)?),
                u32::try_from(r.get::<_, i64>(1)?).unwrap_or(0),
            ))
        })
        .map_err(falha)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(falha);
    v
}
