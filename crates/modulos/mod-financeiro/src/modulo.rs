//! O `impl Modulo` do financeiro — o ponto por onde o motor coleta manifesto, migrações e
//! manipuladores (`docs/contratos-internos.md` §4). Sem lógica: só amarração.

use cardeal_kernel::Resultado;
use cardeal_modkit::{Manifesto, Modulo, Registro};
use cardeal_storage::ConjuntoMigracoes;

use crate::comandos::{BaixarRecebimento, LancarTituloAReceber};
use crate::manifesto::MANIFESTO;
use crate::migracoes;

/// O módulo financeiro, para registrar no [`Despachante`](cardeal_modkit::Despachante).
pub struct ModuloFinanceiro;

impl Modulo for ModuloFinanceiro {
    fn manifesto(&self) -> &'static Manifesto {
        &MANIFESTO
    }

    fn migracoes(&self) -> ConjuntoMigracoes {
        migracoes::conjunto()
    }

    fn registrar(&self, registro: &mut Registro) -> Resultado<()> {
        registro
            .comando::<LancarTituloAReceber>("financeiro.lancar_titulo_a_receber.v1")
            .comando::<BaixarRecebimento>("financeiro.baixar_recebimento.v1");
        Ok(())
    }
}
