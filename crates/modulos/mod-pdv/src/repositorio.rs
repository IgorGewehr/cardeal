//! Todo o SQL do módulo de PDV (`docs/15-convencoes-codigo.md` §5): um único lugar com
//! strings SQL, sempre colunas explícitas, sempre parâmetros posicionais.
//!
//! [`RepositorioPdv`] grava e lê `pdv_faixa_numeracao`/`pdv_cupom`/`pdv_item_cupom`/
//! `pdv_pagamento_cupom` sobre a [`UnidadeDeTrabalho`](cardeal_storage::UnidadeDeTrabalho) do
//! escritor único — mesmo padrão do `RepositorioVendas`.

use cardeal_kernel::{
    CodigoErro, Dinheiro, Erro, Id, Instante, Percentual, Preco, Quantidade, Resultado, Versao,
};
use cardeal_storage::UnidadeDeTrabalho;
use rusqlite::{params, Connection, OptionalExtension};

use crate::cupom::{Cupom, EstadoCupom, FormaPagamentoPdv, ItemCupom, PagamentoCupom};

#[allow(clippy::needless_pass_by_value)] // usado como `.map_err(persist)`
pub(crate) fn persist(e: rusqlite::Error) -> Erro {
    Erro::novo(CodigoErro::FALHA_INTERNA, format!("pdv/SQL: {e}"))
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

pub(crate) fn versao_de(n: i64) -> Versao {
    Versao::nova(u64::try_from(n).unwrap_or(1))
}

pub(crate) fn versao_i64(v: Versao) -> i64 {
    i64::try_from(v.numero()).unwrap_or(i64::MAX)
}

pub(crate) fn estado_txt(e: EstadoCupom) -> &'static str {
    match e {
        EstadoCupom::EmAndamento => "EmAndamento",
        EstadoCupom::Finalizado => "Finalizado",
        EstadoCupom::Cancelado => "Cancelado",
    }
}

pub(crate) fn estado_de(s: &str) -> EstadoCupom {
    match s {
        "Finalizado" => EstadoCupom::Finalizado,
        "Cancelado" => EstadoCupom::Cancelado,
        _ => EstadoCupom::EmAndamento,
    }
}

pub(crate) fn forma_txt(f: FormaPagamentoPdv) -> &'static str {
    match f {
        FormaPagamentoPdv::Dinheiro => "Dinheiro",
        FormaPagamentoPdv::Pix => "Pix",
        FormaPagamentoPdv::Debito => "Debito",
        FormaPagamentoPdv::Credito => "Credito",
    }
}

pub(crate) fn forma_de(s: &str) -> FormaPagamentoPdv {
    match s {
        "Pix" => FormaPagamentoPdv::Pix,
        "Debito" => FormaPagamentoPdv::Debito,
        "Credito" => FormaPagamentoPdv::Credito,
        _ => FormaPagamentoPdv::Dinheiro,
    }
}

pub(crate) fn item_de_linha(r: &rusqlite::Row<'_>) -> rusqlite::Result<ItemCupom> {
    Ok(ItemCupom {
        id: id_de(r.get::<_, Vec<u8>>(0)?),
        produto: id_de(r.get::<_, Vec<u8>>(1)?),
        variacao: id_opt_de(r.get::<_, Option<Vec<u8>>>(2)?),
        quantidade: Quantidade::interna(r.get::<_, i64>(3)?),
        preco_unitario: Preco::interna(r.get::<_, i64>(4)?),
        desconto_percentual: Percentual::unidades(r.get::<_, i64>(5)?),
        desconto_valor: Dinheiro::centavos(r.get::<_, i64>(6)?),
        total_item: Dinheiro::centavos(r.get::<_, i64>(7)?),
        cancelado: r.get::<_, i64>(8)? != 0,
        motivo_cancelamento: r.get::<_, Option<String>>(9)?,
    })
}

pub(crate) fn pagamento_de_linha(r: &rusqlite::Row<'_>) -> rusqlite::Result<PagamentoCupom> {
    Ok(PagamentoCupom {
        id: id_de(r.get::<_, Vec<u8>>(0)?),
        forma: forma_de(&r.get::<_, String>(1)?),
        valor: Dinheiro::centavos(r.get::<_, i64>(2)?),
    })
}

/// Grava e lê cupons, itens, pagamentos e a numeração por terminal do PDV.
pub struct RepositorioPdv<'a, 'b> {
    uow: &'a mut UnidadeDeTrabalho<'b>,
}

impl<'a, 'b> RepositorioPdv<'a, 'b> {
    /// Cria o repositório sobre a unidade de trabalho corrente.
    pub fn novo(uow: &'a mut UnidadeDeTrabalho<'b>) -> Self {
        Self { uow }
    }

    fn conn(&self) -> &Connection {
        self.uow.conexao()
    }

    /// O próximo número de cupom para `(terminal, serie_fiscal)`, criando a faixa na primeira
    /// venda do terminal se ainda não existir.
    ///
    /// **Simplificação desta fatia** (ver `docs/modulos/pdv.md` §3/§11 regra 4): o desenho
    /// original reserva um lote de números por sessão de caixa, para terminais realmente
    /// offline não conflitarem entre si. Nesta versão — monoposto, um só terminal por vez —
    /// a faixa nasce com `numero_final = i64::MAX` (efetivamente sem fim) e `proximo` avança
    /// um a um; o esquema (`numero_inicial`/`numero_final`) já está pronto para a reserva por
    /// lote quando fizer sentido, sem precisar de migração nova.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn proximo_numero_cupom(
        &mut self,
        empresa: Id,
        terminal: Id,
        serie_fiscal: i64,
    ) -> Resultado<i64> {
        let existente: Option<i64> = self
            .conn()
            .query_row(
                "SELECT proximo FROM pdv_faixa_numeracao WHERE empresa = ?1 AND terminal = ?2 AND serie_fiscal = ?3",
                params![blob(empresa), blob(terminal), serie_fiscal],
                |r| r.get(0),
            )
            .optional()
            .map_err(persist)?;

        if let Some(proximo) = existente {
            self.conn()
                .execute(
                    "UPDATE pdv_faixa_numeracao SET proximo = proximo + 1
                     WHERE empresa = ?1 AND terminal = ?2 AND serie_fiscal = ?3",
                    params![blob(empresa), blob(terminal), serie_fiscal],
                )
                .map_err(persist)?;
            return Ok(proximo);
        }

        self.conn()
            .execute(
                "INSERT INTO pdv_faixa_numeracao
                   (id, empresa, terminal, serie_fiscal, numero_inicial, numero_final, proximo, reservada_em)
                 VALUES (?1,?2,?3,?4,1,?5,2,?6)",
                params![
                    blob(Id::novo()),
                    blob(empresa),
                    blob(terminal),
                    serie_fiscal,
                    i64::MAX,
                    self.uow.agora().em_micros(),
                ],
            )
            .map_err(persist)?;
        Ok(1)
    }

    /// Grava um cupom recém-aberto (sem itens ainda).
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn inserir_cupom(&mut self, c: &Cupom) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO pdv_cupom
                   (id, empresa, sessao_caixa, terminal, numero_terminal, serie_fiscal, cliente,
                    operador, tabela_preco, local_expedicao, abertura, subtotal, desconto, total,
                    estado, versao)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
                params![
                    blob(c.id),
                    blob(c.empresa),
                    blob(c.sessao_caixa),
                    blob(c.terminal),
                    c.numero_terminal,
                    c.serie_fiscal,
                    blob_opt(c.cliente),
                    blob(c.operador),
                    blob(c.tabela_preco),
                    blob(c.local_expedicao),
                    c.abertura.em_micros(),
                    c.subtotal.em_centavos(),
                    c.desconto.em_centavos(),
                    c.total.em_centavos(),
                    estado_txt(c.estado),
                    versao_i64(c.versao),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Regrava totais/estado de um cupom.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn atualizar_cupom(&mut self, c: &Cupom) -> Resultado<()> {
        self.conn()
            .execute(
                "UPDATE pdv_cupom SET subtotal = ?2, desconto = ?3, total = ?4, estado = ?5, versao = ?6
                 WHERE id = ?1",
                params![
                    blob(c.id),
                    c.subtotal.em_centavos(),
                    c.desconto.em_centavos(),
                    c.total.em_centavos(),
                    estado_txt(c.estado),
                    versao_i64(c.versao),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Busca um cupom (com itens e pagamentos) pelo id. `Ok(None)` = não existe.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn buscar_cupom(&self, id: Id) -> Resultado<Option<Cupom>> {
        let cabecalho = self
            .conn()
            .query_row(
                "SELECT id, empresa, sessao_caixa, terminal, numero_terminal, serie_fiscal,
                        cliente, operador, tabela_preco, local_expedicao, abertura, subtotal,
                        desconto, total, estado, versao
                 FROM pdv_cupom WHERE id = ?1",
                [blob(id)],
                |r| {
                    Ok((
                        id_de(r.get::<_, Vec<u8>>(0)?),
                        id_de(r.get::<_, Vec<u8>>(1)?),
                        id_de(r.get::<_, Vec<u8>>(2)?),
                        id_de(r.get::<_, Vec<u8>>(3)?),
                        r.get::<_, i64>(4)?,
                        r.get::<_, i64>(5)?,
                        id_opt_de(r.get::<_, Option<Vec<u8>>>(6)?),
                        id_de(r.get::<_, Vec<u8>>(7)?),
                        id_de(r.get::<_, Vec<u8>>(8)?),
                        id_de(r.get::<_, Vec<u8>>(9)?),
                        Instante::de_micros(r.get::<_, i64>(10)?),
                        Dinheiro::centavos(r.get::<_, i64>(11)?),
                        Dinheiro::centavos(r.get::<_, i64>(12)?),
                        Dinheiro::centavos(r.get::<_, i64>(13)?),
                        estado_de(&r.get::<_, String>(14)?),
                        versao_de(r.get::<_, i64>(15)?),
                    ))
                },
            )
            .optional()
            .map_err(persist)?;
        let Some((
            id,
            empresa,
            sessao_caixa,
            terminal,
            numero_terminal,
            serie_fiscal,
            cliente,
            operador,
            tabela_preco,
            local_expedicao,
            abertura,
            subtotal,
            desconto,
            total,
            estado,
            versao,
        )) = cabecalho
        else {
            return Ok(None);
        };
        let itens = self.itens_do_cupom(id)?;
        let pagamentos = self.pagamentos_do_cupom(id)?;
        Ok(Some(Cupom {
            id,
            empresa,
            sessao_caixa,
            terminal,
            numero_terminal,
            serie_fiscal,
            cliente,
            operador,
            abertura,
            tabela_preco,
            local_expedicao,
            itens,
            pagamentos,
            subtotal,
            desconto,
            total,
            estado,
            versao,
        }))
    }

    /// Os itens de um cupom.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn itens_do_cupom(&self, cupom: Id) -> Resultado<Vec<ItemCupom>> {
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT id, produto, variacao, quantidade, preco_unitario, desconto_percentual,
                        desconto_valor, total_item, cancelado, motivo_cancelamento
                 FROM pdv_item_cupom WHERE cupom = ?1 ORDER BY rowid",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map([blob(cupom)], item_de_linha)
            .map_err(persist)?;
        linhas
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)
    }

    /// Grava um item novo do cupom.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn inserir_item(&mut self, cupom: Id, item: &ItemCupom) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO pdv_item_cupom
                   (id, cupom, produto, variacao, quantidade, preco_unitario, desconto_percentual,
                    desconto_valor, total_item, cancelado, motivo_cancelamento)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
                params![
                    blob(item.id),
                    blob(cupom),
                    blob(item.produto),
                    blob_opt(item.variacao),
                    item.quantidade.unidades_internas(),
                    item.preco_unitario.unidades_internas(),
                    item.desconto_percentual.unidades_internas(),
                    item.desconto_valor.em_centavos(),
                    item.total_item.em_centavos(),
                    i64::from(item.cancelado),
                    item.motivo_cancelamento,
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Regrava um item (desconto aplicado, ou cancelamento).
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn atualizar_item(&mut self, item: &ItemCupom) -> Resultado<()> {
        self.conn()
            .execute(
                "UPDATE pdv_item_cupom
                 SET desconto_percentual = ?2, desconto_valor = ?3, total_item = ?4,
                     cancelado = ?5, motivo_cancelamento = ?6
                 WHERE id = ?1",
                params![
                    blob(item.id),
                    item.desconto_percentual.unidades_internas(),
                    item.desconto_valor.em_centavos(),
                    item.total_item.em_centavos(),
                    i64::from(item.cancelado),
                    item.motivo_cancelamento,
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Os pagamentos de um cupom.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn pagamentos_do_cupom(&self, cupom: Id) -> Resultado<Vec<PagamentoCupom>> {
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT id, forma, valor FROM pdv_pagamento_cupom WHERE cupom = ?1 ORDER BY rowid",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map([blob(cupom)], pagamento_de_linha)
            .map_err(persist)?;
        linhas
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)
    }

    /// Grava um pagamento do cupom.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn inserir_pagamento(&mut self, cupom: Id, p: &PagamentoCupom) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO pdv_pagamento_cupom (id, cupom, forma, valor) VALUES (?1,?2,?3,?4)",
                params![
                    blob(p.id),
                    blob(cupom),
                    forma_txt(p.forma),
                    p.valor.em_centavos()
                ],
            )
            .map_err(persist)?;
        Ok(())
    }
}
