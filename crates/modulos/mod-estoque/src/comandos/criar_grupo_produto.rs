//! Cadastra um grupo de produto — fatia mínima: hierarquia por `pai`, sem perfil tributário
//! ainda (ver `src/lib.rs`).

use cardeal_kernel::{CodigoErro, Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::repositorio::RepositorioEstoque;

/// Cadastra um grupo de produto.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriarGrupoProduto {
    /// Código estável, único na empresa.
    pub codigo: String,
    /// Nome do grupo.
    pub nome: String,
    /// O grupo pai, quando hierárquico.
    pub pai: Option<Id>,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct GrupoProdutoCriado {
    /// O grupo criado.
    pub grupo_produto: Id,
}

impl Comando for CriarGrupoProduto {
    type Saida = GrupoProdutoCriado;
    const PERMISSAO: &'static str = "estoque.produto.criar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let codigo = self.codigo.trim();
        let nome = self.nome.trim();
        if codigo.is_empty() || nome.is_empty() {
            return Err(Erro::novo(
                CodigoErro::CAMPO_OBRIGATORIO,
                "código e nome do grupo são obrigatórios",
            ));
        }
        let id = Id::novo();
        RepositorioEstoque::novo(uow).inserir_grupo_produto(
            ctx.empresa,
            id,
            codigo,
            nome,
            self.pai,
        )?;
        Ok(GrupoProdutoCriado { grupo_produto: id })
    }
}
