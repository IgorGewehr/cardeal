//! Todo o SQL do módulo de vendas (`docs/15-convencoes-codigo.md` §5): um único lugar com
//! strings SQL, sempre colunas explícitas, sempre parâmetros posicionais.
//!
//! [`RepositorioVendas`] grava e lê `vendas_tabela_preco`/`vendas_regra_preco`/
//! `vendas_pedido`/`vendas_item_pedido` sobre a
//! [`UnidadeDeTrabalho`](cardeal_storage::UnidadeDeTrabalho) do escritor único — mesmo
//! padrão do `RepositorioFinanceiro`.

use cardeal_kernel::{
    CodigoErro, Data, Dinheiro, Erro, Id, Percentual, Preco, Quantidade, Resultado, Versao,
};
use cardeal_storage::UnidadeDeTrabalho;
use rusqlite::{params, Connection, OptionalExtension};

use crate::pedido::{EstadoPedido, ItemVenda, Pedido};
use crate::preco::{AlvoRegra, RegraPreco, TabelaPreco, TipoTabela};

pub(crate) fn persist(e: rusqlite::Error) -> Erro {
    Erro::novo(CodigoErro::FALHA_INTERNA, format!("vendas/SQL: {e}"))
}

pub(crate) fn blob(id: Id) -> Vec<u8> {
    id.em_bytes().to_vec()
}

pub(crate) fn blob_opt(id: Option<Id>) -> Option<Vec<u8>> {
    id.map(blob)
}

pub(crate) fn id_de(bytes: Vec<u8>) -> Id {
    <[u8; 16]>::try_from(bytes).map_or(Id::NULO, Id::de_bytes)
}

pub(crate) fn id_opt_de(bytes: Option<Vec<u8>>) -> Option<Id> {
    bytes.map(id_de)
}

pub(crate) fn dias(d: Data) -> i64 {
    i64::from(d.em_dias())
}

pub(crate) fn data_de(dias: i64) -> Data {
    Data::de_dias(i32::try_from(dias).unwrap_or(0))
}

pub(crate) fn versao_de(n: i64) -> Versao {
    Versao::nova(u64::try_from(n).unwrap_or(1))
}

pub(crate) fn versao_i64(v: Versao) -> i64 {
    i64::try_from(v.numero()).unwrap_or(i64::MAX)
}

pub(crate) fn tipo_tabela_txt(t: TipoTabela) -> &'static str {
    match t {
        TipoTabela::Venda => "Venda",
        TipoTabela::Atacado => "Atacado",
        TipoTabela::Promocional => "Promocional",
    }
}

pub(crate) fn tipo_tabela_de(s: &str) -> TipoTabela {
    match s {
        "Atacado" => TipoTabela::Atacado,
        "Promocional" => TipoTabela::Promocional,
        _ => TipoTabela::Venda,
    }
}

pub(crate) fn estado_pedido_txt(e: EstadoPedido) -> &'static str {
    match e {
        EstadoPedido::Rascunho => "Rascunho",
        EstadoPedido::Confirmado => "Confirmado",
        EstadoPedido::Faturado => "Faturado",
        EstadoPedido::EmEntrega => "EmEntrega",
        EstadoPedido::Concluido => "Concluido",
        EstadoPedido::Cancelado => "Cancelado",
    }
}

pub(crate) fn estado_pedido_de(s: &str) -> EstadoPedido {
    match s {
        "Confirmado" => EstadoPedido::Confirmado,
        "Faturado" => EstadoPedido::Faturado,
        "EmEntrega" => EstadoPedido::EmEntrega,
        "Concluido" => EstadoPedido::Concluido,
        "Cancelado" => EstadoPedido::Cancelado,
        _ => EstadoPedido::Rascunho,
    }
}

pub(crate) fn tabela_de_linha(r: &rusqlite::Row<'_>) -> rusqlite::Result<TabelaPreco> {
    Ok(TabelaPreco {
        id: id_de(r.get::<_, Vec<u8>>(0)?),
        empresa: id_de(r.get::<_, Vec<u8>>(1)?),
        nome: r.get(2)?,
        tipo: tipo_tabela_de(&r.get::<_, String>(3)?),
        vigente_de: data_de(r.get::<_, i64>(4)?),
        vigente_ate: r.get::<_, Option<i64>>(5)?.map(data_de),
        ativa: r.get::<_, i64>(6)? != 0,
        versao: versao_de(r.get::<_, i64>(7)?),
    })
}

pub(crate) fn regra_de_linha(r: &rusqlite::Row<'_>) -> rusqlite::Result<RegraPreco> {
    let produto: Option<Vec<u8>> = r.get(3)?;
    let grupo: Option<Vec<u8>> = r.get(4)?;
    let alvo = produto.map_or_else(
        || AlvoRegra::Grupo(id_de(grupo.unwrap_or_default())),
        |p| AlvoRegra::Produto(id_de(p)),
    );
    Ok(RegraPreco {
        id: id_de(r.get::<_, Vec<u8>>(0)?),
        empresa: id_de(r.get::<_, Vec<u8>>(1)?),
        tabela_preco: id_de(r.get::<_, Vec<u8>>(2)?),
        alvo,
        quantidade_minima: r.get::<_, Option<i64>>(5)?.map(Quantidade::interna),
        preco: Preco::interna(r.get::<_, i64>(6)?),
        periodo_de: r.get::<_, Option<i64>>(7)?.map(data_de),
        periodo_ate: r.get::<_, Option<i64>>(8)?.map(data_de),
    })
}

pub(crate) fn item_de_linha(r: &rusqlite::Row<'_>) -> rusqlite::Result<ItemVenda> {
    Ok(ItemVenda {
        id: id_de(r.get::<_, Vec<u8>>(0)?),
        produto: id_de(r.get::<_, Vec<u8>>(1)?),
        variacao: id_opt_de(r.get::<_, Option<Vec<u8>>>(2)?),
        quantidade: Quantidade::interna(r.get::<_, i64>(3)?),
        preco_unitario: Preco::interna(r.get::<_, i64>(4)?),
        desconto_percentual: Percentual::unidades(r.get::<_, i64>(5)?),
        desconto_valor: Dinheiro::centavos(r.get::<_, i64>(6)?),
        total_item: Dinheiro::centavos(r.get::<_, i64>(7)?),
        reserva: id_opt_de(r.get::<_, Option<Vec<u8>>>(8)?),
    })
}

/// Grava e lê tabelas de preço, regras, pedidos e itens de pedido.
pub struct RepositorioVendas<'a, 'b> {
    uow: &'a mut UnidadeDeTrabalho<'b>,
}

impl<'a, 'b> RepositorioVendas<'a, 'b> {
    /// Cria o repositório sobre a unidade de trabalho corrente.
    pub fn novo(uow: &'a mut UnidadeDeTrabalho<'b>) -> Self {
        Self { uow }
    }

    fn conn(&self) -> &Connection {
        self.uow.conexao()
    }

    /// Grava uma tabela de preço recém-criada.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn inserir_tabela_preco(&mut self, t: &TabelaPreco) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO vendas_tabela_preco (id, empresa, nome, tipo, vigente_de, vigente_ate, ativa, versao)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![
                    blob(t.id),
                    blob(t.empresa),
                    t.nome,
                    tipo_tabela_txt(t.tipo),
                    dias(t.vigente_de),
                    t.vigente_ate.map(dias),
                    i64::from(t.ativa),
                    versao_i64(t.versao),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Busca uma tabela de preço pelo id. `Ok(None)` = não existe.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn buscar_tabela_preco(&self, id: Id) -> Resultado<Option<TabelaPreco>> {
        self.conn()
            .query_row(
                "SELECT id, empresa, nome, tipo, vigente_de, vigente_ate, ativa, versao
                 FROM vendas_tabela_preco WHERE id = ?1",
                [blob(id)],
                tabela_de_linha,
            )
            .optional()
            .map_err(persist)
    }

    /// Grava uma regra de preço recém-criada.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn inserir_regra_preco(&mut self, r: &RegraPreco) -> Resultado<()> {
        let (produto, grupo) = match r.alvo {
            AlvoRegra::Produto(p) => (Some(blob(p)), None),
            AlvoRegra::Grupo(g) => (None, Some(blob(g))),
        };
        self.conn()
            .execute(
                "INSERT INTO vendas_regra_preco
                   (id, empresa, tabela_preco, produto, grupo_produto, quantidade_minima, preco, periodo_de, periodo_ate)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![
                    blob(r.id),
                    blob(r.empresa),
                    blob(r.tabela_preco),
                    produto,
                    grupo,
                    r.quantidade_minima.map(Quantidade::unidades_internas),
                    r.preco.unidades_internas(),
                    r.periodo_de.map(dias),
                    r.periodo_ate.map(dias),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Todas as regras de uma tabela de preço.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn regras_da_tabela(&self, tabela_preco: Id) -> Resultado<Vec<RegraPreco>> {
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT id, empresa, tabela_preco, produto, grupo_produto, quantidade_minima, preco, periodo_de, periodo_ate
                 FROM vendas_regra_preco WHERE tabela_preco = ?1",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map([blob(tabela_preco)], regra_de_linha)
            .map_err(persist)?;
        linhas
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)
    }

    /// Grava um pedido recém-criado (sem itens ainda).
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn inserir_pedido(&mut self, p: &Pedido) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO vendas_pedido
                   (id, empresa, orcamento_origem, cliente, vendedor, data, condicao_pagamento,
                    tabela_preco, centro_custo, local_expedicao, desconto_total, total, estado,
                    versao, criado_por)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
                params![
                    blob(p.id),
                    blob(p.empresa),
                    blob_opt(p.orcamento_origem),
                    blob(p.cliente),
                    blob(p.vendedor),
                    dias(p.data),
                    blob(p.condicao_pagamento),
                    blob(p.tabela_preco),
                    blob_opt(p.centro_custo),
                    blob(p.local_expedicao),
                    p.desconto_total.em_centavos(),
                    p.total.em_centavos(),
                    estado_pedido_txt(p.estado),
                    versao_i64(p.versao),
                    blob(p.criado_por),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Regrava o estado/total de um pedido (transição, ou recálculo após item novo).
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn atualizar_pedido(&mut self, p: &Pedido) -> Resultado<()> {
        self.conn()
            .execute(
                "UPDATE vendas_pedido SET desconto_total = ?2, total = ?3, estado = ?4, versao = ?5 WHERE id = ?1",
                params![blob(p.id), p.desconto_total.em_centavos(), p.total.em_centavos(), estado_pedido_txt(p.estado), versao_i64(p.versao)],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Busca um pedido (sem itens) pelo id. `Ok(None)` = não existe.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn buscar_pedido(&self, id: Id) -> Resultado<Option<Pedido>> {
        let cabecalho = self
            .conn()
            .query_row(
                "SELECT id, empresa, orcamento_origem, cliente, vendedor, data, condicao_pagamento,
                        tabela_preco, centro_custo, local_expedicao, desconto_total, total, estado,
                        versao, criado_por
                 FROM vendas_pedido WHERE id = ?1",
                [blob(id)],
                |r| {
                    Ok((
                        id_de(r.get::<_, Vec<u8>>(0)?),
                        id_de(r.get::<_, Vec<u8>>(1)?),
                        id_opt_de(r.get::<_, Option<Vec<u8>>>(2)?),
                        id_de(r.get::<_, Vec<u8>>(3)?),
                        id_de(r.get::<_, Vec<u8>>(4)?),
                        data_de(r.get::<_, i64>(5)?),
                        id_de(r.get::<_, Vec<u8>>(6)?),
                        id_de(r.get::<_, Vec<u8>>(7)?),
                        id_opt_de(r.get::<_, Option<Vec<u8>>>(8)?),
                        id_de(r.get::<_, Vec<u8>>(9)?),
                        Dinheiro::centavos(r.get::<_, i64>(10)?),
                        Dinheiro::centavos(r.get::<_, i64>(11)?),
                        estado_pedido_de(&r.get::<_, String>(12)?),
                        versao_de(r.get::<_, i64>(13)?),
                        id_de(r.get::<_, Vec<u8>>(14)?),
                    ))
                },
            )
            .optional()
            .map_err(persist)?;
        let Some((
            id,
            empresa,
            orcamento_origem,
            cliente,
            vendedor,
            data,
            condicao_pagamento,
            tabela_preco,
            centro_custo,
            local_expedicao,
            desconto_total,
            total,
            estado,
            versao,
            criado_por,
        )) = cabecalho
        else {
            return Ok(None);
        };
        let itens = self.itens_do_pedido(id)?;
        Ok(Some(Pedido {
            id,
            empresa,
            orcamento_origem,
            cliente,
            vendedor,
            data,
            condicao_pagamento,
            tabela_preco,
            centro_custo,
            local_expedicao,
            itens,
            desconto_total,
            total,
            estado,
            versao,
            criado_por,
        }))
    }

    /// Os itens de um pedido.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn itens_do_pedido(&self, pedido: Id) -> Resultado<Vec<ItemVenda>> {
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT id, produto, variacao, quantidade, preco_unitario, desconto_percentual, desconto_valor, total_item, reserva
                 FROM vendas_item_pedido WHERE pedido = ?1 ORDER BY rowid",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map([blob(pedido)], item_de_linha)
            .map_err(persist)?;
        linhas
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)
    }

    /// Grava um item novo do pedido.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn inserir_item(&mut self, pedido: Id, item: &ItemVenda) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO vendas_item_pedido
                   (id, pedido, produto, variacao, quantidade, preco_unitario, desconto_percentual,
                    desconto_valor, total_item, reserva)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![
                    blob(item.id),
                    blob(pedido),
                    blob(item.produto),
                    blob_opt(item.variacao),
                    item.quantidade.unidades_internas(),
                    item.preco_unitario.unidades_internas(),
                    item.desconto_percentual.unidades_internas(),
                    item.desconto_valor.em_centavos(),
                    item.total_item.em_centavos(),
                    blob_opt(item.reserva),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }
}
