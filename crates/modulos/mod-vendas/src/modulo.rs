//! O `impl Modulo` de vendas — o ponto por onde o motor coleta manifesto, migrações e
//! comandos (`docs/contratos-internos.md` §4). Sem lógica: só amarração.

use cardeal_kernel::Resultado;
use cardeal_modkit::{Manifesto, Modulo, Registro};
use cardeal_storage::ConjuntoMigracoes;

use crate::comandos::{
    AdicionarItemPedido, CancelarPedido, ConfirmarPedido, CriarPedido, CriarRegraPreco,
    CriarTabelaPreco, FaturarPedido,
};
use crate::consultas::PedidosRecentes;
use crate::manifesto::MANIFESTO;
use crate::migracoes;

/// O módulo de vendas, para registrar no [`Despachante`](cardeal_modkit::Despachante).
pub struct ModuloVendas;

impl Modulo for ModuloVendas {
    fn manifesto(&self) -> &'static Manifesto {
        &MANIFESTO
    }

    fn migracoes(&self) -> ConjuntoMigracoes {
        migracoes::conjunto()
    }

    fn registrar(&self, registro: &mut Registro) -> Resultado<()> {
        registro
            .comando::<CriarTabelaPreco>("vendas.criar_tabela_preco.v1")
            .comando::<CriarRegraPreco>("vendas.criar_regra_preco.v1")
            .comando::<CriarPedido>("vendas.criar_pedido.v1")
            .comando::<AdicionarItemPedido>("vendas.adicionar_item_pedido.v1")
            .comando::<ConfirmarPedido>("vendas.confirmar_pedido.v1")
            .comando::<CancelarPedido>("vendas.cancelar_pedido.v1")
            .comando::<FaturarPedido>("vendas.faturar_pedido.v1")
            .consulta::<PedidosRecentes>("vendas.pedidos_recentes.v1");
        Ok(())
    }
}
