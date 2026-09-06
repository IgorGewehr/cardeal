//! O `impl Modulo` de compras — o ponto por onde o motor coleta manifesto, migrações e
//! comandos (`docs/contratos-internos.md` §4). Sem lógica: só amarração.

use cardeal_kernel::Resultado;
use cardeal_modkit::{Manifesto, Modulo, Registro};
use cardeal_storage::ConjuntoMigracoes;

use crate::comandos::{
    ConfirmarEntrada, DefinirPreferenciasCompras, LancarNotaManual, VincularProdutoManual,
};
use crate::consultas::NotasRecentes;
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
            .comando::<ConfirmarEntrada>("compras.confirmar_entrada.v1")
            .comando::<LancarNotaManual>("compras.lancar_nota_manual.v1")
            .consulta::<NotasRecentes>("compras.notas_recentes.v1");
        Ok(())
    }
}
