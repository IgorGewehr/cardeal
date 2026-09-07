//! Todo o SQL do módulo de orçamentos (`docs/15-convencoes-codigo.md` §5): um lugar só,
//! colunas explícitas, parâmetros posicionais. Mesmo padrão do `RepositorioOs`.

use cardeal_kernel::{
    CodigoErro, Data, Dinheiro, Erro, Id, Instante, Percentual, Preco, Quantidade, Resultado, Versao,
};
use cardeal_storage::UnidadeDeTrabalho;
use rusqlite::{params, Connection, OptionalExtension};

use crate::item::ItemOrcamento;
use crate::orcamento::{EstadoOrcamento, Orcamento};

#[allow(clippy::needless_pass_by_value)]
pub(crate) fn persist(e: rusqlite::Error) -> Erro {
    Erro::novo(CodigoErro::FALHA_INTERNA, format!("orcamentos/SQL: {e}"))
}

pub(crate) fn blob(id: Id) -> Vec<u8> {
    id.em_bytes().to_vec()
}

pub(crate) fn id_de(bytes: Vec<u8>) -> Id {
    <[u8; 16]>::try_from(bytes).map_or(Id::NULO, Id::de_bytes)
}

pub(crate) fn id_opt(bytes: Option<Vec<u8>>) -> Option<Id> {
    bytes.and_then(|b| <[u8; 16]>::try_from(b).ok()).map(Id::de_bytes)
}

fn dias(d: Data) -> i64 {
    i64::from(d.em_dias())
}

pub(crate) fn data_de(dias: i64) -> Data {
    Data::de_dias(i32::try_from(dias).unwrap_or(0))
}

fn versao_i64(v: Versao) -> i64 {
    i64::try_from(v.numero()).unwrap_or(i64::MAX)
}

fn versao_de(n: i64) -> Versao {
    Versao::nova(u64::try_from(n).unwrap_or(1))
}

pub(crate) fn estado_de(s: &str) -> EstadoOrcamento {
    match s {
        "Enviado" => EstadoOrcamento::Enviado,
        "Aprovado" => EstadoOrcamento::Aprovado,
        "Recusado" => EstadoOrcamento::Recusado,
        "Expirado" => EstadoOrcamento::Expirado,
        "Cancelado" => EstadoOrcamento::Cancelado,
        "Convertido" => EstadoOrcamento::Convertido,
        _ => EstadoOrcamento::Rascunho,
    }
}

/// Grava e lê orçamentos e seus itens.
pub struct RepositorioOrcamentos<'a, 'b> {
    uow: &'a mut UnidadeDeTrabalho<'b>,
}

impl<'a, 'b> RepositorioOrcamentos<'a, 'b> {
    /// Cria o repositório sobre a unidade de trabalho corrente.
    pub fn novo(uow: &'a mut UnidadeDeTrabalho<'b>) -> Self {
        Self { uow }
    }

    fn conn(&self) -> &Connection {
        self.uow.conexao()
    }

    /// O próximo número de orçamento da empresa.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn proximo_numero(&mut self) -> Resultado<u64> {
        self.uow
            .proximo_numero("orcamentos.orcamento")
            .map_err(|e| Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string()))
    }

    /// Grava um orçamento novo.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn inserir_orcamento(&mut self, o: &Orcamento) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO orcamentos_orcamento
                   (id, empresa, numero, cliente, cliente_nome, cliente_documento, cliente_contato,
                    assunto, descricao, data_emissao, validade, condicoes_pagamento, prazo_entrega,
                    observacoes, desconto_percentual, estado, responsavel, aprovado_por, os_gerada,
                    criado_em, versao)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21)",
                params![
                    blob(o.id),
                    blob(o.empresa),
                    i64::try_from(o.numero).unwrap_or(i64::MAX),
                    o.cliente.map(blob),
                    o.cliente_nome,
                    o.cliente_documento,
                    o.cliente_contato,
                    o.assunto,
                    o.descricao,
                    dias(o.data_emissao),
                    dias(o.validade),
                    o.condicoes_pagamento,
                    o.prazo_entrega,
                    o.observacoes,
                    o.desconto_percentual.unidades_internas(),
                    o.estado.rotulo(),
                    blob(o.responsavel),
                    o.aprovado_por,
                    o.os_gerada.map(blob),
                    o.criado_em.em_micros(),
                    versao_i64(o.versao),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Regrava um orçamento (cabeçalho, estado, transições).
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn atualizar_orcamento(&mut self, o: &Orcamento) -> Resultado<()> {
        self.conn()
            .execute(
                "UPDATE orcamentos_orcamento SET
                    cliente = ?2, cliente_nome = ?3, cliente_documento = ?4, cliente_contato = ?5,
                    assunto = ?6, descricao = ?7, validade = ?8, condicoes_pagamento = ?9,
                    prazo_entrega = ?10, observacoes = ?11, desconto_percentual = ?12, estado = ?13,
                    aprovado_por = ?14, os_gerada = ?15, versao = ?16
                 WHERE id = ?1",
                params![
                    blob(o.id),
                    o.cliente.map(blob),
                    o.cliente_nome,
                    o.cliente_documento,
                    o.cliente_contato,
                    o.assunto,
                    o.descricao,
                    dias(o.validade),
                    o.condicoes_pagamento,
                    o.prazo_entrega,
                    o.observacoes,
                    o.desconto_percentual.unidades_internas(),
                    o.estado.rotulo(),
                    o.aprovado_por,
                    o.os_gerada.map(blob),
                    versao_i64(o.versao),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Busca um orçamento pelo id.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn buscar_orcamento(&self, id: Id) -> Resultado<Option<Orcamento>> {
        buscar_orcamento(self.conn(), id)
    }

    /// Substitui a lista inteira de itens de um orçamento.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn redefinir_itens(&mut self, orcamento: Id, itens: &[ItemOrcamento]) -> Resultado<()> {
        self.conn()
            .execute(
                "DELETE FROM orcamentos_item WHERE orcamento = ?1",
                [blob(orcamento)],
            )
            .map_err(persist)?;
        for item in itens {
            self.inserir_item(item)?;
        }
        Ok(())
    }

    /// Grava um item.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn inserir_item(&mut self, item: &ItemOrcamento) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO orcamentos_item
                   (id, orcamento, ordem, descricao, quantidade, unidade, preco_unitario,
                    desconto_percentual, total)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![
                    blob(item.id),
                    blob(item.orcamento),
                    i64::from(item.ordem),
                    item.descricao,
                    item.quantidade.unidades_internas(),
                    item.unidade,
                    item.preco_unitario.unidades_internas(),
                    item.desconto_percentual.unidades_internas(),
                    item.total.em_centavos(),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Os itens de um orçamento, na ordem digitada.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn itens_do_orcamento(&self, orcamento: Id) -> Resultado<Vec<ItemOrcamento>> {
        itens_do_orcamento(self.conn(), orcamento)
    }
}

pub(crate) fn buscar_orcamento(conexao: &Connection, id: Id) -> Resultado<Option<Orcamento>> {
    conexao
        .query_row(
            "SELECT id, empresa, numero, cliente, cliente_nome, cliente_documento, cliente_contato,
                    assunto, descricao, data_emissao, validade, condicoes_pagamento, prazo_entrega,
                    observacoes, desconto_percentual, estado, responsavel, aprovado_por, os_gerada,
                    criado_em, versao
             FROM orcamentos_orcamento WHERE id = ?1",
            [blob(id)],
            orcamento_de_linha,
        )
        .optional()
        .map_err(persist)
}

pub(crate) fn itens_do_orcamento(
    conexao: &Connection,
    orcamento: Id,
) -> Resultado<Vec<ItemOrcamento>> {
    let mut stmt = conexao
        .prepare(
            "SELECT id, orcamento, ordem, descricao, quantidade, unidade, preco_unitario,
                    desconto_percentual, total
             FROM orcamentos_item WHERE orcamento = ?1 ORDER BY ordem, rowid",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map([blob(orcamento)], item_de_linha)
        .map_err(persist)?;
    linhas.collect::<rusqlite::Result<Vec<_>>>().map_err(persist)
}

pub(crate) fn orcamento_de_linha(r: &rusqlite::Row<'_>) -> rusqlite::Result<Orcamento> {
    Ok(Orcamento {
        id: id_de(r.get::<_, Vec<u8>>(0)?),
        empresa: id_de(r.get::<_, Vec<u8>>(1)?),
        numero: u64::try_from(r.get::<_, i64>(2)?).unwrap_or(0),
        cliente: id_opt(r.get::<_, Option<Vec<u8>>>(3)?),
        cliente_nome: r.get(4)?,
        cliente_documento: r.get(5)?,
        cliente_contato: r.get(6)?,
        assunto: r.get(7)?,
        descricao: r.get(8)?,
        data_emissao: data_de(r.get::<_, i64>(9)?),
        validade: data_de(r.get::<_, i64>(10)?),
        condicoes_pagamento: r.get(11)?,
        prazo_entrega: r.get(12)?,
        observacoes: r.get(13)?,
        desconto_percentual: Percentual::unidades(r.get::<_, i64>(14)?),
        estado: estado_de(&r.get::<_, String>(15)?),
        responsavel: id_de(r.get::<_, Vec<u8>>(16)?),
        aprovado_por: r.get(17)?,
        os_gerada: id_opt(r.get::<_, Option<Vec<u8>>>(18)?),
        criado_em: Instante::de_micros(r.get::<_, i64>(19)?),
        versao: versao_de(r.get::<_, i64>(20)?),
    })
}

pub(crate) fn item_de_linha(r: &rusqlite::Row<'_>) -> rusqlite::Result<ItemOrcamento> {
    Ok(ItemOrcamento {
        id: id_de(r.get::<_, Vec<u8>>(0)?),
        orcamento: id_de(r.get::<_, Vec<u8>>(1)?),
        ordem: u32::try_from(r.get::<_, i64>(2)?).unwrap_or(0),
        descricao: r.get(3)?,
        quantidade: Quantidade::interna(r.get::<_, i64>(4)?),
        unidade: r.get(5)?,
        preco_unitario: Preco::interna(r.get::<_, i64>(6)?),
        desconto_percentual: Percentual::unidades(r.get::<_, i64>(7)?),
        total: Dinheiro::centavos(r.get::<_, i64>(8)?),
    })
}
