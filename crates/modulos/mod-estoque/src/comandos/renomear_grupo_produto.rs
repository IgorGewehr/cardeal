//! Renomeia um grupo da estrutura mercadológica (nome e código).

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

/// Renomeia um grupo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenomearGrupoProduto {
    /// O grupo.
    pub grupo: Id,
    /// O novo código, único na empresa.
    pub codigo: String,
    /// O novo nome.
    pub nome: String,
}

impl Comando for RenomearGrupoProduto {
    type Saida = ();
    const PERMISSAO: &'static str = "estoque.produto.editar";
    const RISCO: Risco = Risco::Baixo;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let (codigo, nome) = (self.codigo.trim(), self.nome.trim());
        if codigo.is_empty() || nome.is_empty() {
            return Err(Erro::novo(
                CodigoErro::CAMPO_OBRIGATORIO,
                "código e nome do grupo são obrigatórios",
            ));
        }
        if !RepositorioEstoque::novo(uow).renomear_grupo(
            ctx.organizacao,
            self.grupo,
            codigo,
            nome,
        )? {
            return Err(Erro::nao_encontrado("grupo de produto"));
        }
        Ok(())
    }
}
