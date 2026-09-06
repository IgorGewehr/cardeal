//! Os comandos de vendas (`docs/modulos/vendas.md` §5). Um arquivo, um comando, nas cinco
//! etapas de `docs/15-convencoes-codigo.md` §3. Orçamento, devolução, comissão e contrato
//! recorrente ficam para depois (ver `src/lib.rs`).

mod adicionar_item_pedido;
mod cancelar_pedido;
mod confirmar_pedido;
mod criar_pedido;
mod criar_regra_preco;
mod criar_tabela_preco;
mod faturar_pedido;

pub use adicionar_item_pedido::{AdicionarItemPedido, ItemFoiAdicionado};
pub use cancelar_pedido::CancelarPedido;
pub use confirmar_pedido::ConfirmarPedido;
pub use criar_pedido::{CriarPedido, PedidoCriado};
pub use criar_regra_preco::{CriarRegraPreco, RegraPrecoCriada};
pub use criar_tabela_preco::{CriarTabelaPreco, TabelaPrecoCriada};
pub use faturar_pedido::{FaturarPedido, PedidoFoiFaturado};

use cardeal_auth::ValorLimite;
use cardeal_kernel::{Erro, Id, Percentual, Resultado};
use cardeal_modkit::Ctx;
use cardeal_storage::UnidadeDeTrabalho;

use crate::pedido::Pedido;
use crate::repositorio::RepositorioVendas;

/// Carrega um pedido ou devolve `NAO_ENCONTRADO`. Usado por todo comando que opera sobre um
/// pedido já existente.
pub(crate) fn carregar_pedido(uow: &mut UnidadeDeTrabalho, id: Id) -> Resultado<Pedido> {
    RepositorioVendas::novo(uow)
        .buscar_pedido(id)?
        .ok_or_else(|| Erro::nao_encontrado("pedido"))
}

/// A [`crate::receituario::Autoria`] extraída de um [`Ctx`] de comando.
pub(crate) fn autoria_de(ctx: &Ctx) -> crate::receituario::Autoria {
    crate::receituario::Autoria {
        usuario: ctx.usuario,
        dispositivo: ctx.dispositivo,
        agora: ctx.agora,
    }
}

/// O teto de desconto do papel do usuário (`vendas.desconto_maximo`, §11.4). Sem limite
/// configurado para o papel, o teto é zero — desconto exige um limite explícito.
pub(crate) fn limite_desconto(ctx: &Ctx) -> Percentual {
    match ctx.limite("vendas.desconto_maximo") {
        Some(ValorLimite::Percentual(p)) => p,
        Some(ValorLimite::Ilimitado) => Percentual::CEM,
        _ => Percentual::ZERO,
    }
}
