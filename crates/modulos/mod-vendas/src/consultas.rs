//! As consultas do módulo de vendas — leitura autorizada sobre `vendas_pedido`, sem SQL cru
//! na tela. `docs/modulos/vendas.md` §6/§10 e `docs/09-protocolo-api.md` §5.

use cardeal_kernel::{Data, Dinheiro, Id, Resultado};
use cardeal_modkit::{Consulta, Ctx};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::pedido::{EstadoPedido, ItemVenda};
use crate::preco::{RegraPreco, TabelaPreco};
use crate::repositorio::{
    blob, data_de, estado_pedido_de, id_de, item_de_linha, persist, regra_de_linha,
    tabela_de_linha,
};

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

/// Lista as tabelas de preço ativas da empresa, por nome. Alimenta o seletor de tabela ao
/// abrir um pedido.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabelasDePreco;

impl Consulta for TabelasDePreco {
    type Saida = Vec<TabelaPreco>;
    const PERMISSAO: &'static str = "vendas.tabela_preco.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let mut stmt = conexao
            .prepare(
                "SELECT id, empresa, nome, tipo, vigente_de, vigente_ate, ativa, versao
                 FROM vendas_tabela_preco
                 WHERE empresa = ?1 AND ativa = 1
                 ORDER BY nome ASC",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map([blob(ctx.empresa)], tabela_de_linha)
            .map_err(persist)?;
        linhas.collect::<rusqlite::Result<Vec<_>>>().map_err(persist)
    }
}

/// As regras de preço de uma tabela — para a tela mostrar/gerenciar preços.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegrasDaTabela {
    /// A tabela.
    pub tabela: Id,
}

impl Consulta for RegrasDaTabela {
    type Saida = Vec<RegraPreco>;
    const PERMISSAO: &'static str = "vendas.tabela_preco.ver";

    fn executar(self, _ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let mut stmt = conexao
            .prepare(
                "SELECT id, empresa, tabela_preco, produto, grupo_produto, quantidade_minima,
                        preco, periodo_de, periodo_ate
                 FROM vendas_regra_preco
                 WHERE tabela_preco = ?1
                 ORDER BY rowid ASC",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map([blob(self.tabela)], regra_de_linha)
            .map_err(persist)?;
        linhas.collect::<rusqlite::Result<Vec<_>>>().map_err(persist)
    }
}

/// Os itens de um pedido — para o dialog de detalhe montar a lista e o total.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItensDoPedido {
    /// O pedido.
    pub pedido: Id,
}

impl Consulta for ItensDoPedido {
    type Saida = Vec<ItemVenda>;
    const PERMISSAO: &'static str = "vendas.pedido.ver";

    fn executar(self, _ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let mut stmt = conexao
            .prepare(
                "SELECT id, produto, variacao, quantidade, preco_unitario, desconto_percentual,
                        desconto_valor, total_item, reserva
                 FROM vendas_item_pedido WHERE pedido = ?1 ORDER BY rowid",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map([blob(self.pedido)], item_de_linha)
            .map_err(persist)?;
        linhas.collect::<rusqlite::Result<Vec<_>>>().map_err(persist)
    }
}
