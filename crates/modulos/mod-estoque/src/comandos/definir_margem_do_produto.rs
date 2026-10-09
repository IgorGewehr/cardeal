//! Define a margem líquida própria de um produto, que vale mais que a do grupo. `None`
//! volta a herdar.

#[cfg(feature = "sqlite")]
use cardeal_kernel::{Erro, Resultado};
use cardeal_kernel::{Id, Percentual};
use cardeal_modkit::Comando;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioEstoque;
#[cfg(feature = "sqlite")]
use crate::repositorio_preco as sql;

/// Define a margem do produto.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DefinirMargemDoProduto {
    /// O produto.
    pub produto: Id,
    /// A margem; `None` limpa.
    pub margem: Option<Percentual>,
}

impl Comando for DefinirMargemDoProduto {
    type Saida = ();
    const PERMISSAO: &'static str = "estoque.produto.editar";
    const RISCO: Risco = Risco::Medio;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        crate::comandos::definir_margem_do_grupo::validar_margem(self.margem)?;
        let repo = RepositorioEstoque::novo(uow);
        if !sql::definir_margem_do_produto(repo.conn(), ctx.organizacao, self.produto, self.margem)?
        {
            return Err(Erro::nao_encontrado("produto"));
        }
        Ok(())
    }
}
