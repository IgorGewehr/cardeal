//! Todo o SQL do módulo de compras (`docs/15-convencoes-codigo.md` §5): um único lugar com
//! strings SQL, sempre colunas explícitas, sempre parâmetros posicionais.
//!
//! [`RepositorioCompras`] grava e lê `compras_nota_entrada`/`compras_item_nota_entrada`/
//! `compras_regra_casamento`/`compras_preferencias`/`compras_estado_dfe` sobre a
//! [`UnidadeDeTrabalho`](cardeal_storage::UnidadeDeTrabalho) do escritor único — mesmo
//! padrão do `RepositorioFinanceiro`.

use cardeal_kernel::{CodigoErro, Data, Dinheiro, Erro, Id, Preco, Quantidade, Resultado, Versao};
use cardeal_storage::UnidadeDeTrabalho;
use rusqlite::{params, Connection, OptionalExtension};

use crate::casamento::RegraCasamentoAprendida;
use crate::nota::{EstadoCasamento, EstadoNotaEntrada, ItemNotaEntrada, NotaEntrada};
use crate::preferencias::{PreferenciasCompras, RateioPor};

#[allow(clippy::needless_pass_by_value)] // usado como `.map_err(persist)`
fn persist(e: rusqlite::Error) -> Erro {
    Erro::novo(CodigoErro::FALHA_INTERNA, format!("compras/SQL: {e}"))
}

fn blob(id: Id) -> Vec<u8> {
    id.em_bytes().to_vec()
}

fn blob_opt(id: Option<Id>) -> Option<Vec<u8>> {
    id.map(blob)
}

fn id_de(bytes: Vec<u8>) -> Id {
    <[u8; 16]>::try_from(bytes).map_or(Id::NULO, Id::de_bytes)
}

fn dias(d: Data) -> i64 {
    i64::from(d.em_dias())
}

fn data_de(dias: i64) -> Data {
    Data::de_dias(i32::try_from(dias).unwrap_or(0))
}

fn versao_de(n: i64) -> Versao {
    Versao::nova(u64::try_from(n).unwrap_or(1))
}

fn versao_i64(v: Versao) -> i64 {
    i64::try_from(v.numero()).unwrap_or(i64::MAX)
}

fn estado_nota_txt(e: EstadoNotaEntrada) -> &'static str {
    e.rotulo()
}

fn estado_nota_de(s: &str) -> EstadoNotaEntrada {
    match s {
        "Conferida" => EstadoNotaEntrada::Conferida,
        "Confirmada" => EstadoNotaEntrada::Confirmada,
        "Devolvida" => EstadoNotaEntrada::Devolvida,
        _ => EstadoNotaEntrada::AConferir,
    }
}

fn estado_casamento_txt(e: EstadoCasamento) -> &'static str {
    match e {
        EstadoCasamento::NaoCasado => "NaoCasado",
        EstadoCasamento::SugestaoForte => "SugestaoForte",
        EstadoCasamento::Casado => "Casado",
    }
}

fn estado_casamento_de(s: &str) -> EstadoCasamento {
    match s {
        "SugestaoForte" => EstadoCasamento::SugestaoForte,
        "Casado" => EstadoCasamento::Casado,
        _ => EstadoCasamento::NaoCasado,
    }
}

fn rateio_por_txt(r: RateioPor) -> &'static str {
    match r {
        RateioPor::Valor => "Valor",
        RateioPor::Peso => "Peso",
    }
}

fn rateio_por_de(s: &str) -> RateioPor {
    if s == "Peso" {
        RateioPor::Peso
    } else {
        RateioPor::Valor
    }
}

/// Grava e lê notas de entrada, itens, regras de casamento e preferências do módulo de
/// compras.
pub struct RepositorioCompras<'a, 'b> {
    uow: &'a mut UnidadeDeTrabalho<'b>,
}

impl<'a, 'b> RepositorioCompras<'a, 'b> {
    /// Cria o repositório sobre a unidade de trabalho corrente.
    pub fn novo(uow: &'a mut UnidadeDeTrabalho<'b>) -> Self {
        Self { uow }
    }

    fn conn(&self) -> &Connection {
        self.uow.conexao()
    }

    /// Grava uma nota de entrada recém-criada.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite (inclusive `UNIQUE(empresa,
    /// chave_acesso)` — o comando confere duplicidade antes, mas o índice é a garantia
    /// física).
    pub fn inserir_nota(&mut self, n: &NotaEntrada) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO compras_nota_entrada
                   (id, empresa, fornecedor, chave_acesso, numero, serie, data_emissao,
                    valor_produtos, valor_frete, valor_seguro, valor_outras_despesas,
                    valor_total, estado, versao)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
                params![
                    blob(n.id),
                    blob(n.empresa),
                    blob(n.fornecedor),
                    n.chave_acesso,
                    n.numero,
                    n.serie,
                    dias(n.data_emissao),
                    n.valor_produtos.em_centavos(),
                    n.valor_frete.em_centavos(),
                    n.valor_seguro.em_centavos(),
                    n.valor_outras_despesas.em_centavos(),
                    n.valor_total.em_centavos(),
                    estado_nota_txt(n.estado),
                    versao_i64(n.versao),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Regrava uma nota de entrada (transição de estado).
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn atualizar_nota(&mut self, n: &NotaEntrada) -> Resultado<()> {
        self.conn()
            .execute(
                "UPDATE compras_nota_entrada SET estado = ?2, versao = ?3 WHERE id = ?1",
                params![blob(n.id), estado_nota_txt(n.estado), versao_i64(n.versao)],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Busca uma nota pelo id. `Ok(None)` = não existe.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn buscar_nota(&self, id: Id) -> Resultado<Option<NotaEntrada>> {
        self.conn()
            .query_row(
                "SELECT id, empresa, fornecedor, chave_acesso, numero, serie, data_emissao,
                        valor_produtos, valor_frete, valor_seguro, valor_outras_despesas,
                        valor_total, estado, versao
                 FROM compras_nota_entrada WHERE id = ?1",
                [blob(id)],
                nota_de_linha,
            )
            .optional()
            .map_err(persist)
    }

    /// Busca o id de uma nota já importada por `(empresa, chave_acesso)` — usado para a
    /// idempotência da importação. `Ok(None)` = ainda não importada.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn buscar_nota_por_chave(&self, empresa: Id, chave_acesso: &str) -> Resultado<Option<Id>> {
        self.conn()
            .query_row(
                "SELECT id FROM compras_nota_entrada WHERE empresa = ?1 AND chave_acesso = ?2",
                params![blob(empresa), chave_acesso],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .optional()
            .map_err(persist)
            .map(|opt| opt.map(id_de))
    }

    /// Grava um item de nota de entrada.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn inserir_item(&mut self, item: &ItemNotaEntrada) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO compras_item_nota_entrada
                   (id, nota_entrada, produto_casado, codigo_fornecedor, descricao_fornecedor,
                    ncm, quantidade, valor_unitario, valor_rateio, estado_casamento)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![
                    blob(item.id),
                    blob(item.nota_entrada),
                    blob_opt(item.produto_casado),
                    item.codigo_fornecedor,
                    item.descricao_fornecedor,
                    item.ncm,
                    item.quantidade.unidades_internas(),
                    item.valor_unitario.unidades_internas(),
                    item.valor_rateio.em_centavos(),
                    estado_casamento_txt(item.estado_casamento),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Regrava um item (vínculo de produto, rateio).
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn atualizar_item(&mut self, item: &ItemNotaEntrada) -> Resultado<()> {
        self.conn()
            .execute(
                "UPDATE compras_item_nota_entrada
                 SET produto_casado = ?2, valor_rateio = ?3, estado_casamento = ?4
                 WHERE id = ?1",
                params![
                    blob(item.id),
                    blob_opt(item.produto_casado),
                    item.valor_rateio.em_centavos(),
                    estado_casamento_txt(item.estado_casamento),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Busca um item pelo id. `Ok(None)` = não existe.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn buscar_item(&self, id: Id) -> Resultado<Option<ItemNotaEntrada>> {
        self.conn()
            .query_row(
                "SELECT id, nota_entrada, produto_casado, codigo_fornecedor,
                        descricao_fornecedor, ncm, quantidade, valor_unitario, valor_rateio,
                        estado_casamento
                 FROM compras_item_nota_entrada WHERE id = ?1",
                [blob(id)],
                item_de_linha,
            )
            .optional()
            .map_err(persist)
    }

    /// Todos os itens de uma nota, na ordem em que foram gravados.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn itens_da_nota(&self, nota_entrada: Id) -> Resultado<Vec<ItemNotaEntrada>> {
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT id, nota_entrada, produto_casado, codigo_fornecedor,
                        descricao_fornecedor, ncm, quantidade, valor_unitario, valor_rateio,
                        estado_casamento
                 FROM compras_item_nota_entrada WHERE nota_entrada = ?1 ORDER BY rowid",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map([blob(nota_entrada)], item_de_linha)
            .map_err(persist)?;
        linhas
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)
    }

    /// Quantos itens da nota ainda não estão `Casado`.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn contar_itens_nao_casados(&self, nota_entrada: Id) -> Resultado<usize> {
        let n: i64 = self
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM compras_item_nota_entrada
                 WHERE nota_entrada = ?1 AND estado_casamento <> 'Casado'",
                [blob(nota_entrada)],
                |r| r.get(0),
            )
            .map_err(persist)?;
        Ok(usize::try_from(n).unwrap_or(0))
    }

    /// Busca o produto de uma regra de casamento aprendida. `Ok(None)` = nenhuma regra
    /// ainda para este `(fornecedor, codigo_fornecedor)`.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn buscar_regra_casamento(
        &self,
        empresa: Id,
        fornecedor: Id,
        codigo_fornecedor: &str,
    ) -> Resultado<Option<Id>> {
        self.conn()
            .query_row(
                "SELECT produto FROM compras_regra_casamento
                 WHERE empresa = ?1 AND fornecedor = ?2 AND codigo_fornecedor = ?3",
                params![blob(empresa), blob(fornecedor), codigo_fornecedor],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .optional()
            .map_err(persist)
            .map(|opt| opt.map(id_de))
    }

    /// Grava (ou substitui) uma regra de casamento aprendida.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn upsert_regra_casamento(&mut self, r: &RegraCasamentoAprendida) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO compras_regra_casamento
                   (empresa, fornecedor, codigo_fornecedor, produto, aprendido_em)
                 VALUES (?1,?2,?3,?4,?5)
                 ON CONFLICT (empresa, fornecedor, codigo_fornecedor)
                 DO UPDATE SET produto = excluded.produto, aprendido_em = excluded.aprendido_em",
                params![
                    blob(r.empresa),
                    blob(r.fornecedor),
                    r.codigo_fornecedor,
                    blob(r.produto),
                    r.aprendido_em.em_micros(),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// As preferências de importação da empresa, ou os padrões se ainda não configuradas.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn preferencias(&self, empresa: Id) -> Resultado<PreferenciasCompras> {
        self.conn()
            .query_row(
                "SELECT confirma_automaticamente_quando_tudo_casa, gera_titulo_a_pagar,
                        rateio_por, local_padrao
                 FROM compras_preferencias WHERE empresa = ?1",
                [blob(empresa)],
                |r| {
                    Ok(PreferenciasCompras {
                        empresa,
                        confirma_automaticamente_quando_tudo_casa: r.get::<_, i64>(0)? != 0,
                        gera_titulo_a_pagar: r.get::<_, i64>(1)? != 0,
                        rateio_por: rateio_por_de(&r.get::<_, String>(2)?),
                        local_padrao: r.get::<_, Option<Vec<u8>>>(3)?.map(id_de),
                    })
                },
            )
            .optional()
            .map_err(persist)
            .map(|opt| opt.unwrap_or_else(|| PreferenciasCompras::padrao(empresa)))
    }

    /// Grava (ou substitui) as preferências de importação da empresa.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn definir_preferencias(&mut self, p: &PreferenciasCompras) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO compras_preferencias
                   (empresa, confirma_automaticamente_quando_tudo_casa, gera_titulo_a_pagar,
                    rateio_por, local_padrao)
                 VALUES (?1,?2,?3,?4,?5)
                 ON CONFLICT (empresa) DO UPDATE SET
                   confirma_automaticamente_quando_tudo_casa = excluded.confirma_automaticamente_quando_tudo_casa,
                   gera_titulo_a_pagar = excluded.gera_titulo_a_pagar,
                   rateio_por = excluded.rateio_por,
                   local_padrao = excluded.local_padrao",
                params![
                    blob(p.empresa),
                    i64::from(p.confirma_automaticamente_quando_tudo_casa),
                    i64::from(p.gera_titulo_a_pagar),
                    rateio_por_txt(p.rateio_por),
                    blob_opt(p.local_padrao),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// O CNPJ cadastrado da empresa (`nucleo_empresa.cnpj`) — usado para consultar a
    /// distribuição `DFe`. `nucleo_*` é esquema núcleo, não de outro módulo
    /// (`docs/contratos-internos.md` §7 regra 2 é sobre módulos entre si).
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite ou se o CNPJ cadastrado for inválido.
    pub fn cnpj_da_empresa(&self, empresa: Id) -> Resultado<cardeal_kernel::Cnpj> {
        let texto: String = self
            .conn()
            .query_row(
                "SELECT cnpj FROM nucleo_empresa WHERE id = ?1",
                [blob(empresa)],
                |r| r.get(0),
            )
            .map_err(persist)?;
        cardeal_kernel::Cnpj::novo(&texto).map_err(|_| {
            Erro::novo(
                CodigoErro::FALHA_INTERNA,
                "CNPJ cadastrado da empresa é inválido",
            )
        })
    }

    /// O último NSU visto na distribuição `DFe` para a empresa (`0` se nunca varrida).
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn ultimo_nsu(&self, empresa: Id) -> Resultado<u64> {
        self.conn()
            .query_row(
                "SELECT ultimo_nsu FROM compras_estado_dfe WHERE empresa = ?1",
                [blob(empresa)],
                |r| r.get::<_, i64>(0),
            )
            .optional()
            .map_err(persist)
            .map(|opt| opt.and_then(|n| u64::try_from(n).ok()).unwrap_or(0))
    }

    /// Grava o novo cursor de NSU depois de uma varredura.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn atualizar_ultimo_nsu(&mut self, empresa: Id, nsu: u64) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO compras_estado_dfe (empresa, ultimo_nsu) VALUES (?1,?2)
                 ON CONFLICT (empresa) DO UPDATE SET ultimo_nsu = excluded.ultimo_nsu",
                params![blob(empresa), i64::try_from(nsu).unwrap_or(i64::MAX)],
            )
            .map_err(persist)?;
        Ok(())
    }
}

fn nota_de_linha(r: &rusqlite::Row<'_>) -> rusqlite::Result<NotaEntrada> {
    Ok(NotaEntrada {
        id: id_de(r.get::<_, Vec<u8>>(0)?),
        empresa: id_de(r.get::<_, Vec<u8>>(1)?),
        fornecedor: id_de(r.get::<_, Vec<u8>>(2)?),
        chave_acesso: r.get::<_, Option<String>>(3)?,
        numero: r.get(4)?,
        serie: r.get(5)?,
        data_emissao: data_de(r.get::<_, i64>(6)?),
        valor_produtos: Dinheiro::centavos(r.get::<_, i64>(7)?),
        valor_frete: Dinheiro::centavos(r.get::<_, i64>(8)?),
        valor_seguro: Dinheiro::centavos(r.get::<_, i64>(9)?),
        valor_outras_despesas: Dinheiro::centavos(r.get::<_, i64>(10)?),
        valor_total: Dinheiro::centavos(r.get::<_, i64>(11)?),
        estado: estado_nota_de(&r.get::<_, String>(12)?),
        versao: versao_de(r.get::<_, i64>(13)?),
    })
}

fn item_de_linha(r: &rusqlite::Row<'_>) -> rusqlite::Result<ItemNotaEntrada> {
    Ok(ItemNotaEntrada {
        id: id_de(r.get::<_, Vec<u8>>(0)?),
        nota_entrada: id_de(r.get::<_, Vec<u8>>(1)?),
        produto_casado: r.get::<_, Option<Vec<u8>>>(2)?.map(id_de),
        codigo_fornecedor: r.get(3)?,
        descricao_fornecedor: r.get(4)?,
        ncm: r.get(5)?,
        quantidade: Quantidade::interna(r.get::<_, i64>(6)?),
        valor_unitario: Preco::interna(r.get::<_, i64>(7)?),
        valor_rateio: Dinheiro::centavos(r.get::<_, i64>(8)?),
        estado_casamento: estado_casamento_de(&r.get::<_, String>(9)?),
    })
}
