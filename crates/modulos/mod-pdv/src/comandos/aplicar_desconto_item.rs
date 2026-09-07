//! Aplica desconto a um item já no cupom (`F5`). `docs/modulos/pdv.md` §5.

use cardeal_kernel::{Erro, Id, Percentual, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::{carregar_cupom, limite_desconto};
use crate::repositorio::RepositorioPdv;

/// Aplica (ou substitui) o desconto de um item.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AplicarDescontoItem {
    /// O cupom.
    pub cupom: Id,
    /// O item.
    pub item: Id,
    /// O desconto em pontos percentuais.
    pub desconto_percentual: Percentual,
}

impl Comando for AplicarDescontoItem {
    type Saida = ();
    const PERMISSAO: &'static str = "pdv.desconto.aplicar";
    const RISCO: Risco = Risco::Medio;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut cupom = carregar_cupom(uow, self.cupom)?;
        let item = cupom
            .itens
            .iter_mut()
            .find(|i| i.id == self.item)
            .ok_or_else(|| Erro::nao_encontrado("item do cupom"))?;

        // 2. Validar (domínio puro): recusa acima do teto do papel.
        item.aplicar_desconto(self.desconto_percentual, limite_desconto(ctx))
            .map_err(|e| Erro::de_dominio(&e))?;
        let item = item.clone();
        cupom.recalcular();

        // 4. Persistir.
        let mut repo = RepositorioPdv::novo(uow);
        repo.atualizar_item(&item)?;
        repo.atualizar_cupom(&cupom)?;
        Ok(())
    }
}
