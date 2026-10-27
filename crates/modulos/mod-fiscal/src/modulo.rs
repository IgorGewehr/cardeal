//! O `impl Modulo` — só amarração.

use cardeal_kernel::Resultado;
use cardeal_modkit::{Manifesto, Modulo, Registro};
use cardeal_storage::ConjuntoMigracoes;

use crate::comandos::{DefinirAmbienteFiscal, InstalarCertificado, RemoverCertificado};
use crate::consultas::{SituacaoFiscal, StatusDaSefaz};
use crate::correcao::{CartasDeCorrecao, EmitirCartaDeCorrecao};
use crate::fechamento::NotasFiscaisDoMes;
use crate::manifesto::MANIFESTO;
use crate::nfce::{
    AnularDocumentoDaVenda, BuscarXmlsQueFaltam, CancelarNfce, ConfiguracaoDaNfce, ConfigurarNfce,
    DocumentoDaVenda, DocumentoFiscal, DocumentosDasVendas, DocumentosFiscais, EmitirNfceDoCupom,
    InutilizarNumero, ReenviarDocumento, ResumoDaFilaFiscal, TAREFA_TRANSMITIR,
};
use crate::nfe_venda::{ConfiguracaoDaNfe, ConfigurarNfe, DocumentoDoPedido, EmitirNfeDoPedido};
use crate::terminal::{DefinirSerieDoTerminal, SeriesDosTerminais};

/// O módulo, para registrar no [`Despachante`](cardeal_modkit::Despachante).
pub struct ModuloFiscal;

impl Modulo for ModuloFiscal {
    fn manifesto(&self) -> &'static Manifesto {
        &MANIFESTO
    }

    fn migracoes(&self) -> ConjuntoMigracoes {
        crate::migracoes::conjunto()
    }

    fn registrar(&self, registro: &mut Registro) -> Resultado<()> {
        registro
            .comando::<InstalarCertificado>("fiscal.instalar_certificado.v1")
            .comando::<RemoverCertificado>("fiscal.remover_certificado.v1")
            .comando::<DefinirAmbienteFiscal>("fiscal.definir_ambiente.v1")
            .consulta::<SituacaoFiscal>("fiscal.situacao.v1")
            .consulta::<StatusDaSefaz>("fiscal.status_sefaz.v1")
            .comando::<ConfigurarNfce>("fiscal.configurar_nfce.v1")
            .comando::<EmitirNfceDoCupom>("fiscal.emitir_nfce_do_cupom.v1")
            .comando::<ReenviarDocumento>("fiscal.reenviar_documento.v1")
            .comando::<CancelarNfce>("fiscal.cancelar_nfce.v1")
            .comando::<InutilizarNumero>("fiscal.inutilizar_numero.v1")
            .comando::<AnularDocumentoDaVenda>("fiscal.anular_documento_da_venda.v1")
            .comando::<DefinirSerieDoTerminal>("fiscal.definir_serie_do_terminal.v1")
            .comando::<ConfigurarNfe>("fiscal.configurar_nfe.v1")
            .comando::<EmitirCartaDeCorrecao>("fiscal.emitir_carta_de_correcao.v1")
            .consulta::<CartasDeCorrecao>("fiscal.cartas_de_correcao.v1")
            .consulta::<crate::devolucao_compra::DocumentoDaDevolucao>(
                "fiscal.documento_da_devolucao.v1",
            )
            .comando::<crate::devolucao_venda::EmitirNfeDaDevolucaoDeVenda>(
                "fiscal.emitir_nfe_da_devolucao_de_venda.v1",
            )
            .consulta::<crate::devolucao_venda::DocumentoDaDevolucaoDeVenda>(
                "fiscal.documento_da_devolucao_de_venda.v1",
            )
            .consulta::<ConfiguracaoDaNfe>("fiscal.configuracao_nfe.v1")
            .comando::<EmitirNfeDoPedido>("fiscal.emitir_nfe_do_pedido.v1")
            .consulta::<DocumentoDoPedido>("fiscal.documento_do_pedido.v1")
            .consulta::<SeriesDosTerminais>("fiscal.series_dos_terminais.v1")
            .consulta::<ConfiguracaoDaNfce>("fiscal.configuracao_nfce.v1")
            .consulta::<DocumentosFiscais>("fiscal.documentos.v1")
            .consulta::<ResumoDaFilaFiscal>("fiscal.resumo_da_fila.v1")
            .consulta::<DocumentoFiscal>("fiscal.documento.v1")
            .consulta::<DocumentoDaVenda>("fiscal.documento_da_venda.v1")
            .consulta::<DocumentosDasVendas>("fiscal.documentos_das_vendas.v1")
            .consulta::<NotasFiscaisDoMes>("fiscal.notas_do_mes.v1")
            .comando::<BuscarXmlsQueFaltam>("fiscal.buscar_xmls_que_faltam.v1")
            .tarefa(
                TAREFA_TRANSMITIR,
                "fiscal.emitir",
                crate::nfce::sqlite::transmitir_pendentes,
            );
        Ok(())
    }
}
