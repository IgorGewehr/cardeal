//! Ativa ou desativa um produto. Desativado some das consultas padrão (`ProdutosComSaldo` e
//! afins já filtram `ativo = 1`) — é o "excluir" que a tela de Estoque expõe: nada é
//! apagado (saldo, lotes, histórico de movimentações continuam intactos), e um produto com
//! movimentação não pode virar órfão. Reversível a qualquer momento.

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

/// Ativa ou desativa um produto.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DefinirAtivoProduto {
    /// O produto.
    pub produto: Id,
    /// `false` desativa (some da busca padrão); `true` reativa.
    pub ativo: bool,
}

impl Comando for DefinirAtivoProduto {
    type Saida = ();
    const PERMISSAO: &'static str = "estoque.produto.editar";
    const RISCO: Risco = Risco::Baixo;

    #[cfg(feature = "sqlite")]
    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let mut repo = RepositorioEstoque::novo(uow);
        repo.buscar_produto(self.produto)?
            .ok_or_else(|| Erro::nao_encontrado("produto"))?;
        repo.atualizar_ativo(self.produto, self.ativo)?;
        Ok(())
    }
}
