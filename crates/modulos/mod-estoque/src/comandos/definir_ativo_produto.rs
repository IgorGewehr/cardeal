//! Ativa ou desativa um produto. Desativado some das consultas padrão (`ProdutosComSaldo` e
//! afins já filtram `ativo = 1`) — é o "excluir" que a tela de Estoque expõe: nada é
//! apagado (saldo, lotes, histórico de movimentações continuam intactos), e um produto com
//! movimentação não pode virar órfão. Reversível a qualquer momento.

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

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

    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let mut repo = RepositorioEstoque::novo(uow);
        repo.buscar_produto(self.produto)?
            .ok_or_else(|| Erro::nao_encontrado("produto"))?;
        repo.atualizar_ativo(self.produto, self.ativo)?;
        Ok(())
    }
}
