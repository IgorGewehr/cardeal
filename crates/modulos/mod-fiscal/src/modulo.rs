//! O `impl Modulo` — só amarração.

use cardeal_kernel::Resultado;
use cardeal_modkit::{Manifesto, Modulo, Registro};
use cardeal_storage::ConjuntoMigracoes;

use crate::comandos::{DefinirAmbienteFiscal, InstalarCertificado, RemoverCertificado};
use crate::consultas::{SituacaoFiscal, StatusDaSefaz};
use crate::manifesto::MANIFESTO;
use crate::nfce::{
    AnularDocumentoDaVenda, CancelarNfce, ConfiguracaoDaNfce, ConfigurarNfce, DocumentoDaVenda,
    DocumentoFiscal, DocumentosDasVendas, DocumentosFiscais, EmitirNfceDoCupom, InutilizarNumero,
    ReenviarDocumento, ResumoDaFilaFiscal, TAREFA_TRANSMITIR,
};

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
            .consulta::<ConfiguracaoDaNfce>("fiscal.configuracao_nfce.v1")
            .consulta::<DocumentosFiscais>("fiscal.documentos.v1")
            .consulta::<ResumoDaFilaFiscal>("fiscal.resumo_da_fila.v1")
            .consulta::<DocumentoFiscal>("fiscal.documento.v1")
            .consulta::<DocumentoDaVenda>("fiscal.documento_da_venda.v1")
            .consulta::<DocumentosDasVendas>("fiscal.documentos_das_vendas.v1")
            .tarefa(
                TAREFA_TRANSMITIR,
                "fiscal.emitir",
                crate::nfce::sqlite::transmitir_pendentes,
            );
        Ok(())
    }
}
