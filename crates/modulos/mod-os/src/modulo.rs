//! O `impl Modulo` de OS — o ponto por onde o motor coleta manifesto, migrações e comandos
//! (`docs/contratos-internos.md` §4). Sem lógica: só amarração.

use cardeal_kernel::Resultado;
use cardeal_modkit::{Manifesto, Modulo, Registro};
use cardeal_storage::ConjuntoMigracoes;

use crate::comandos::{
    AbrirOrdemServico, AplicarPeca, AprovarOrcamentoOs, CancelarOrdemServico, ConcluirExecucao,
    EnviarParaAprovacao, FaturarOrdemServico, IniciarExecucao, MontarOrcamentoOs, RegistrarLaudo,
    RegistrarMaoDeObra, RemoverItemOrcamento, ReprovarOrcamentoOs,
};
use crate::consultas::{BuscarDetalheOrdem, OrdensEmAberto};
use crate::manifesto::MANIFESTO;
use crate::migracoes;

/// O módulo de ordens de serviço, para registrar no
/// [`Despachante`](cardeal_modkit::Despachante).
pub struct ModuloOs;

impl Modulo for ModuloOs {
    fn manifesto(&self) -> &'static Manifesto {
        &MANIFESTO
    }

    fn migracoes(&self) -> ConjuntoMigracoes {
        migracoes::conjunto()
    }

    fn registrar(&self, registro: &mut Registro) -> Resultado<()> {
        registro
            .comando::<AbrirOrdemServico>("os.abrir_ordem_servico.v1")
            .comando::<RegistrarLaudo>("os.registrar_laudo.v1")
            .comando::<MontarOrcamentoOs>("os.montar_orcamento.v1")
            .comando::<RemoverItemOrcamento>("os.remover_item_orcamento.v1")
            .comando::<EnviarParaAprovacao>("os.enviar_para_aprovacao.v1")
            .comando::<AprovarOrcamentoOs>("os.aprovar_orcamento.v1")
            .comando::<ReprovarOrcamentoOs>("os.reprovar_orcamento.v1")
            .comando::<CancelarOrdemServico>("os.cancelar_ordem_servico.v1")
            .comando::<IniciarExecucao>("os.iniciar_execucao.v1")
            .comando::<AplicarPeca>("os.aplicar_peca.v1")
            .comando::<RegistrarMaoDeObra>("os.registrar_mao_de_obra.v1")
            .comando::<ConcluirExecucao>("os.concluir_execucao.v1")
            .comando::<FaturarOrdemServico>("os.faturar_ordem_servico.v1")
            .consulta::<OrdensEmAberto>("os.ordens_em_aberto.v1")
            .consulta::<BuscarDetalheOrdem>("os.buscar_detalhe_ordem.v1");
        Ok(())
    }
}
