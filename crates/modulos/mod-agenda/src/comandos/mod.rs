//! Os comandos da agenda (`docs/modulos/agenda.md` §5). Um arquivo, um comando.
//!
//! `RegistrarNaoComparecimento` é "tarefa agendada, sem permissão de usuário" no próprio
//! doc — por isso não é `Comando`, é uma função `pub` comum
//! ([`registrar_nao_comparecimento_pendentes`]), mesmo padrão de
//! `mod_financeiro::materializar_recorrencias_pendentes`.

mod cancelar_compromisso;
mod concluir_compromisso;
mod confirmar_compromisso;
mod criar_compromisso;
mod criar_recurso;
mod definir_disponibilidade;
mod iniciar_compromisso;
mod registrar_nao_comparecimento;

pub use cancelar_compromisso::CancelarCompromisso;
pub use concluir_compromisso::ConcluirCompromisso;
pub use confirmar_compromisso::ConfirmarCompromisso;
pub use criar_compromisso::{CompromissoAgendado, CriarCompromisso};
pub use criar_recurso::{CriarRecurso, RecursoCriado};
pub use definir_disponibilidade::{DefinirDisponibilidade, DisponibilidadeDefinida};
pub use iniciar_compromisso::IniciarCompromisso;
pub use registrar_nao_comparecimento::registrar_nao_comparecimento_pendentes;

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_storage::UnidadeDeTrabalho;

use crate::compromisso::Compromisso;
use crate::repositorio::RepositorioAgenda;

/// Carrega um compromisso ou devolve `NAO_ENCONTRADO`.
pub(crate) fn carregar_compromisso(uow: &mut UnidadeDeTrabalho, id: Id) -> Resultado<Compromisso> {
    RepositorioAgenda::novo(uow)
        .buscar_compromisso(id)?
        .ok_or_else(|| Erro::nao_encontrado("compromisso"))
}
