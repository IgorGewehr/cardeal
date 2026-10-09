//! Muda um grupo (com toda a sua subárvore) de lugar na estrutura mercadológica. Recusa
//! o que passaria de quatro níveis e o que criaria ciclo.

use cardeal_kernel::Id;
#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
use cardeal_modkit::Comando;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioEstoque;

/// Move um grupo para baixo de outro (ou para a raiz, com `novo_pai: None`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MoverGrupoProduto {
    /// O grupo a mover.
    pub grupo: Id,
    /// O novo pai; `None` o promove a grupo de primeiro nível.
    pub novo_pai: Option<Id>,
}

impl Comando for MoverGrupoProduto {
    type Saida = ();
    const PERMISSAO: &'static str = "estoque.produto.editar";
    const RISCO: Risco = Risco::Medio;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let mut repo = RepositorioEstoque::novo(uow);
        crate::hierarquia::Arvore::de_nos(&repo.nos_dos_grupos(ctx.organizacao)?)
            .validar_movimento(self.grupo, self.novo_pai)
            .map_err(crate::comandos::erro_de_hierarquia)?;
        repo.mover_grupo(ctx.organizacao, self.grupo, self.novo_pai)
    }
}
