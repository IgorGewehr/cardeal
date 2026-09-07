//! Todo o SQL do módulo de estoque (`docs/15-convencoes-codigo.md` §5): um único lugar com
//! strings SQL, sempre colunas explícitas, sempre parâmetros posicionais.
//!
//! [`RepositorioEstoque`] grava e lê `estoque_grupo_produto`/`estoque_unidade`/
//! `estoque_produto`/`estoque_local`/`estoque_saldo_local`/`estoque_movimento` sobre a
//! [`UnidadeDeTrabalho`](cardeal_storage::UnidadeDeTrabalho) do escritor único — mesmo
//! padrão do `RepositorioFinanceiro`.

use cardeal_kernel::{CodigoErro, Erro, Id, Instante, Preco, Quantidade, Resultado, Versao};
use cardeal_storage::UnidadeDeTrabalho;
use rusqlite::{params, Connection, OptionalExtension};

use crate::produto::{Produto, Unidade};
use crate::saldo::{Movimento, SaldoLocal, TipoMovimento};

#[allow(clippy::needless_pass_by_value)] // usado como `.map_err(persist)`
pub(crate) fn persist(e: rusqlite::Error) -> Erro {
    Erro::novo(CodigoErro::FALHA_INTERNA, format!("estoque/SQL: {e}"))
}

pub(crate) fn blob(id: Id) -> Vec<u8> {
    id.em_bytes().to_vec()
}

fn blob_opt(id: Option<Id>) -> Option<Vec<u8>> {
    id.map(blob)
}

/// Sentinela de blob vazio para "sem variação" — mantém `UNIQUE(produto, variacao, local)`
/// estável, porque SQLite trata `NULL` como distinto em toda comparação.
fn variacao_blob(id: Option<Id>) -> Vec<u8> {
    id.map_or_else(Vec::new, |v| v.em_bytes().to_vec())
}

fn variacao_de(bytes: Vec<u8>) -> Option<Id> {
    if bytes.is_empty() {
        None
    } else {
        Some(id_de(bytes))
    }
}

pub(crate) fn id_de(bytes: Vec<u8>) -> Id {
    <[u8; 16]>::try_from(bytes).map_or(Id::NULO, Id::de_bytes)
}

fn versao_de(n: i64) -> Versao {
    Versao::nova(u64::try_from(n).unwrap_or(1))
}

fn versao_i64(v: Versao) -> i64 {
    i64::try_from(v.numero()).unwrap_or(i64::MAX)
}

fn tipo_movimento_txt(t: TipoMovimento) -> &'static str {
    match t {
        TipoMovimento::Entrada => "Entrada",
        TipoMovimento::Saida => "Saida",
        TipoMovimento::TransferenciaSaida => "TransferenciaSaida",
        TipoMovimento::TransferenciaEntrada => "TransferenciaEntrada",
        TipoMovimento::AjustePositivo => "AjustePositivo",
        TipoMovimento::AjusteNegativo => "AjusteNegativo",
        TipoMovimento::Reserva => "Reserva",
        TipoMovimento::LiberacaoReserva => "LiberacaoReserva",
        TipoMovimento::Producao => "Producao",
        TipoMovimento::Perda => "Perda",
    }
}

/// Grava e lê grupos, unidades, produtos, locais, saldos e movimentos do estoque.
pub struct RepositorioEstoque<'a, 'b> {
    uow: &'a mut UnidadeDeTrabalho<'b>,
}

impl<'a, 'b> RepositorioEstoque<'a, 'b> {
    /// Cria o repositório sobre a unidade de trabalho corrente.
    pub fn novo(uow: &'a mut UnidadeDeTrabalho<'b>) -> Self {
        Self { uow }
    }

    fn conn(&self) -> &Connection {
        self.uow.conexao()
    }

    /// Cria um grupo de produto (fatia mínima: sem perfil tributário ainda).
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite (inclusive código duplicado).
    pub fn inserir_grupo_produto(
        &mut self,
        empresa: Id,
        id: Id,
        codigo: &str,
        nome: &str,
        pai: Option<Id>,
    ) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO estoque_grupo_produto (id, empresa, codigo, nome, pai, versao)
                 VALUES (?1,?2,?3,?4,?5,1)",
                params![blob(id), blob(empresa), codigo, nome, blob_opt(pai)],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Cria uma unidade de medida.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite (inclusive sigla duplicada).
    pub fn inserir_unidade(&mut self, u: &Unidade) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO estoque_unidade (id, empresa, sigla, nome, fracionavel)
                 VALUES (?1,?2,?3,?4,?5)",
                params![
                    blob(u.id),
                    blob(u.empresa),
                    u.sigla,
                    u.nome,
                    i64::from(u.fracionavel),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Grava um produto recém-criado.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn inserir_produto(&mut self, p: &Produto) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO estoque_produto
                   (id, empresa, grupo_produto, nome, ncm, cest, codigo_barras, controla_grade,
                    controla_lote, controla_validade, unidade_padrao, ponto_pedido,
                    estoque_minimo, estoque_maximo, ativo, versao)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
                params![
                    blob(p.id),
                    blob(p.empresa),
                    blob(p.grupo_produto),
                    p.nome,
                    p.ncm,
                    p.cest,
                    p.codigo_barras,
                    i64::from(p.controla_grade),
                    i64::from(p.controla_lote),
                    i64::from(p.controla_validade),
                    blob(p.unidade_padrao),
                    p.ponto_pedido.map(Quantidade::unidades_internas),
                    p.estoque_minimo.map(Quantidade::unidades_internas),
                    p.estoque_maximo.map(Quantidade::unidades_internas),
                    i64::from(p.ativo),
                    versao_i64(p.versao),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Busca um produto pelo id. `Ok(None)` = não existe.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn buscar_produto(&self, id: Id) -> Resultado<Option<Produto>> {
        self.conn()
            .query_row(
                "SELECT id, empresa, grupo_produto, nome, ncm, cest, codigo_barras,
                        controla_grade, controla_lote, controla_validade, unidade_padrao,
                        ponto_pedido, estoque_minimo, estoque_maximo, ativo, versao
                 FROM estoque_produto WHERE id = ?1",
                [blob(id)],
                produto_de_linha,
            )
            .optional()
            .map_err(persist)
    }

    /// Busca um produto pelo código de barras — o que sustenta o bipe no balcão/PDV.
    /// `Ok(None)` = não existe (ou nenhum produto tem esse código cadastrado).
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn buscar_produto_por_codigo_barras(
        &self,
        empresa: Id,
        codigo_barras: &str,
    ) -> Resultado<Option<Produto>> {
        crate::consultas::produto_por_codigo_barras(self.conn(), empresa, codigo_barras)
    }

    /// Cria um local de estoque.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn inserir_local(&mut self, empresa: Id, id: Id, nome: &str, tipo: &str) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO estoque_local (id, empresa, nome, tipo, ativo)
                 VALUES (?1,?2,?3,?4,1)",
                params![blob(id), blob(empresa), nome, tipo],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Produtos ativos da empresa com o NCM informado — `(id, nome)`, usado por
    /// `mod-compras` na etapa de casamento por NCM + similaridade de descrição
    /// (`docs/modulos/compras.md` §5). Consulta pública, não SQL cru de outro módulo
    /// (`docs/contratos-internos.md` §7 regra 2) — o dono da tabela expõe a leitura.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn produtos_por_ncm(&self, empresa: Id, ncm: &str) -> Resultado<Vec<(Id, String)>> {
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT id, nome FROM estoque_produto
                 WHERE empresa = ?1 AND ncm = ?2 AND ativo = 1",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map(params![blob(empresa), ncm], |r| {
                Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(persist)?;
        linhas
            .map(|l| l.map(|(id, nome)| (id_de(id), nome)))
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)
    }

    /// Busca o saldo de um produto (sem variação) num local. `Ok(None)` = ainda não existe
    /// (o chamador cria um saldo zerado nesse caso).
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn buscar_saldo(&self, produto: Id, local: Id) -> Resultado<Option<SaldoLocal>> {
        self.conn()
            .query_row(
                "SELECT id, empresa, produto, variacao, local, quantidade_disponivel,
                        quantidade_reservada, custo_medio, atualizado_em, versao
                 FROM estoque_saldo_local WHERE produto = ?1 AND variacao = ?2 AND local = ?3",
                params![blob(produto), variacao_blob(None), blob(local)],
                saldo_de_linha,
            )
            .optional()
            .map_err(persist)
    }

    /// Grava um saldo recém-criado (primeiro movimento do produto naquele local).
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn inserir_saldo(&mut self, s: &SaldoLocal) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO estoque_saldo_local
                   (id, empresa, produto, variacao, local, quantidade_disponivel,
                    quantidade_reservada, custo_medio, atualizado_em, versao)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![
                    blob(s.id),
                    blob(s.empresa),
                    blob(s.produto),
                    variacao_blob(s.variacao),
                    blob(s.local),
                    s.quantidade_disponivel.unidades_internas(),
                    s.quantidade_reservada.unidades_internas(),
                    s.custo_medio.unidades_internas(),
                    s.atualizado_em.em_micros(),
                    versao_i64(s.versao),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Regrava um saldo existente após uma entrada/saída/ajuste.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn atualizar_saldo(&mut self, s: &SaldoLocal) -> Resultado<()> {
        self.conn()
            .execute(
                "UPDATE estoque_saldo_local
                 SET quantidade_disponivel = ?2, quantidade_reservada = ?3, custo_medio = ?4,
                     atualizado_em = ?5, versao = ?6
                 WHERE id = ?1",
                params![
                    blob(s.id),
                    s.quantidade_disponivel.unidades_internas(),
                    s.quantidade_reservada.unidades_internas(),
                    s.custo_medio.unidades_internas(),
                    s.atualizado_em.em_micros(),
                    versao_i64(s.versao),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Grava um movimento de estoque.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn inserir_movimento(&mut self, m: &Movimento) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO estoque_movimento
                   (id, empresa, produto, variacao, local, tipo, quantidade, custo_unitario,
                    lote, origem_modulo, origem_id, lancamento, criado_em, criado_por)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
                params![
                    blob(m.id),
                    blob(m.empresa),
                    blob(m.produto),
                    variacao_blob(m.variacao),
                    blob(m.local),
                    tipo_movimento_txt(m.tipo),
                    m.quantidade.unidades_internas(),
                    m.custo_unitario.map(Preco::unidades_internas),
                    blob_opt(m.lote),
                    m.origem_modulo,
                    blob_opt(m.origem_id),
                    blob_opt(m.lancamento),
                    m.criado_em.em_micros(),
                    blob(m.criado_por),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }
}

pub(crate) fn produto_de_linha(r: &rusqlite::Row<'_>) -> rusqlite::Result<Produto> {
    Ok(Produto {
        id: id_de(r.get::<_, Vec<u8>>(0)?),
        empresa: id_de(r.get::<_, Vec<u8>>(1)?),
        grupo_produto: id_de(r.get::<_, Vec<u8>>(2)?),
        nome: r.get(3)?,
        ncm: r.get(4)?,
        cest: r.get::<_, Option<String>>(5)?,
        codigo_barras: r.get::<_, Option<String>>(6)?,
        controla_grade: r.get::<_, i64>(7)? != 0,
        controla_lote: r.get::<_, i64>(8)? != 0,
        controla_validade: r.get::<_, i64>(9)? != 0,
        unidade_padrao: id_de(r.get::<_, Vec<u8>>(10)?),
        ponto_pedido: r.get::<_, Option<i64>>(11)?.map(Quantidade::interna),
        estoque_minimo: r.get::<_, Option<i64>>(12)?.map(Quantidade::interna),
        estoque_maximo: r.get::<_, Option<i64>>(13)?.map(Quantidade::interna),
        ativo: r.get::<_, i64>(14)? != 0,
        versao: versao_de(r.get::<_, i64>(15)?),
    })
}

fn saldo_de_linha(r: &rusqlite::Row<'_>) -> rusqlite::Result<SaldoLocal> {
    Ok(SaldoLocal {
        id: id_de(r.get::<_, Vec<u8>>(0)?),
        empresa: id_de(r.get::<_, Vec<u8>>(1)?),
        produto: id_de(r.get::<_, Vec<u8>>(2)?),
        variacao: variacao_de(r.get::<_, Vec<u8>>(3)?),
        local: id_de(r.get::<_, Vec<u8>>(4)?),
        quantidade_disponivel: Quantidade::interna(r.get::<_, i64>(5)?),
        quantidade_reservada: Quantidade::interna(r.get::<_, i64>(6)?),
        custo_medio: Preco::interna(r.get::<_, i64>(7)?),
        atualizado_em: Instante::de_micros(r.get::<_, i64>(8)?),
        versao: versao_de(r.get::<_, i64>(9)?),
    })
}
