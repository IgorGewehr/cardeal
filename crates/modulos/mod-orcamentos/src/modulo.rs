//! O `impl Modulo` de orçamentos — o ponto por onde o motor coleta manifesto, migrações e
//! comandos (`docs/contratos-internos.md` §4). Sem lógica: só amarração.

use cardeal_kernel::Resultado;
use cardeal_modkit::{Manifesto, Modulo, Registro};
use cardeal_storage::ConjuntoMigracoes;

use crate::comandos::{
    CancelarOrcamento, ConverterOrcamentoEmOs, CriarOrcamento, DefinirItensOrcamento,
    DuplicarOrcamento, EditarOrcamento, EnviarOrcamento, RegistrarDecisaoOrcamento,
};
use crate::consultas::{BuscarOrcamento, OrcamentosRecentes, ResumoOrcamentos};
use crate::manifesto::MANIFESTO;
use crate::migracoes;

/// O módulo de orçamentos, para registrar no
/// [`Despachante`](cardeal_modkit::Despachante).
pub struct ModuloOrcamentos;

impl Modulo for ModuloOrcamentos {
    fn manifesto(&self) -> &'static Manifesto {
        &MANIFESTO
    }

    fn migracoes(&self) -> ConjuntoMigracoes {
        migracoes::conjunto()
    }

    fn registrar(&self, registro: &mut Registro) -> Resultado<()> {
        registro
            .comando::<CriarOrcamento>("orcamentos.criar_orcamento.v1")
            .comando::<EditarOrcamento>("orcamentos.editar_orcamento.v1")
            .comando::<DefinirItensOrcamento>("orcamentos.definir_itens.v1")
            .comando::<EnviarOrcamento>("orcamentos.enviar_orcamento.v1")
            .comando::<RegistrarDecisaoOrcamento>("orcamentos.registrar_decisao.v1")
            .comando::<CancelarOrcamento>("orcamentos.cancelar_orcamento.v1")
            .comando::<DuplicarOrcamento>("orcamentos.duplicar_orcamento.v1")
            .comando::<ConverterOrcamentoEmOs>("orcamentos.converter_em_os.v1")
            .consulta::<OrcamentosRecentes>("orcamentos.orcamentos_recentes.v1")
            .consulta::<BuscarOrcamento>("orcamentos.buscar_orcamento.v1")
            .consulta::<ResumoOrcamentos>("orcamentos.resumo.v1");
        Ok(())
    }
}
