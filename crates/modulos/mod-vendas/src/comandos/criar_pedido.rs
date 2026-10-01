//! Abre um pedido `Rascunho`, sem itens. `docs/modulos/vendas.md` §5.

use cardeal_kernel::Id;
#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_modkit::{Comando, Ctx};
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::pedido::Pedido;
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioVendas;

/// Abre um pedido novo.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CriarPedido {
    /// O cliente.
    pub cliente: Id,
    /// O vendedor responsável.
    pub vendedor: Id,
    /// A condição de pagamento (`financeiro_condicao_pagamento`).
    pub condicao_pagamento: Id,
    /// A tabela de preço a usar na resolução de preço dos itens.
    pub tabela_preco: Id,
    /// O local de estoque de onde os itens saem ao faturar.
    pub local_expedicao: Id,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct PedidoCriado {
    /// O pedido criado.
    pub pedido: Id,
}

#[cfg(feature = "sqlite")]
impl Comando for CriarPedido {
    type Saida = PedidoCriado;
    const PERMISSAO: &'static str = "vendas.pedido.criar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let pedido = Pedido::novo(
            ctx.empresa,
            self.cliente,
            self.vendedor,
            ctx.hoje(),
            self.condicao_pagamento,
            self.tabela_preco,
            self.local_expedicao,
            ctx.usuario,
        );
        RepositorioVendas::novo(uow).inserir_pedido(&pedido)?;
        Ok(PedidoCriado { pedido: pedido.id })
    }
}
