//! O `impl Modulo` da agenda — o ponto por onde o motor coleta manifesto, migrações e
//! comandos (`docs/contratos-internos.md` §4). Sem lógica: só amarração.

use cardeal_kernel::Resultado;
use cardeal_modkit::{Manifesto, Modulo, Registro};
use cardeal_storage::ConjuntoMigracoes;

use crate::comandos::{
    CancelarCompromisso, ConcluirCompromisso, ConfirmarCompromisso, CriarCompromisso, CriarRecurso,
    DefinirDisponibilidade, IniciarCompromisso,
};
use crate::consultas::{
    AgendaDoRecurso, CompromissosNoPeriodo, ConflitosDeAgenda, DisponibilidadeNoPeriodo,
    ProximosCompromissos, Recursos,
};
use crate::manifesto::MANIFESTO;
use crate::migracoes;

/// O módulo de agenda, para registrar no [`Despachante`](cardeal_modkit::Despachante).
pub struct ModuloAgenda;

impl Modulo for ModuloAgenda {
    fn manifesto(&self) -> &'static Manifesto {
        &MANIFESTO
    }

    fn migracoes(&self) -> ConjuntoMigracoes {
        migracoes::conjunto()
    }

    fn registrar(&self, registro: &mut Registro) -> Resultado<()> {
        registro
            .comando::<CriarRecurso>("agenda.criar_recurso.v1")
            .comando::<DefinirDisponibilidade>("agenda.definir_disponibilidade.v1")
            .comando::<CriarCompromisso>("agenda.criar_compromisso.v1")
            .comando::<ConfirmarCompromisso>("agenda.confirmar_compromisso.v1")
            .comando::<IniciarCompromisso>("agenda.iniciar_compromisso.v1")
            .comando::<ConcluirCompromisso>("agenda.concluir_compromisso.v1")
            .comando::<CancelarCompromisso>("agenda.cancelar_compromisso.v1")
            .consulta::<AgendaDoRecurso>("agenda.agenda_do_recurso.v1")
            .consulta::<DisponibilidadeNoPeriodo>("agenda.disponibilidade_no_periodo.v1")
            .consulta::<ConflitosDeAgenda>("agenda.conflitos_de_agenda.v1")
            .consulta::<ProximosCompromissos>("agenda.proximos_compromissos.v1")
            .consulta::<CompromissosNoPeriodo>("agenda.compromissos_no_periodo.v1")
            .consulta::<Recursos>("agenda.recursos.v1");
        Ok(())
    }
}
