//! As consultas do dicionário de nomes: os termos e o nome sugerido para uma descrição de
//! fornecedor (ver [`crate::padronizacao`]).

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::padronizacao::TermoDicionario;

/// Os termos do dicionário DE/PARA, em ordem alfabética.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DicionarioDeNomes;

impl Consulta for DicionarioDeNomes {
    type Saida = Vec<TermoDicionario>;
    const PERMISSAO: &'static str = "estoque.produto.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        crate::repositorio_dicionario::termos(conexao, ctx.organizacao)
    }
}

/// O nome sugerido para a descrição de um fornecedor (o `xProd` da nota), com o dicionário da
/// empresa.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SugerirNomeDoProduto {
    /// A descrição como veio.
    pub descricao: String,
}

impl Consulta for SugerirNomeDoProduto {
    type Saida = String;
    const PERMISSAO: &'static str = "estoque.produto.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let dicionario = crate::repositorio_dicionario::dicionario(conexao, ctx.organizacao)?;
        Ok(crate::padronizacao::sugerir_nome(
            &self.descricao,
            &dicionario,
        ))
    }
}
