//! As consultas do módulo de vendas — leitura autorizada sobre `vendas_pedido`, sem SQL cru
//! na tela. `docs/modulos/vendas.md` §6/§10 e `docs/09-protocolo-api.md` §5.

use cardeal_kernel::{Data, Dinheiro, Id, Resultado};
use cardeal_modkit::{Consulta, Ctx};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::pedido::EstadoPedido;
use crate::repositorio::{blob, data_de, estado_pedido_de, id_de, persist};

/// Um pedido na lista de Vendas — cabeçalho suficiente para a grade, sem carregar itens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemPedido {
    /// O pedido.
    pub pedido: Id,
    /// O cliente.
    pub cliente: Id,
    /// A data de emissão.
    pub data: Data,
    /// O total do pedido.
    pub total: Dinheiro,
    /// O estado atual.
    pub estado: EstadoPedido,
    /// Quantos itens o pedido tem.
    pub itens: u32,
}

/// Lista os pedidos da empresa, mais recentes primeiro. Sem cursor real ainda
/// (`docs/09-protocolo-api.md` §5) — teto de 200 linhas, suficiente para o volume de uma PME.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PedidosRecentes;

impl Consulta for PedidosRecentes {
    type Saida = Vec<ItemPedido>;
    const PERMISSAO: &'static str = "vendas.pedido.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let mut stmt = conexao
            .prepare(
                "SELECT p.id, p.cliente, p.data, p.total, p.estado,
                        (SELECT COUNT(*) FROM vendas_item_pedido i WHERE i.pedido = p.id)
                 FROM vendas_pedido p
                 WHERE p.empresa = ?1
                 ORDER BY p.data DESC, p.id DESC
                 LIMIT 200",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map([blob(ctx.empresa)], |r| {
                Ok(ItemPedido {
                    pedido: id_de(r.get::<_, Vec<u8>>(0)?),
                    cliente: id_de(r.get::<_, Vec<u8>>(1)?),
                    data: data_de(r.get::<_, i64>(2)?),
                    total: Dinheiro::centavos(r.get::<_, i64>(3)?),
                    estado: estado_pedido_de(&r.get::<_, String>(4)?),
                    itens: r.get::<_, i64>(5)?.try_into().unwrap_or(0),
                })
            })
            .map_err(persist)?;
        linhas.collect::<rusqlite::Result<Vec<_>>>().map_err(persist)
    }
}
