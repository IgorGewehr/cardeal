//! Confirma o pedido: `Rascunho → Confirmado`. A reserva de estoque em si é responsabilidade
//! de um comando de `estoque` — este comando só faz a transição (§11.2).

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::carregar_pedido;
use crate::eventos::PedidoConfirmado;
use crate::repositorio::RepositorioVendas;

/// Confirma um pedido em `Rascunho`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ConfirmarPedido {
    /// O pedido.
    pub pedido: Id,
}

impl Comando for ConfirmarPedido {
    type Saida = ();
    const PERMISSAO: &'static str = "vendas.pedido.confirmar";
    const RISCO: Risco = Risco::Medio;

    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut pedido = carregar_pedido(uow, self.pedido)?;

        // 2. Validar (domínio puro).
        pedido.confirmar().map_err(|e| Erro::de_dominio(&e))?;

        // 4. Persistir.
        RepositorioVendas::novo(uow).atualizar_pedido(&pedido)?;

        // 5. Publicar.
        uow.publicar(PedidoConfirmado { pedido: pedido.id })
            .map_err(|e| Erro::de_dominio(&e))?;

        Ok(())
    }
}
