//! O `impl Modulo` de vendas — o ponto por onde o motor coleta manifesto, migrações e
//! comandos (`docs/contratos-internos.md` §4). Sem lógica: só amarração.

use cardeal_kernel::Resultado;
use cardeal_modkit::{Manifesto, Modulo, Registro};
use cardeal_storage::ConjuntoMigracoes;

use crate::comandos::{
    AdicionarItemPedido, CancelarPedido, ConfirmarPedido, CriarPedido, CriarRegraPreco,
    CriarTabelaPreco, FaturarPedido,
};
use crate::consultas::{ItensDoPedido, PedidosRecentes, RegrasDaTabela, TabelasDePreco};
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
            .comando::<crate::comandos::AplicarPrecosSugeridos>(
                "vendas.aplicar_precos_sugeridos.v1",
            )
            .comando::<CriarPedido>("vendas.criar_pedido.v1")
            .comando::<AdicionarItemPedido>("vendas.adicionar_item_pedido.v1")
            .comando::<ConfirmarPedido>("vendas.confirmar_pedido.v1")
            .comando::<CancelarPedido>("vendas.cancelar_pedido.v1")
            .comando::<FaturarPedido>("vendas.faturar_pedido.v1")
            .comando::<crate::SolicitarDevolucao>("vendas.solicitar_devolucao.v1")
            .comando::<crate::AprovarDevolucao>("vendas.aprovar_devolucao.v1")
            .comando::<crate::CancelarDevolucao>("vendas.cancelar_devolucao.v1")
            .comando::<crate::ConcluirDevolucao>("vendas.concluir_devolucao.v1")
            .consulta::<crate::DevolucoesDoPedido>("vendas.devolucoes_do_pedido.v1")
            .consulta::<crate::DevolucoesPendentes>("vendas.devolucoes_pendentes.v1")
            .comando::<crate::ApurarComissoes>("vendas.apurar_comissoes.v1")
            .comando::<crate::PagarComissoes>("vendas.pagar_comissoes.v1")
            .consulta::<crate::ComissoesDoMes>("vendas.comissoes_do_mes.v1")
            .consulta::<PedidosRecentes>("vendas.pedidos_recentes.v1")
            .consulta::<TabelasDePreco>("vendas.tabelas_de_preco.v1")
            .consulta::<RegrasDaTabela>("vendas.regras_da_tabela.v1")
            .consulta::<ItensDoPedido>("vendas.itens_do_pedido.v1")
            .consulta::<crate::MargemDoPedido>("vendas.margem_do_pedido.v1")
            .consulta::<crate::PrecosDefasados>("vendas.precos_defasados.v1")
            .consulta::<crate::ReceitaPorProduto>("vendas.receita_por_produto.v1");
        Ok(())
    }
}
