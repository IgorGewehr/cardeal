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
