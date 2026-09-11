//! As consultas de estoque: leitura autorizada sobre `estoque_produto`/`estoque_saldo_local`
//! — `docs/modulos/estoque.md` §10 (a ficha de produto precisa do saldo agregado por
//! produto para a lista principal da tela).

use cardeal_kernel::{Id, Preco, Quantidade, Resultado};
use cardeal_modkit::{Consulta, Ctx};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::produto::Produto;
use crate::repositorio::{blob, id_de, persist, produto_de_linha};

/// Um produto com o saldo somado entre todos os locais — o que sustenta a lista principal da
/// tela de Estoque. `custo_medio` aqui é uma aproximação (o maior custo médio entre os
/// locais, não uma média ponderada entre eles) — suficiente para a lista; o detalhe por
/// local (`docs/modulos/estoque.md` §10) fica para quando a ficha de produto existir.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemProdutoComSaldo {
    /// O produto.
    pub produto: Id,
    /// O nome.
    pub nome: String,
    /// NCM.
    pub ncm: String,
    /// A soma do disponível em todos os locais.
    pub disponivel: Quantidade,
    /// A soma do reservado em todos os locais.
    pub reservado: Quantidade,
    /// Uma aproximação do custo médio (ver nota do tipo).
    pub custo_medio: Preco,
}

/// Todos os produtos ativos com saldo agregado, por nome. Sem cursor real ainda, mesma
/// decisão do financeiro (`docs/09-protocolo-api.md` §5): um teto de 500 linhas é suficiente
/// para o catálogo de uma PME.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn produtos_com_saldo(
    conexao: &Connection,
    empresa: Id,
) -> Resultado<Vec<ItemProdutoComSaldo>> {
    let mut stmt = conexao
        .prepare(
            "SELECT p.id, p.nome, p.ncm,
                    COALESCE(SUM(s.quantidade_disponivel), 0),
                    COALESCE(SUM(s.quantidade_reservada), 0),
                    COALESCE(MAX(s.custo_medio), 0)
             FROM estoque_produto p
             LEFT JOIN estoque_saldo_local s ON s.produto = p.id
             WHERE p.empresa = ?1 AND p.ativo = 1
             GROUP BY p.id, p.nome, p.ncm
             ORDER BY p.nome ASC
             LIMIT 500",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map([blob(empresa)], |r| {
            Ok(ItemProdutoComSaldo {
                produto: id_de(r.get::<_, Vec<u8>>(0)?),
                nome: r.get(1)?,
                ncm: r.get(2)?,
                disponivel: Quantidade::interna(r.get::<_, i64>(3)?),
                reservado: Quantidade::interna(r.get::<_, i64>(4)?),
                custo_medio: Preco::interna(r.get::<_, i64>(5)?),
            })
        })
        .map_err(persist)?;
    linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)
}

/// Lista os produtos ativos da empresa com saldo agregado.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProdutosComSaldo;

impl Consulta for ProdutosComSaldo {
    type Saida = Vec<ItemProdutoComSaldo>;
    const PERMISSAO: &'static str = "estoque.produto.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        produtos_com_saldo(conexao, ctx.empresa)
    }
}

/// Busca um produto pelo código de barras (GTIN) — o que sustenta o bipe no balcão/PDV.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn produto_por_codigo_barras(
    conexao: &Connection,
    empresa: Id,
    codigo_barras: &str,
) -> Resultado<Option<Produto>> {
    conexao
        .query_row(
            "SELECT id, empresa, grupo_produto, nome, ncm, cest, codigo_barras, fabricante,
                    codigo_fabricante, categoria_tecnica, especificacao_tecnica,
                    compatibilidade, garantia_fornecedor_dias, localizacao_fisica,
                    controla_grade, controla_lote, controla_validade, unidade_padrao,
                    ponto_pedido, estoque_minimo, estoque_maximo, ativo, versao
             FROM estoque_produto WHERE empresa = ?1 AND codigo_barras = ?2",
            rusqlite::params![blob(empresa), codigo_barras],
            produto_de_linha,
        )
        .optional()
        .map_err(persist)
}

/// Busca um produto pelo código de barras bipado no balcão/PDV.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProdutoPorCodigoBarras {
    /// O código de barras (GTIN), como veio do leitor.
    pub codigo_barras: String,
}

impl Consulta for ProdutoPorCodigoBarras {
    type Saida = Option<Produto>;
    const PERMISSAO: &'static str = "estoque.produto.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        produto_por_codigo_barras(conexao, ctx.empresa, &self.codigo_barras)
    }
}

/// Um grupo de produto — o suficiente para popular um seletor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemGrupoProduto {
    /// O grupo.
    pub id: Id,
    /// O código curto.
    pub codigo: String,
    /// O nome.
    pub nome: String,
}

/// Todos os grupos de produto da empresa — para o formulário de cadastro de produto (não há
/// tantos grupos a ponto de precisar de teto/paginação numa PME).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GruposProduto;

impl Consulta for GruposProduto {
    type Saida = Vec<ItemGrupoProduto>;
    const PERMISSAO: &'static str = "estoque.produto.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let mut stmt = conexao
            .prepare("SELECT id, codigo, nome FROM estoque_grupo_produto WHERE empresa = ?1 ORDER BY nome ASC")
            .map_err(persist)?;
        let linhas = stmt
            .query_map([blob(ctx.empresa)], |r| {
                Ok(ItemGrupoProduto {
                    id: id_de(r.get::<_, Vec<u8>>(0)?),
                    codigo: r.get(1)?,
                    nome: r.get(2)?,
                })
            })
            .map_err(persist)?;
        linhas
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)
    }
}

/// Uma unidade de medida — o suficiente para popular um seletor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemUnidade {
    /// A unidade.
    pub id: Id,
    /// A sigla curta (ex.: "UN").
    pub sigla: String,
    /// O nome.
    pub nome: String,
}

/// Todas as unidades de medida da empresa.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Unidades;

impl Consulta for Unidades {
    type Saida = Vec<ItemUnidade>;
    const PERMISSAO: &'static str = "estoque.produto.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let mut stmt = conexao
            .prepare(
                "SELECT id, sigla, nome FROM estoque_unidade WHERE empresa = ?1 ORDER BY sigla ASC",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map([blob(ctx.empresa)], |r| {
                Ok(ItemUnidade {
                    id: id_de(r.get::<_, Vec<u8>>(0)?),
                    sigla: r.get(1)?,
                    nome: r.get(2)?,
                })
            })
            .map_err(persist)?;
        linhas
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)
    }
}

/// O saldo disponível de um produto somado entre todos os locais — versão "só este produto"
/// de [`produtos_com_saldo`], para quem já tem o `Id` do produto e não precisa da lista
/// inteira (ex.: `mod-os` cruzando itens de peça pendentes de aplicação contra o estoque
/// real, via a porta pública desta consulta — nunca lendo `estoque_saldo_local` direto,
/// `docs/contratos-internos.md` §7 regra 2).
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn saldo_disponivel_do_produto(conexao: &Connection, produto: Id) -> Resultado<Quantidade> {
    let soma: i64 = conexao
        .query_row(
            "SELECT COALESCE(SUM(quantidade_disponivel), 0) FROM estoque_saldo_local
             WHERE produto = ?1",
            [blob(produto)],
            |r| r.get(0),
        )
        .map_err(persist)?;
    Ok(Quantidade::interna(soma))
}

/// Consulta o saldo disponível de um produto (somado entre locais).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SaldoDisponivelDoProduto {
    /// O produto.
    pub produto: Id,
}

impl Consulta for SaldoDisponivelDoProduto {
    type Saida = Quantidade;
    const PERMISSAO: &'static str = "estoque.saldo.ver";

    fn executar(self, _ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        saldo_disponivel_do_produto(conexao, self.produto)
    }
}

/// Um local de estoque — o suficiente para popular um seletor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemLocal {
    /// O local.
    pub id: Id,
    /// O nome.
    pub nome: String,
    /// O tipo (`"Loja"`, `"Deposito"`, ...).
    pub tipo: String,
}

/// Todos os locais ativos da empresa.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Locais;

impl Consulta for Locais {
    type Saida = Vec<ItemLocal>;
    const PERMISSAO: &'static str = "estoque.produto.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let mut stmt = conexao
            .prepare("SELECT id, nome, tipo FROM estoque_local WHERE empresa = ?1 AND ativo = 1 ORDER BY nome ASC")
            .map_err(persist)?;
        let linhas = stmt
            .query_map([blob(ctx.empresa)], |r| {
                Ok(ItemLocal {
                    id: id_de(r.get::<_, Vec<u8>>(0)?),
                    nome: r.get(1)?,
                    tipo: r.get(2)?,
                })
            })
            .map_err(persist)?;
        linhas
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)
    }
}
