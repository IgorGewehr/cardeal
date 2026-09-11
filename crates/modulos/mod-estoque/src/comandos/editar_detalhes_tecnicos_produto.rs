//! Edita os detalhes técnicos de um produto já cadastrado — fabricante, MPN, categoria
//! técnica, especificação, compatibilidade, garantia do fornecedor e localização física.
//! Não toca nome/NCM/código de barras, que continuam sem comando de edição nesta fatia
//! (`docs/modulos/estoque.md` §5).

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::produto::DetalhesTecnicos;
use crate::repositorio::RepositorioEstoque;

/// Edita os detalhes técnicos de um produto.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditarDetalhesTecnicosProduto {
    /// O produto.
    pub produto: Id,
    /// Os novos detalhes técnicos — substituem por completo os anteriores (campo não
    /// informado vira `None`, não "mantém o que já tinha").
    pub detalhes: DetalhesTecnicos,
}

impl Comando for EditarDetalhesTecnicosProduto {
    type Saida = ();
    const PERMISSAO: &'static str = "estoque.produto.editar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut repo = RepositorioEstoque::novo(uow);
        let produto = repo
            .buscar_produto(self.produto)?
            .ok_or_else(|| Erro::nao_encontrado("produto"))?;

        // 2. Validar (domínio puro) — normaliza vazio para `None`.
        let produto = produto.com_detalhes_tecnicos(self.detalhes);

        // 4. Persistir.
        repo.atualizar_detalhes_tecnicos(&produto)?;

        Ok(())
    }
}
