//! Remove um item (peça ou mão de obra) do orçamento antes de enviar para aprovação.
//!
//! `docs/modulos/os.md` §5 — não estava no spec original: sem isto, um item digitado errado
//! só se corrige cancelando a OS inteira e recomeçando do zero.

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::carregar_ordem;
use crate::repositorio::RepositorioOs;

/// O tipo do item a remover.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TipoItemOrcamento {
    /// Uma peça do estoque.
    Peca,
    /// Um serviço de mão de obra.
    MaoDeObra,
}

/// Remove um item do orçamento.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RemoverItemOrcamento {
    /// A ordem de serviço.
    pub ordem_servico: Id,
    /// O item a remover.
    pub item: Id,
    /// O tipo do item.
    pub tipo: TipoItemOrcamento,
}

impl Comando for RemoverItemOrcamento {
    type Saida = ();
    const PERMISSAO: &'static str = "os.orcamento.montar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut os = carregar_ordem(uow, self.ordem_servico)?;
        let total_do_item = match self.tipo {
            TipoItemOrcamento::Peca => {
                let item = RepositorioOs::novo(uow)
                    .buscar_item_peca(self.item)?
                    .ok_or_else(|| Erro::nao_encontrado("item de peça"))?;
                item.total_cobrado()
            }
            TipoItemOrcamento::MaoDeObra => {
                let item = RepositorioOs::novo(uow)
                    .buscar_item_mao_de_obra(self.item)?
                    .ok_or_else(|| Erro::nao_encontrado("item de mão de obra"))?;
                item.valor
            }
        };

        // 2. Validar (domínio puro): só antes da aprovação.
        os.remover_do_orcamento(total_do_item)
            .map_err(|e| Erro::de_dominio(&e))?;

        // 4. Persistir.
        let mut repo = RepositorioOs::novo(uow);
        match self.tipo {
            TipoItemOrcamento::Peca => repo.remover_item_peca(self.item)?,
            TipoItemOrcamento::MaoDeObra => repo.remover_item_mao_de_obra(self.item)?,
        }
        repo.atualizar_ordem(&os)?;

        Ok(())
    }
}
