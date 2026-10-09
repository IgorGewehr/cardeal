//! Passa vários produtos para outro grupo de uma vez — a "transferência de grupo" da
//! alteração em lote. Tudo-ou-nada: um produto inexistente desfaz o lote.

use cardeal_kernel::Id;
#[cfg(feature = "sqlite")]
use cardeal_kernel::{Erro, Resultado};
use cardeal_modkit::Comando;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioEstoque;

/// Move os produtos para o grupo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoverProdutosDeGrupo {
    /// Os produtos.
    pub produtos: Vec<Id>,
    /// O grupo de destino.
    pub grupo: Id,
}

/// Quantos produtos de fato mudaram (os que já estavam no destino não contam).
#[derive(Debug, Serialize, Deserialize)]
pub struct ProdutosMovidos {
    /// A contagem.
    pub movidos: u32,
}

impl Comando for MoverProdutosDeGrupo {
    type Saida = ProdutosMovidos;
    const PERMISSAO: &'static str = "estoque.produto.editar";
    const RISCO: Risco = Risco::Medio;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let mut repo = RepositorioEstoque::novo(uow);
        if !repo.grupo_existe(ctx.organizacao, self.grupo)? {
            return Err(Erro::nao_encontrado("grupo de produto"));
        }
        for p in &self.produtos {
            repo.buscar_produto(*p)?
                .ok_or_else(|| Erro::nao_encontrado("produto"))?;
        }
        let n = repo.mover_produtos_de_grupo(ctx.organizacao, &self.produtos, self.grupo)?;
        Ok(ProdutosMovidos {
            movidos: u32::try_from(n).unwrap_or(u32::MAX),
        })
    }
}
