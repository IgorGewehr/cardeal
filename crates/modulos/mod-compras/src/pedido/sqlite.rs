//! A persistência do pedido de compra e os ganchos na entrada da nota: ligar na importação,
//! receber na confirmação, desfazer no estorno.

use std::collections::HashMap;

use cardeal_kernel::{CodigoErro, Data, Dinheiro, Erro, Id, Preco, Quantidade, Resultado};
use cardeal_modkit::Ctx;
use cardeal_storage::UnidadeDeTrabalho;
use rusqlite::{params, Connection, OptionalExtension};

use super::{
    conferir, distribuir_recebimento, numero_do_xped, ConferenciaDoPedido, EstadoPedido,
    ItemParaConferir, ItemPedido, NotaDoPedido, PecaDaOsNoPedido, PedidoCompra, PedidoSalvo,
    PedidosDeCompra, PrecoPago, ResumoPedido, SalvarPedidoCompra,
};
use crate::erros::ErroCompras;
use crate::nota::EstadoNotaEntrada;

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

/// Como a nota foi ligada ao pedido.
pub(crate) const LIGADA_PELO_XML: &str = "pelo número do pedido no XML";
/// Ligada por ser o único pedido em aberto do fornecedor com os produtos da nota.
pub(crate) const LIGADA_PELO_UNICO: &str = "único pedido em aberto deste fornecedor";
/// Ligada por quem conferiu.
pub(crate) const LIGADA_A_MAO: &str = "ligada na conferência";

/// O próximo número de pedido da empresa.
fn proximo_numero(c: &Connection, empresa: Id) -> Resultado<u32> {
    let n: i64 = c
        .query_row(
            "SELECT COALESCE(MAX(numero), 0) + 1 FROM compras_pedido WHERE empresa = ?1",
            [b(empresa)],
            |r| r.get(0),
        )
        .map_err(falha)?;
    Ok(u32::try_from(n).unwrap_or(u32::MAX))
}

/// O pedido gravado (sem o nome do fornecedor nem as notas, que a consulta completa).
fn carregar(c: &Connection, empresa: Id, id: Id) -> Resultado<Option<PedidoCompra>> {
    let Some(mut p) = c
        .query_row(
            "SELECT numero, fornecedor, cotacao, data, previsao_entrega, condicao_pagamento,
                    frete, observacao, estado, motivo
             FROM compras_pedido WHERE empresa = ?1 AND id = ?2",
            params![b(empresa), b(id)],
            |r| {
                Ok(PedidoCompra {
                    id,
                    numero: u32::try_from(r.get::<_, i64>(0)?).unwrap_or(0),
                    fornecedor: id_de(&r.get::<_, Vec<u8>>(1)?),
                    fornecedor_nome: String::new(),
                    cotacao: r.get::<_, Option<Vec<u8>>>(2)?.map(|x| id_de(&x)),
                    data: data_de(r.get(3)?),
                    previsao_entrega: r.get::<_, Option<i64>>(4)?.map(data_de),
                    condicao_pagamento: r.get(5)?,
                    frete: Dinheiro::centavos(r.get(6)?),
                    observacao: r.get(7)?,
                    estado: EstadoPedido::de_codigo(&r.get::<_, String>(8)?)
                        .unwrap_or(EstadoPedido::Rascunho),
                    motivo: r.get(9)?,
                    itens: Vec::new(),
                    notas: Vec::new(),
                })
            },
        )
        .optional()
        .map_err(falha)?
    else {
        return Ok(None);
    };
    p.itens = itens(c, id)?;
    Ok(Some(p))
}

/// Os itens do pedido, com o recebido de cada um.
fn itens(c: &Connection, pedido: Id) -> Resultado<Vec<ItemPedido>> {
    let mut st = c
        .prepare_cached(
            "SELECT i.id, i.produto, i.descricao, i.unidade, i.quantidade, i.preco_unitario,
                    COALESCE((SELECT SUM(r.quantidade) FROM compras_pedido_recebimento r
                              WHERE r.item_pedido = i.id), 0)
             FROM compras_pedido_item i WHERE i.pedido = ?1 ORDER BY i.ordem",
        )
        .map_err(falha)?;
    let v = st
        .query_map([b(pedido)], |r| {
            Ok(ItemPedido {
                id: id_de(&r.get::<_, Vec<u8>>(0)?),
                produto: id_de(&r.get::<_, Vec<u8>>(1)?),
                descricao: r.get(2)?,
                unidade: r.get(3)?,
                quantidade: Quantidade::interna(r.get(4)?),
                preco_unitario: Preco::interna(r.get(5)?),
                recebida: Quantidade::interna(r.get(6)?),
                para_os: Vec::new(),
            })
        })
        .map_err(falha)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(falha);
    let mut v = v?;
    let mut st = c
        .prepare_cached(
            "SELECT item_peca, quantidade FROM compras_pedido_item_os
             WHERE item_pedido = ?1 ORDER BY ordem",
        )
        .map_err(falha)?;
    for item in &mut v {
        item.para_os = st
            .query_map([b(item.id)], |r| {
                Ok(PecaDaOsNoPedido {
                    item_peca: id_de(&r.get::<_, Vec<u8>>(0)?),
                    quantidade: Quantidade::interna(r.get(1)?),
                })
            })
            .map_err(falha)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(falha)?;
    }
    Ok(v)
}

/// O cabeçalho do pedido, de volta à base.
fn gravar_estado(c: &Connection, p: &PedidoCompra, ctx: &Ctx) -> Resultado<()> {
    c.execute(
        "UPDATE compras_pedido SET estado = ?2, motivo = ?3,
                aprovado_em = CASE WHEN ?2 = 'Enviado' AND aprovado_em IS NULL THEN ?4
                                   ELSE aprovado_em END,
                aprovado_por = CASE WHEN ?2 = 'Enviado' AND aprovado_por IS NULL THEN ?5
                                    ELSE aprovado_por END
         WHERE id = ?1",
        params![
            b(p.id),
            p.estado.codigo(),
            p.motivo,
            ctx.agora.em_micros(),
            b(ctx.usuario)
        ],
    )
    .map_err(falha)?;
    if p.estado == EstadoPedido::Cancelado {
        // As notas que ainda não entraram deixam de apontar para o pedido cancelado.
        c.execute(
            "UPDATE compras_nota_entrada SET pedido = NULL, pedido_ligacao = NULL
             WHERE pedido = ?1 AND estado IN ('AConferir', 'Conferida')",
            [b(p.id)],
        )
        .map_err(falha)?;
    }
    Ok(())
}

/// Os itens com quantidade, conferidos (fornecedor de verdade, produto que existe e não se
/// repete), com o nome e a unidade de cada produto.
fn validar(
    cmd: &SalvarPedidoCompra,
    ctx: &Ctx,
    c: &Connection,
) -> Resultado<(
    Vec<super::ItemDoPedido>,
    HashMap<Id, mod_estoque::ProdutoParaNota>,
    String,
)> {
    let itens: Vec<_> = cmd
        .itens
        .iter()
        .filter(|i| i.quantidade.e_positiva())
        .cloned()
        .collect();
    if itens.is_empty() {
        return Err(Erro::de_dominio(&ErroCompras::PedidoVazio));
    }
    if itens
        .iter()
        .any(|i| i.preco_unitario.unidades_internas() < 0)
        || cmd.frete.e_negativo()
    {
        return Err(Erro::novo(
            CodigoErro::ENTRADA_INVALIDA,
            "Preço e frete não podem ser negativos",
        ));
    }
    let ids: Vec<Id> = itens.iter().map(|i| i.produto).collect();
    let fornecedor = mod_clientes::pessoa_por_id(c, cmd.fornecedor)?
        .ok_or_else(|| Erro::nao_encontrado("fornecedor"))?;
    if !fornecedor.tem_papel(mod_clientes::Papel::Fornecedor) {
        return Err(Erro::novo(
            CodigoErro::ENTRADA_INVALIDA,
            format!("{} não está cadastrado como fornecedor", fornecedor.nome),
        ));
    }
    let produtos = mod_estoque::produtos_para_nota(c, ctx.organizacao, &ids)?;
    for (n, i) in itens.iter().enumerate() {
        let Some(p) = produtos.get(&i.produto) else {
            return Err(Erro::nao_encontrado("produto do pedido"));
        };
        if itens[..n].iter().any(|o| o.produto == i.produto) {
            return Err(Erro::de_dominio(&ErroCompras::ProdutoRepetido(
                p.nome.clone(),
            )));
        }
        let para_os: Quantidade = i.para_os.iter().map(|o| o.quantidade).sum();
        if i.para_os.iter().any(|o| !o.quantidade.e_positiva()) || para_os > i.quantidade {
            return Err(Erro::novo(
                CodigoErro::ENTRADA_INVALIDA,
                format!(
                    "{}: as peças das OS pedem {}, mais do que o item do pedido ({})",
                    p.nome,
                    para_os.formatar(0),
                    i.quantidade.formatar(0)
                ),
            ));
        }
    }
    let mut vistas: Vec<Id> = Vec::new();
    for o in itens.iter().flat_map(|i| &i.para_os) {
        if vistas.contains(&o.item_peca) {
            return Err(Erro::novo(
                CodigoErro::ENTRADA_INVALIDA,
                "A mesma peça de OS está em mais de um item do pedido",
            ));
        }
        vistas.push(o.item_peca);
    }
    Ok((itens, produtos, fornecedor.nome))
}

/// As peças de OS do pedido existem, esperam peça e são do produto do item.
fn conferir_pecas_das_os(
    itens: &[super::ItemDoPedido],
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<()> {
    for i in itens {
        for o in &i.para_os {
            let peca = mod_os::RepositorioOs::novo(uow)
                .buscar_item_peca(o.item_peca)?
                .ok_or_else(|| Erro::nao_encontrado("peça da OS"))?;
            if peca.produto != i.produto {
                return Err(Erro::novo(
                    CodigoErro::ENTRADA_INVALIDA,
                    "A peça da OS é de outro produto que o item do pedido",
                ));
            }
            if !mod_os::peca_espera_o_pedido(o.item_peca, uow)? {
                return Err(Erro::novo(
                    CodigoErro::REGRA_VIOLADA,
                    "A peça da OS já foi aplicada ou a OS terminou",
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn salvar(
    cmd: &SalvarPedidoCompra,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<PedidoSalvo> {
    let (itens, produtos, fornecedor) = validar(cmd, ctx, uow.conexao())?;
    conferir_pecas_das_os(&itens, uow)?;
    let c = uow.conexao();
    let tinha_os = match cmd.pedido {
        Some(id) => c
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM compras_pedido_item_os o
                                JOIN compras_pedido_item i ON i.id = o.item_pedido
                                WHERE i.pedido = ?1)",
                [b(id)],
                |r| r.get::<_, bool>(0),
            )
            .map_err(falha)?,
        None => false,
    };
    let (id, numero) = match cmd.pedido {
        None => criar_cabecalho(cmd, ctx, c)?,
        Some(id) => editar_cabecalho(id, cmd, ctx, c)?,
    };
    for (ordem, i) in itens.iter().enumerate() {
        let p = &produtos[&i.produto];
        let item_id = Id::novo();
        c.execute(
            "INSERT INTO compras_pedido_item
               (id, pedido, ordem, produto, descricao, unidade, quantidade, preco_unitario)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                b(item_id),
                b(id),
                i64::try_from(ordem).unwrap_or(0),
                b(i.produto),
                p.nome,
                p.unidade,
                i.quantidade.unidades_internas(),
                i.preco_unitario.unidades_internas(),
            ],
        )
        .map_err(falha)?;
        for (n, o) in i.para_os.iter().enumerate() {
            c.execute(
                "INSERT INTO compras_pedido_item_os (item_pedido, item_peca, ordem, quantidade)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    b(item_id),
                    b(o.item_peca),
                    i64::try_from(n).unwrap_or(0),
                    o.quantidade.unidades_internas()
                ],
            )
            .map_err(falha)?;
        }
    }
    // As peças das OS ficam encomendadas por este pedido; as que saíram dele voltam a "a comprar".
    let mut atendidas = Vec::new();
    for i in &itens {
        for o in &i.para_os {
            let custo = (i.preco_unitario.unidades_internas() > 0).then_some(i.preco_unitario);
            mod_os::encomendar_pelo_pedido_comum(
                o.item_peca,
                id,
                &fornecedor,
                custo,
                cmd.previsao_entrega,
                ctx,
                uow,
            )?;
            atendidas.push(o.item_peca);
        }
    }
    if tinha_os {
        mod_os::desfazer_encomendas_do_pedido_comum(id, &atendidas, uow)?;
    }
    Ok(PedidoSalvo {
        pedido: id,
        numero,
        estado: EstadoPedido::Rascunho,
    })
}

/// O pedido novo, em rascunho: (id, número).
fn criar_cabecalho(cmd: &SalvarPedidoCompra, ctx: &Ctx, c: &Connection) -> Resultado<(Id, u32)> {
    let id = Id::novo();
    let numero = proximo_numero(c, ctx.empresa)?;
    c.execute(
        "INSERT INTO compras_pedido
                   (id, empresa, numero, fornecedor, data, previsao_entrega, condicao_pagamento,
                    frete, observacao, estado, criado_em, criado_por)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'Rascunho', ?10, ?11)",
        params![
            b(id),
            b(ctx.empresa),
            i64::from(numero),
            b(cmd.fornecedor),
            ctx.hoje().em_dias(),
            cmd.previsao_entrega.map(Data::em_dias),
            cmd.condicao_pagamento.trim(),
            cmd.frete.em_centavos(),
            cmd.observacao.trim(),
            ctx.agora.em_micros(),
            b(ctx.usuario),
        ],
    )
    .map_err(falha)?;
    Ok((id, numero))
}

/// O pedido editado volta ao rascunho, sem os itens (que se gravam de novo): (id, número).
fn editar_cabecalho(
    id: Id,
    cmd: &SalvarPedidoCompra,
    ctx: &Ctx,
    c: &Connection,
) -> Resultado<(Id, u32)> {
    let atual = carregar(c, ctx.empresa, id)?.ok_or_else(|| Erro::nao_encontrado("pedido"))?;
    if !atual.editavel() {
        return Err(Erro::de_dominio(&ErroCompras::EstadoDoPedidoInvalido {
            atual: atual.estado.nome(),
            acao: "editar",
        }));
    }
    let ligado = c
        .query_row(
            "SELECT EXISTS (SELECT 1 FROM compras_nota_entrada WHERE pedido = ?1)",
            [b(id)],
            |r| r.get::<_, bool>(0),
        )
        .map_err(falha)?;
    if ligado && atual.fornecedor != cmd.fornecedor {
        return Err(Erro::novo(
            CodigoErro::REGRA_VIOLADA,
            "Há nota deste fornecedor ligada ao pedido — desligue-a antes de trocar o \
                     fornecedor",
        ));
    }
    // Editar o aprovado o devolve ao rascunho: precisa de nova aprovação.
    c.execute(
        "UPDATE compras_pedido SET fornecedor = ?2, previsao_entrega = ?3,
                        condicao_pagamento = ?4, frete = ?5, observacao = ?6,
                        estado = 'Rascunho', aprovado_em = NULL, aprovado_por = NULL
                 WHERE id = ?1",
        params![
            b(id),
            b(cmd.fornecedor),
            cmd.previsao_entrega.map(Data::em_dias),
            cmd.condicao_pagamento.trim(),
            cmd.frete.em_centavos(),
            cmd.observacao.trim(),
        ],
    )
    .map_err(falha)?;
    c.execute(
        "DELETE FROM compras_pedido_item_os
         WHERE item_pedido IN (SELECT id FROM compras_pedido_item WHERE pedido = ?1)",
        [b(id)],
    )
    .map_err(falha)?;
    c.execute("DELETE FROM compras_pedido_item WHERE pedido = ?1", [b(id)])
        .map_err(falha)?;
    Ok((id, atual.numero))
}

/// Carrega o pedido, aplica a transição e grava.
pub(crate) fn mudar(
    pedido: Id,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
    f: impl FnOnce(&mut PedidoCompra) -> Result<(), ErroCompras>,
) -> Resultado<()> {
    let c = uow.conexao();
    let mut p = carregar(c, ctx.empresa, pedido)?.ok_or_else(|| Erro::nao_encontrado("pedido"))?;
    f(&mut p).map_err(|e| Erro::de_dominio(&e))?;
    gravar_estado(c, &p, ctx)?;
    let tem_os = p.itens.iter().any(|i| !i.para_os.is_empty());
    if tem_os && matches!(p.estado, EstadoPedido::Cancelado | EstadoPedido::Encerrado) {
        // O que o pedido não vai mais trazer volta a "a comprar" na OS; a peça que já chegou
        // (reservada) continua ligada a ele.
        let pecas: Vec<Id> = p
            .itens
            .iter()
            .flat_map(|i| i.para_os.iter().map(|o| o.item_peca))
            .collect();
        let chegaram: Vec<Id> = mod_estoque::reservas_das_origens(
            uow.conexao(),
            ctx.empresa,
            mod_os::ORIGEM_RESERVA_PECA,
            &pecas,
        )?
        .into_iter()
        .map(|r| r.origem_id)
        .collect();
        mod_os::desfazer_encomendas_do_pedido_comum(p.id, &chegaram, uow)?;
    }
    Ok(())
}

/// O que já chegou pelo pedido fica guardado para as peças das OS que ele atende: em cada item,
/// o recebido vai às peças na ordem (cada uma até a quantidade dela) e o resto é do estoque. Só
/// reserva o que falta (idempotente) e só para a peça que ainda espera; reserva no `local`.
fn reservar_para_as_os(
    pedido: Id,
    local: Id,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<()> {
    let Some(p) = carregar(uow.conexao(), ctx.empresa, pedido)? else {
        return Ok(());
    };
    for item in &p.itens {
        let mut resta = item.recebida;
        for o in &item.para_os {
            let alvo = o.quantidade.min(resta);
            resta = (resta - alvo).nao_negativa();
            if !alvo.e_positiva() || !mod_os::peca_espera_o_pedido(o.item_peca, uow)? {
                continue;
            }
            let ja: Quantidade = mod_estoque::reservas_das_origens(
                uow.conexao(),
                ctx.empresa,
                mod_os::ORIGEM_RESERVA_PECA,
                &[o.item_peca],
            )?
            .iter()
            .map(|r| r.quantidade)
            .sum();
            if ja < alvo {
                mod_estoque::reservar_para_origem_comum(
                    mod_estoque::DadosReserva {
                        produto: item.produto,
                        local,
                        quantidade: alvo - ja,
                        origem_modulo: mod_os::ORIGEM_RESERVA_PECA,
                        origem_id: o.item_peca,
                    },
                    ctx,
                    uow,
                )?;
            }
        }
    }
    Ok(())
}

/// No estorno da entrada, **antes** de tirar a mercadoria: as reservas das peças de OS do pedido
/// da nota voltam ao disponível (senão a mercadoria parece já consumida). Devolve o local onde
/// estavam, para [`desfazer_recebimento`] reservar de novo o que outras notas trouxeram.
pub(crate) fn soltar_reservas_da_nota(
    nota: Id,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<Option<Id>> {
    let Some((pedido, _)) = pedido_da_nota(uow.conexao(), nota)? else {
        return Ok(None);
    };
    let Some(p) = carregar(uow.conexao(), ctx.empresa, pedido)? else {
        return Ok(None);
    };
    let mut local = None;
    for o in p.itens.iter().flat_map(|i| &i.para_os) {
        let soltas = mod_estoque::liberar_reservas_da_origem_comum(
            mod_os::ORIGEM_RESERVA_PECA,
            o.item_peca,
            ctx,
            uow,
        )?;
        local = local.or_else(|| soltas.first().map(|r| r.local));
    }
    Ok(local)
}

/// O pedido e como a nota está ligada a ele.
fn pedido_da_nota(c: &Connection, nota: Id) -> Resultado<Option<(Id, String)>> {
    c.query_row(
        "SELECT pedido, COALESCE(pedido_ligacao, '') FROM compras_nota_entrada WHERE id = ?1",
        [b(nota)],
        |r| {
            Ok(r.get::<_, Option<Vec<u8>>>(0)?
                .map(|p| (id_de(&p), r.get::<_, String>(1).unwrap_or_default())))
        },
    )
    .optional()
    .map(Option::flatten)
    .map_err(falha)
}

fn ligar(c: &Connection, nota: Id, pedido: Option<Id>, como: &str) -> Resultado<()> {
    c.execute(
        "UPDATE compras_nota_entrada SET pedido = ?2, pedido_ligacao = ?3 WHERE id = ?1",
        params![b(nota), pedido.map(b), pedido.map(|_| como)],
    )
    .map(|_| ())
    .map_err(falha)
}

pub(super) fn vincular(
    nota: Id,
    pedido: Option<Id>,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<()> {
    let n = crate::comandos::carregar_nota(uow, nota)?;
    let c = uow.conexao();
    if let Some(pedido) = pedido {
        let p = carregar(c, ctx.empresa, pedido)?.ok_or_else(|| Erro::nao_encontrado("pedido"))?;
        if p.fornecedor != n.fornecedor {
            return Err(Erro::de_dominio(&ErroCompras::PedidoDeOutroFornecedor));
        }
        if !p.estado.aguardando() {
            return Err(Erro::de_dominio(&ErroCompras::EstadoDoPedidoInvalido {
                atual: p.estado.nome(),
                acao: "receber nota",
            }));
        }
    }
    if !matches!(
        n.estado,
        EstadoNotaEntrada::AConferir | EstadoNotaEntrada::Conferida
    ) {
        return Err(Erro::de_dominio(&ErroCompras::NotaJaConfirmada));
    }
    ligar(c, nota, pedido, LIGADA_A_MAO)
}

/// Os pedidos aprovados do fornecedor que ainda esperam mercadoria: (id, número).
fn aguardando_do_fornecedor(
    c: &Connection,
    empresa: Id,
    fornecedor: Id,
) -> Resultado<Vec<(Id, u32)>> {
    let mut st = c
        .prepare(
            "SELECT id, numero FROM compras_pedido
             WHERE empresa = ?1 AND fornecedor = ?2
               AND estado IN ('Enviado', 'ParcialmenteRecebido')
             ORDER BY numero",
        )
        .map_err(falha)?;
    let v = st
        .query_map(params![b(empresa), b(fornecedor)], |r| {
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

/// Na importação: liga a nota ao pedido do fornecedor — pelo número que o fornecedor escreveu
/// no XML (`xped`) ou, sem ele, ao único pedido em aberto do fornecedor que tem algum produto
/// da nota. Devolve o pedido ligado.
pub(crate) fn ligar_na_importacao(
    nota: Id,
    fornecedor: Id,
    xped: &[&str],
    produtos_da_nota: &[Id],
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<Option<Id>> {
    let c = uow.conexao();
    let abertos = aguardando_do_fornecedor(c, ctx.empresa, fornecedor)?;
    if abertos.is_empty() {
        return Ok(None);
    }
    for x in xped {
        if let Some(n) = numero_do_xped(x) {
            if let Some((id, _)) = abertos.iter().find(|(_, numero)| *numero == n) {
                ligar(c, nota, Some(*id), LIGADA_PELO_XML)?;
                return Ok(Some(*id));
            }
        }
    }
    if let [(id, _)] = abertos.as_slice() {
        let itens = itens(c, *id)?;
        if itens.iter().any(|i| produtos_da_nota.contains(&i.produto)) {
            ligar(c, nota, Some(*id), LIGADA_PELO_UNICO)?;
            return Ok(Some(*id));
        }
    }
    Ok(None)
}

/// [`ligar_na_importacao`] com o que o XML diz: o `xPed` da nota e o de cada item, e os
/// produtos que já casaram.
pub(crate) fn ligar_pelo_xml(
    nota: Id,
    fornecedor: Id,
    xml: &cardeal_fiscal::NotaFiscalXml,
    itens: &[crate::ItemNotaEntrada],
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<Option<Id>> {
    let xped: Vec<&str> = xml
        .pedido
        .iter()
        .chain(xml.itens.iter().filter_map(|i| i.pedido.as_ref()))
        .map(String::as_str)
        .collect();
    let produtos: Vec<Id> = itens.iter().filter_map(|i| i.produto_casado).collect();
    ligar_na_importacao(nota, fornecedor, &xped, &produtos, ctx, uow)
}

/// O que cada item do pedido recebeu desta nota.
fn recebido_da_nota(c: &Connection, nota: Id) -> Resultado<HashMap<Id, Quantidade>> {
    let mut st = c
        .prepare("SELECT item_pedido, quantidade FROM compras_pedido_recebimento WHERE nota = ?1")
        .map_err(falha)?;
    let v = st
        .query_map([b(nota)], |r| {
            Ok((
                id_de(&r.get::<_, Vec<u8>>(0)?),
                Quantidade::interna(r.get(1)?),
            ))
        })
        .map_err(falha)?
        .collect::<rusqlite::Result<HashMap<_, _>>>()
        .map_err(falha);
    v
}

pub(super) fn conferencia(
    c: &Connection,
    empresa: Id,
    nota: Id,
) -> Resultado<Option<ConferenciaDoPedido>> {
    let Some((pedido, ligacao)) = pedido_da_nota(c, nota)? else {
        return Ok(None);
    };
    let Some(p) = carregar(c, empresa, pedido)? else {
        return Ok(None);
    };
    // O recebido antes desta nota: tira o que ela mesma já deu (a nota confirmada).
    let desta = recebido_da_nota(c, nota)?;
    let antes: HashMap<Id, Quantidade> = p
        .itens
        .iter()
        .map(|i| {
            let d = desta.get(&i.id).copied().unwrap_or(Quantidade::ZERO);
            (i.id, (i.recebida - d).nao_negativa())
        })
        .collect();
    let itens: Vec<ItemParaConferir> = crate::repositorio::itens_da_nota_por_conexao(c, nota)?
        .into_iter()
        .map(|i| ItemParaConferir {
            produto: i.produto_casado,
            descricao: i.descricao_fornecedor,
            quantidade: i.quantidade,
            preco_unitario: i.valor_unitario,
        })
        .collect();
    Ok(Some(conferir(&p, &antes, &itens, &ligacao)))
}

/// A nota ligada a um pedido diverge dele (não se confirma sozinha).
pub(crate) fn diverge_do_pedido(c: &Connection, empresa: Id, nota: Id) -> Resultado<bool> {
    Ok(conferencia(c, empresa, nota)?.is_some_and(|c| !c.divergencias.is_empty()))
}

/// Na confirmação da entrada: o que a nota trouxe entra no pedido ligado.
pub(crate) fn receber(
    nota: Id,
    local: Id,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<()> {
    let c = uow.conexao();
    let Some((pedido, _)) = pedido_da_nota(c, nota)? else {
        return Ok(());
    };
    let Some(mut p) = carregar(c, ctx.empresa, pedido)? else {
        return Ok(());
    };
    let trazido: Vec<(Id, Quantidade)> = crate::repositorio::itens_da_nota_por_conexao(c, nota)?
        .into_iter()
        .filter_map(|i| Some((i.produto_casado?, i.quantidade)))
        .collect();
    for (item, q) in distribuir_recebimento(&p.itens, &trazido) {
        c.execute(
            "INSERT INTO compras_pedido_recebimento (item_pedido, nota, quantidade)
             VALUES (?1, ?2, ?3)
             ON CONFLICT (item_pedido, nota) DO UPDATE SET quantidade = excluded.quantidade",
            params![b(item), b(nota), q.unidades_internas()],
        )
        .map_err(falha)?;
    }
    p.itens = itens(c, pedido)?;
    p.atualizar_pelo_recebimento();
    gravar_estado(c, &p, ctx)?;
    reservar_para_as_os(pedido, local, ctx, uow)
}

/// No estorno da entrada: o que a nota deu ao pedido sai; o que outras notas trouxeram volta a
/// ficar reservado para as OS, no `local` onde as reservas estavam (ver
/// [`soltar_reservas_da_nota`]).
pub(crate) fn desfazer_recebimento(
    nota: Id,
    local: Option<Id>,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<()> {
    let c = uow.conexao();
    let Some((pedido, _)) = pedido_da_nota(c, nota)? else {
        return Ok(());
    };
    c.execute(
        "DELETE FROM compras_pedido_recebimento WHERE nota = ?1",
        [b(nota)],
    )
    .map_err(falha)?;
    let Some(mut p) = carregar(c, ctx.empresa, pedido)? else {
        return Ok(());
    };
    p.atualizar_pelo_recebimento();
    gravar_estado(c, &p, ctx)?;
    match local {
        Some(local) => reservar_para_as_os(pedido, local, ctx, uow),
        None => Ok(()),
    }
}

pub(super) fn pedidos(
    c: &Connection,
    empresa: Id,
    q: PedidosDeCompra,
) -> Resultado<Vec<ResumoPedido>> {
    let mut st = c
        .prepare(
            "SELECT p.id, p.numero, p.fornecedor, p.data, p.previsao_entrega, p.estado, p.frete
             FROM compras_pedido p
             WHERE p.empresa = ?1 AND (?2 IS NULL OR p.fornecedor = ?2)
               AND (?3 = 0 OR p.estado IN ('Rascunho', 'Enviado', 'ParcialmenteRecebido'))
             ORDER BY p.numero DESC
             LIMIT 5000",
        )
        .map_err(falha)?;
    let mut v = st
        .query_map(
            params![
                b(empresa),
                q.fornecedor.map(b),
                i64::from(q.so_em_andamento)
            ],
            |r| {
                Ok(ResumoPedido {
                    pedido: id_de(&r.get::<_, Vec<u8>>(0)?),
                    numero: u32::try_from(r.get::<_, i64>(1)?).unwrap_or(0),
                    fornecedor: id_de(&r.get::<_, Vec<u8>>(2)?),
                    fornecedor_nome: String::new(),
                    data: data_de(r.get(3)?),
                    previsao_entrega: r.get::<_, Option<i64>>(4)?.map(data_de),
                    estado: EstadoPedido::de_codigo(&r.get::<_, String>(5)?)
                        .unwrap_or(EstadoPedido::Rascunho),
                    total: Dinheiro::centavos(r.get(6)?),
                    recebido: Dinheiro::ZERO,
                })
            },
        )
        .map_err(falha)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(falha)?;
    // Os totais pelo mesmo arredondamento do domínio (o SQL só filtra e ordena).
    let ids: Vec<Id> = v.iter().map(|p| p.fornecedor).collect();
    let nomes = mod_clientes::nomes_das_pessoas(c, &ids)?;
    for r in &mut v {
        let itens = itens(c, r.pedido)?;
        r.total += itens.iter().map(ItemPedido::total).sum::<Dinheiro>();
        r.recebido = itens
            .iter()
            .map(|i| {
                Dinheiro::de_total(
                    i.recebida.min(i.quantidade),
                    i.preco_unitario,
                    cardeal_kernel::Arredondamento::MeioAcima,
                )
            })
            .sum();
        r.fornecedor_nome = nomes.get(&r.fornecedor).cloned().unwrap_or_default();
    }
    Ok(v)
}

pub(super) fn pedido(c: &Connection, empresa: Id, id: Id) -> Resultado<Option<PedidoCompra>> {
    let Some(mut p) = carregar(c, empresa, id)? else {
        return Ok(None);
    };
    p.fornecedor_nome = mod_clientes::nomes_das_pessoas(c, &[p.fornecedor])?
        .remove(&p.fornecedor)
        .unwrap_or_default();
    let mut st = c
        .prepare(
            "SELECT id, numero, serie, data_emissao, estado, valor_total
             FROM compras_nota_entrada WHERE pedido = ?1 ORDER BY data_emissao, rowid",
        )
        .map_err(falha)?;
    p.notas = st
        .query_map([b(id)], |r| {
            Ok(NotaDoPedido {
                nota: id_de(&r.get::<_, Vec<u8>>(0)?),
                numero: r.get(1)?,
                serie: r.get(2)?,
                emissao: data_de(r.get(3)?),
                estado: crate::repositorio::estado_nota_de(&r.get::<_, String>(4)?),
                total: Dinheiro::centavos(r.get(5)?),
            })
        })
        .map_err(falha)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(falha)?;
    Ok(Some(p))
}

pub(super) fn a_caminho(c: &Connection, empresa: Id) -> Resultado<Vec<(Id, Quantidade)>> {
    let mut st = c
        .prepare(
            "SELECT i.produto, i.quantidade,
                    COALESCE((SELECT SUM(r.quantidade) FROM compras_pedido_recebimento r
                              WHERE r.item_pedido = i.id), 0)
             FROM compras_pedido_item i JOIN compras_pedido p ON p.id = i.pedido
             WHERE p.empresa = ?1 AND p.estado IN ('Enviado', 'ParcialmenteRecebido')",
        )
        .map_err(falha)?;
    let linhas = st
        .query_map([b(empresa)], |r| {
            Ok((
                id_de(&r.get::<_, Vec<u8>>(0)?),
                Quantidade::interna(r.get(1)?),
                Quantidade::interna(r.get(2)?),
            ))
        })
        .map_err(falha)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(falha)?;
    let mut por: Vec<(Id, Quantidade)> = Vec::new();
    for (produto, pedida, recebida) in linhas {
        let saldo = (pedida - recebida).nao_negativa();
        if !saldo.e_positiva() {
            continue;
        }
        match por.iter_mut().find(|(p, _)| *p == produto) {
            Some((_, q)) => *q += saldo,
            None => por.push((produto, saldo)),
        }
    }
    Ok(por)
}

pub(crate) fn ultimos_precos(
    c: &Connection,
    empresa: Id,
    produtos: &[Id],
) -> Resultado<Vec<PrecoPago>> {
    let mut st = c
        .prepare_cached(
            "SELECT i.valor_unitario, n.fornecedor, n.data_emissao
             FROM compras_item_nota_entrada i
             JOIN compras_nota_entrada n ON n.id = i.nota_entrada
             WHERE n.empresa = ?1 AND i.produto_casado = ?2
               AND n.estado IN ('Confirmada', 'Devolvida')
             ORDER BY n.data_emissao DESC, n.rowid DESC
             LIMIT 1",
        )
        .map_err(falha)?;
    let mut v = Vec::new();
    for p in produtos {
        if v.iter().any(|x: &PrecoPago| x.produto == *p) {
            continue;
        }
        if let Some(x) = st
            .query_row(params![b(empresa), b(*p)], |r| {
                Ok(PrecoPago {
                    produto: *p,
                    preco: Preco::interna(r.get(0)?),
                    fornecedor: id_de(&r.get::<_, Vec<u8>>(1)?),
                    data: data_de(r.get(2)?),
                })
            })
            .optional()
            .map_err(falha)?
        {
            v.push(x);
        }
    }
    Ok(v)
}
