//! O `impl Modulo` do estoque — o ponto por onde o motor coleta manifesto, migrações e
//! comandos (`docs/contratos-internos.md` §4). Sem lógica: só amarração.

use cardeal_kernel::Resultado;
use cardeal_modkit::{Manifesto, Modulo, Registro};
use cardeal_storage::ConjuntoMigracoes;

use crate::comandos::{
    AjustarSaldo, CriarGrupoProduto, CriarLocal, CriarProduto, CriarUnidade, DefinirAtivoProduto,
    DefinirPontoPedido, EditarDetalhesTecnicosProduto, MoverGrupoProduto, MoverProdutosDeGrupo,
    RegistrarAparelhoOrigem, RegistrarEntrada, RegistrarEntradaComLote, RegistrarSaida,
    RenomearGrupoProduto,
};
use crate::comandos::{
    AplicarAlteracaoEmLote, DefinirCamposFiscaisDoGrupo, DefinirCamposFiscaisDoProduto,
    DesfazerAlteracaoEmLote,
};
use crate::comandos::{
    DefinirMargemDoGrupo, DefinirMargemDoProduto, DefinirParametrosPreco,
    DefinirPerfilFiscalDoGrupo,
};
use crate::consultas::{
    ArvoreGruposProduto, DetalheDoLotePorCodigo, GruposProduto, Locais, LotesDisponiveisDoProduto,
    MovimentosDoProduto, ProdutoPorCodigoBarras, ProdutoPorId, ProdutosAbaixoDoPontoPedido,
    ProdutosComSaldo, SaldoDisponivelDoProduto, Unidades,
};
use crate::consultas_analise::AnaliseDeEstoque;
use crate::consultas_lote::{
    HistoricoDeAlteracoesEmLote, PerfilFiscalDoProduto, PreviaDaAlteracaoEmLote, ProdutosParaLote,
};
use crate::consultas_preco::{ParametrosDePreco, PrecoSugeridoDoProduto, PrecosSugeridosDoGrupo};
use crate::manifesto::MANIFESTO;
use crate::migracoes;

/// O módulo de estoque, para registrar no [`Despachante`](cardeal_modkit::Despachante).
pub struct ModuloEstoque;

impl Modulo for ModuloEstoque {
    fn manifesto(&self) -> &'static Manifesto {
        &MANIFESTO
    }

    fn migracoes(&self) -> ConjuntoMigracoes {
        migracoes::conjunto()
    }

    fn registrar(&self, registro: &mut Registro) -> Resultado<()> {
        registro
            .comando::<CriarGrupoProduto>("estoque.criar_grupo_produto.v1")
            .comando::<RenomearGrupoProduto>("estoque.renomear_grupo_produto.v1")
            .comando::<crate::ImportarEstrutura>("estoque.importar_estrutura.v1")
            .comando::<crate::ImportarProdutos>("estoque.importar_produtos.v1")
            .comando::<crate::CriarInventario>("estoque.criar_inventario.v1")
            .comando::<crate::IniciarContagem>("estoque.iniciar_contagem.v1")
            .comando::<crate::RegistrarContagem>("estoque.registrar_contagem.v1")
            .comando::<crate::EncerrarInventario>("estoque.encerrar_inventario.v1")
            .comando::<crate::CancelarInventario>("estoque.cancelar_inventario.v1")
            .comando::<MoverGrupoProduto>("estoque.mover_grupo_produto.v1")
            .comando::<MoverProdutosDeGrupo>("estoque.mover_produtos_de_grupo.v1")
            .comando::<DefinirMargemDoGrupo>("estoque.definir_margem_do_grupo.v1")
            .comando::<DefinirMargemDoProduto>("estoque.definir_margem_do_produto.v1")
            .comando::<crate::DefinirComissaoDoGrupo>("estoque.definir_comissao_do_grupo.v1")
            .comando::<crate::DefinirComissaoDoProduto>("estoque.definir_comissao_do_produto.v1")
            .comando::<DefinirPerfilFiscalDoGrupo>("estoque.definir_perfil_fiscal_do_grupo.v1")
            .comando::<DefinirParametrosPreco>("estoque.definir_parametros_preco.v1")
            .comando::<DefinirCamposFiscaisDoGrupo>("estoque.definir_campos_fiscais_do_grupo.v1")
            .comando::<DefinirCamposFiscaisDoProduto>(
                "estoque.definir_campos_fiscais_do_produto.v1",
            )
            .comando::<AplicarAlteracaoEmLote>("estoque.aplicar_alteracao_em_lote.v1")
            .comando::<DesfazerAlteracaoEmLote>("estoque.desfazer_alteracao_em_lote.v1")
            .comando::<CriarUnidade>("estoque.criar_unidade.v1")
            .comando::<CriarProduto>("estoque.criar_produto.v1")
            .comando::<crate::comandos::CriarPecaRapida>("estoque.criar_peca_rapida.v1")
            .comando::<EditarDetalhesTecnicosProduto>("estoque.editar_detalhes_tecnicos_produto.v1")
            .comando::<DefinirPontoPedido>("estoque.definir_ponto_pedido.v1")
            .comando::<DefinirAtivoProduto>("estoque.definir_ativo_produto.v1")
            .comando::<CriarLocal>("estoque.criar_local.v1")
            .comando::<RegistrarEntrada>("estoque.registrar_entrada.v1")
            .comando::<RegistrarEntradaComLote>("estoque.registrar_entrada_com_lote.v1")
            .comando::<RegistrarSaida>("estoque.registrar_saida.v1")
            .comando::<AjustarSaldo>("estoque.ajustar_saldo.v1")
            .comando::<RegistrarAparelhoOrigem>("estoque.registrar_aparelho_origem.v1")
            .consulta::<ProdutosComSaldo>("estoque.produtos_com_saldo.v1")
            .consulta::<crate::listagem::ListarProdutos>("estoque.produtos.v2")
            .consulta::<ProdutoPorCodigoBarras>("estoque.produto_por_codigo_barras.v1")
            .consulta::<ProdutoPorId>("estoque.produto_por_id.v1")
            .consulta::<SaldoDisponivelDoProduto>("estoque.saldo_disponivel_do_produto.v1")
            .consulta::<ProdutosAbaixoDoPontoPedido>("estoque.produtos_abaixo_do_ponto_pedido.v1")
            .consulta::<MovimentosDoProduto>("estoque.movimentos_do_produto.v1")
            .consulta::<GruposProduto>("estoque.grupos_produto.v1")
            .consulta::<ArvoreGruposProduto>("estoque.arvore_grupos_produto.v1")
            .consulta::<crate::PreviaDaImportacaoDaEstrutura>(
                "estoque.previa_importacao_estrutura.v1",
            )
            .consulta::<crate::PreviaDaImportacaoDeProdutos>(
                "estoque.previa_importacao_produtos.v1",
            )
            .consulta::<crate::Inventarios>("estoque.inventarios.v1")
            .consulta::<crate::PosicaoDoInventario>("estoque.posicao_do_inventario.v1")
            .consulta::<ParametrosDePreco>("estoque.parametros_de_preco.v1")
            .consulta::<PrecoSugeridoDoProduto>("estoque.preco_sugerido_do_produto.v1")
            .consulta::<PrecosSugeridosDoGrupo>("estoque.precos_sugeridos_do_grupo.v1")
            .consulta::<PerfilFiscalDoProduto>("estoque.perfil_fiscal_do_produto.v1")
            .consulta::<crate::consultas_compras::HistoricoDeCompras>(
                "estoque.historico_de_compras.v1",
            )
            .consulta::<AnaliseDeEstoque>("estoque.analise_de_estoque.v1")
            .consulta::<crate::SaidasPorProdutoEOrigem>("estoque.saidas_por_produto_e_origem.v1")
            .consulta::<crate::EncargosAtuais>("estoque.encargos_atuais.v1")
            .consulta::<ProdutosParaLote>("estoque.produtos_para_lote.v1")
            .consulta::<PreviaDaAlteracaoEmLote>("estoque.previa_da_alteracao_em_lote.v1")
            .consulta::<HistoricoDeAlteracoesEmLote>("estoque.historico_de_alteracoes_em_lote.v1")
            .consulta::<Unidades>("estoque.unidades.v1")
            .consulta::<Locais>("estoque.locais.v1")
            .consulta::<DetalheDoLotePorCodigo>("estoque.detalhe_do_lote_por_codigo.v1")
            .consulta::<LotesDisponiveisDoProduto>("estoque.lotes_disponiveis_do_produto.v1");
        Ok(())
    }
}
