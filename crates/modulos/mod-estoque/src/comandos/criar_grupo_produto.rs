//! Cadastra um grupo de produto na estrutura mercadológica (até quatro níveis:
//! Grupo › Subgrupo › Categoria › Subcategoria — ver [`crate::hierarquia`]).

use cardeal_kernel::Id;
#[cfg(feature = "sqlite")]
use cardeal_kernel::{CodigoErro, Erro, Resultado};
use cardeal_modkit::Comando;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
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

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let codigo = self.codigo.trim();
        let nome = self.nome.trim();
        if codigo.is_empty() || nome.is_empty() {
            return Err(Erro::novo(
                CodigoErro::CAMPO_OBRIGATORIO,
                "código e nome do grupo são obrigatórios",
            ));
        }
        let mut repo = RepositorioEstoque::novo(uow);
        crate::hierarquia::Arvore::de_nos(&repo.nos_dos_grupos(ctx.organizacao)?)
            .validar_novo_filho(self.pai)
            .map_err(crate::comandos::erro_de_hierarquia)?;
        let id = Id::novo();
        repo.inserir_grupo_produto(ctx.organizacao, id, codigo, nome, self.pai)?;
        Ok(GrupoProdutoCriado { grupo_produto: id })
    }
}
