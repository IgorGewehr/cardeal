//! O `impl Modulo` de compras — o ponto por onde o motor coleta manifesto, migrações e
//! comandos (`docs/contratos-internos.md` §4). Sem lógica: só amarração.

use cardeal_kernel::Resultado;
use cardeal_modkit::{Manifesto, Modulo, Registro};
use cardeal_storage::ConjuntoMigracoes;

use crate::comandos::{
    ConfirmarEntrada, DefinirPreferenciasCompras, EstornarEntrada, ImportarNotaDeArquivoXml,
    LancarNotaManual, VincularProdutoManual,
};
use crate::consultas::{
    ItensDaNota, NotasDeEntradaDoMes, NotasRecentes, ParcelasDaNota, PreviaDoEstornoDaEntrada,
};
use crate::manifesto::MANIFESTO;
use crate::migracoes;

/// O módulo de compras, para registrar no [`Despachante`](cardeal_modkit::Despachante).
pub struct ModuloCompras;

impl Modulo for ModuloCompras {
    fn manifesto(&self) -> &'static Manifesto {
        &MANIFESTO
    }

    fn migracoes(&self) -> ConjuntoMigracoes {
        migracoes::conjunto()
    }

    fn registrar(&self, registro: &mut Registro) -> Resultado<()> {
        registro
            .comando::<DefinirPreferenciasCompras>("compras.definir_preferencias.v1")
            .comando::<VincularProdutoManual>("compras.vincular_produto_manual.v1")
            .comando::<crate::CriarProdutoDoItem>("compras.criar_produto_do_item.v1")
            .comando::<ConfirmarEntrada>("compras.confirmar_entrada.v1")
            .comando::<EstornarEntrada>("compras.estornar_entrada.v1")
            .comando::<crate::DevolverAoFornecedor>("compras.devolver_ao_fornecedor.v1")
            .comando::<crate::EmitirNfeDaDevolucao>("compras.emitir_nfe_da_devolucao.v1")
            .consulta::<crate::DevolucoesDaNota>("compras.devolucoes_da_nota.v1")
            .consulta::<crate::st::EstimativaDeStDaNota>("compras.estimativa_de_st.v1")
            .consulta::<crate::escrituracao::EntradasParaEscriturar>(
                "compras.entradas_para_escriturar.v1",
            )
            .comando::<crate::SalvarPedidoCompra>("compras.salvar_pedido.v1")
            .comando::<crate::AprovarPedidoCompra>("compras.aprovar_pedido.v1")
            .comando::<crate::CancelarPedidoCompra>("compras.cancelar_pedido.v1")
            .comando::<crate::EncerrarPedidoCompra>("compras.encerrar_pedido.v1")
            .comando::<crate::VincularNotaAoPedido>("compras.vincular_nota_ao_pedido.v1")
            .consulta::<crate::PedidosDeCompra>("compras.pedidos.v1")
            .consulta::<crate::PedidoDeCompra>("compras.pedido.v1")
            .consulta::<crate::ConferenciaComPedido>("compras.conferencia_com_pedido.v1")
            .consulta::<crate::ACaminho>("compras.a_caminho.v1")
            .consulta::<crate::UltimosPrecosPagos>("compras.ultimos_precos_pagos.v1")
            .comando::<crate::SalvarCotacao>("compras.salvar_cotacao.v1")
            .comando::<crate::RegistrarPropostaFornecedor>("compras.registrar_proposta.v1")
            .comando::<crate::DecidirCotacao>("compras.decidir_cotacao.v1")
            .comando::<crate::CancelarCotacao>("compras.cancelar_cotacao.v1")
            .consulta::<crate::Cotacoes>("compras.cotacoes.v1")
            .consulta::<crate::CotacaoCompleta>("compras.cotacao.v1")
            .consulta::<crate::ComparativoDaCotacao>("compras.comparativo_da_cotacao.v1")
            .comando::<LancarNotaManual>("compras.lancar_nota_manual.v1")
            .comando::<ImportarNotaDeArquivoXml>("compras.importar_nota_de_arquivo_xml.v1")
            .consulta::<NotasRecentes>("compras.notas_recentes.v1")
            .consulta::<ItensDaNota>("compras.itens_da_nota.v1")
            .consulta::<ParcelasDaNota>("compras.parcelas_da_nota.v1")
            .consulta::<PreviaDoEstornoDaEntrada>("compras.previa_estorno_entrada.v1")
            .consulta::<NotasDeEntradaDoMes>("compras.notas_do_mes.v1")
            .consulta::<crate::NotasNaSefaz>("compras.notas_na_sefaz.v1")
            .consulta::<crate::HoraDeBuscarNaSefaz>("compras.hora_de_buscar_na_sefaz.v1")
            .comando::<crate::ImportarDeNovoDaSefaz>("compras.importar_de_novo_da_sefaz.v1")
            .tarefa(
                crate::TAREFA_BUSCAR_NA_SEFAZ,
                "compras.entrada.importar",
                crate::dfe::sqlite::buscar_notas_na_sefaz,
            );
        Ok(())
    }
}
