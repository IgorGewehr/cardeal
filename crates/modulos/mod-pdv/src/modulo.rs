//! O `impl Modulo` do PDV — o ponto por onde o motor coleta manifesto, migrações e comandos
//! (`docs/contratos-internos.md` §4). Sem lógica: só amarração.

use cardeal_kernel::Resultado;
use cardeal_modkit::{Manifesto, Modulo, Registro};
use cardeal_storage::ConjuntoMigracoes;

use crate::comandos::{
    AbrirCupom, AdicionarItem, AplicarDescontoItem, CancelarCupom, CancelarItem, FinalizarVenda,
};
use crate::manifesto::MANIFESTO;
use crate::migracoes;

/// O módulo de PDV, para registrar no [`Despachante`](cardeal_modkit::Despachante).
pub struct ModuloPdv;

impl Modulo for ModuloPdv {
    fn manifesto(&self) -> &'static Manifesto {
        &MANIFESTO
    }

    fn migracoes(&self) -> ConjuntoMigracoes {
        migracoes::conjunto()
    }

    fn registrar(&self, registro: &mut Registro) -> Resultado<()> {
        registro
            .comando::<AbrirCupom>("pdv.abrir_cupom.v1")
            .comando::<AdicionarItem>("pdv.adicionar_item.v1")
            .comando::<AplicarDescontoItem>("pdv.aplicar_desconto_item.v1")
            .comando::<CancelarItem>("pdv.cancelar_item.v1")
            .comando::<CancelarCupom>("pdv.cancelar_cupom.v1")
            .comando::<FinalizarVenda>("pdv.finalizar_venda.v1");
        Ok(())
    }
}
