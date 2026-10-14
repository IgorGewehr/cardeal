//! O `impl Modulo` — só amarração.

use cardeal_kernel::Resultado;
use cardeal_modkit::{Manifesto, Modulo, Registro};
use cardeal_storage::ConjuntoMigracoes;

use crate::comandos::{DefinirAmbienteFiscal, InstalarCertificado, RemoverCertificado};
use crate::consultas::{SituacaoFiscal, StatusDaSefaz};
use crate::manifesto::MANIFESTO;

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
            .consulta::<StatusDaSefaz>("fiscal.status_sefaz.v1");
        Ok(())
    }
}
