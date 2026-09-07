//! Substitui a lista inteira de itens de um orçamento — a tela manda o vetor final.

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::{carregar_orcamento, montar_itens, NovoItemOrcamento};
use crate::repositorio::RepositorioOrcamentos;

/// Define os itens de um orçamento (substitui os que houver).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefinirItensOrcamento {
    /// O orçamento.
    pub orcamento: Id,
    /// A lista final de itens.
    pub itens: Vec<NovoItemOrcamento>,
}

impl Comando for DefinirItensOrcamento {
    type Saida = ();
    const PERMISSAO: &'static str = "orcamentos.orcamento.editar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let mut orcamento = carregar_orcamento(uow, self.orcamento)?;
        let itens =
            montar_itens(orcamento.id, self.itens).map_err(|e| Erro::de_dominio(&e))?;

        orcamento.itens_alterados().map_err(|e| Erro::de_dominio(&e))?;

        let mut repo = RepositorioOrcamentos::novo(uow);
        repo.redefinir_itens(orcamento.id, &itens)?;
        repo.atualizar_orcamento(&orcamento)?;
        Ok(())
    }
}
