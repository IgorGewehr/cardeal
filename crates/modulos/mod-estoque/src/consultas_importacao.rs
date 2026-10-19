//! Prévias das importações por planilha (estrutura e produtos): o mesmo plano do comando,
//! sem gravar.

#[cfg(feature = "sqlite")]
use cardeal_kernel::{Id, Resultado};
use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::importacao_estrutura::PreviaImportacao;
#[cfg(feature = "sqlite")]
use crate::importacao_estrutura::{planejar, NoExistente};
use crate::importacao_produtos::PreviaImportacaoProdutos;
#[cfg(feature = "sqlite")]
use crate::importacao_produtos::{Existentes, LocalExistente, ProdutoExistente, UnidadeExistente};
#[cfg(feature = "sqlite")]
use crate::repositorio::{blob, id_de, persist};

/// Os grupos que a empresa já tem (código, nome e pai), para comparar com a planilha.
#[cfg(feature = "sqlite")]
pub(crate) fn nos_existentes(conexao: &Connection, empresa: Id) -> Resultado<Vec<NoExistente>> {
    let mut stmt = conexao
        .prepare(
            "SELECT id, codigo, nome, pai FROM estoque_grupo_produto
             WHERE empresa = ?1 ORDER BY nome ASC, id ASC",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map([blob(empresa)], |r| {
            Ok(NoExistente {
                id: id_de(r.get::<_, Vec<u8>>(0)?),
                codigo: r.get(1)?,
                nome: r.get(2)?,
                pai: r.get::<_, Option<Vec<u8>>>(3)?.map(id_de),
            })
        })
        .map_err(persist)?;
    linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)
}

/// "N novos, M já existiam, K erros" antes de confirmar a importação.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviaDaImportacaoDaEstrutura {
    /// O conteúdo do CSV.
    pub texto: String,
}

impl Consulta for PreviaDaImportacaoDaEstrutura {
    type Saida = PreviaImportacao;
    const PERMISSAO: &'static str = "estoque.produto.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        Ok(planejar(&self.texto, &nos_existentes(conexao, ctx.organizacao)?).previa())
    }
}

/// Grupos, produtos e unidades da organização (o catálogo é do grupo, ADR-0017) e os
/// locais ativos do CNPJ em que se age (o estoque é de cada CNPJ).
#[cfg(feature = "sqlite")]
pub(crate) fn existentes_para_produtos(
    conexao: &Connection,
    organizacao: Id,
    empresa: Id,
) -> Resultado<Existentes> {
    fn todas<T>(
        conexao: &Connection,
        sql: &str,
        dono: Id,
        f: impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
    ) -> Resultado<Vec<T>> {
        let mut stmt = conexao.prepare(sql).map_err(persist)?;
        let linhas = stmt.query_map([blob(dono)], f).map_err(persist)?;
        linhas
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)
    }
    Ok(Existentes {
        grupos: nos_existentes(conexao, organizacao)?,
        produtos: todas(
            conexao,
            "SELECT id, nome, codigo_barras FROM estoque_produto WHERE empresa = ?1",
            organizacao,
            |r| {
                Ok(ProdutoExistente {
                    id: id_de(r.get(0)?),
                    nome: r.get(1)?,
                    codigo_barras: r.get(2)?,
                })
            },
        )?,
        unidades: todas(
            conexao,
            "SELECT id, sigla FROM estoque_unidade WHERE empresa = ?1",
            organizacao,
            |r| {
                Ok(UnidadeExistente {
                    id: id_de(r.get(0)?),
                    sigla: r.get(1)?,
                })
            },
        )?,
        locais: todas(
            conexao,
            "SELECT id, nome FROM estoque_local WHERE empresa = ?1 AND ativo = 1
             ORDER BY nome ASC, id ASC",
            empresa,
            |r| {
                Ok(LocalExistente {
                    id: id_de(r.get(0)?),
                    nome: r.get(1)?,
                })
            },
        )?,
    })
}

/// "N novos, M já existiam, K erros" antes de confirmar a importação de produtos.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviaDaImportacaoDeProdutos {
    /// O conteúdo do CSV.
    pub texto: String,
}

impl Consulta for PreviaDaImportacaoDeProdutos {
    type Saida = PreviaImportacaoProdutos;
    const PERMISSAO: &'static str = "estoque.produto.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let existentes = existentes_para_produtos(conexao, ctx.organizacao, ctx.empresa)?;
        Ok(crate::importacao_produtos::planejar(&self.texto, &existentes).previa())
    }
}
