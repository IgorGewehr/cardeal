//! Prévia da importação da estrutura por planilha: o mesmo plano do comando, sem gravar.

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
