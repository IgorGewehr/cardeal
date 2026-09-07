//! Cancela um item do cupom (`F7`) — a linha continua na tabela, auditável, só sai do total.
//! `docs/modulos/pdv.md` §5.

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::carregar_cupom;
use crate::eventos::ItemCancelado;
use crate::repositorio::RepositorioPdv;

/// Cancela um item do cupom.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelarItem {
    /// O cupom.
    pub cupom: Id,
    /// O item.
    pub item: Id,
    /// O motivo do cancelamento.
    pub motivo: String,
}

impl Comando for CancelarItem {
    type Saida = ();
    const PERMISSAO: &'static str = "pdv.item.cancelar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut cupom = carregar_cupom(uow, self.cupom)?;
        let item = cupom
            .itens
            .iter_mut()
            .find(|i| i.id == self.item)
            .ok_or_else(|| Erro::nao_encontrado("item do cupom"))?;

        // 2. Validar (domínio puro).
        item.cancelar(self.motivo.clone())
            .map_err(|e| Erro::de_dominio(&e))?;
        let item = item.clone();
        cupom.recalcular();

        // 4. Persistir.
        let mut repo = RepositorioPdv::novo(uow);
        repo.atualizar_item(&item)?;
        repo.atualizar_cupom(&cupom)?;

        // 5. Publicar.
        uow.publicar(ItemCancelado {
            cupom: cupom.id,
            item: item.id,
            motivo: self.motivo,
        })
        .map_err(|e| Erro::de_dominio(&e))?;

        Ok(())
    }
}
