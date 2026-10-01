//! Aplica desconto a um item já no cupom (`F5`). `docs/modulos/pdv.md` §5.

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
use crate::comandos::{carregar_cupom, limite_desconto};
#[cfg(feature = "sqlite")]
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

    #[cfg(feature = "sqlite")]
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
