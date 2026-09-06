//! # mod-agenda
//!
//! Compromissos e recursos — sala, técnico, equipamento — com conflito detectado na hora de
//! marcar. Ver `docs/modulos/agenda.md` para a especificação funcional completa.
//!
//! ## O que este crate contém agora
//!
//! O **domínio puro** (`docs/19-estado-e-processo.md` §3.2):
//!
//! - [`Compromisso`] / [`EstadoCompromisso`] — FSM `Agendado→Confirmado→EmAndamento→Concluido`,
//!   mais `Cancelado`/`NaoCompareceu`, e [`Compromisso::sobrepoe`] — a regra de conflito de
//!   horário usada tanto para gravar quanto para consultar.
//! - [`Recurso`] / [`DisponibilidadeRecurso`] — sala/técnico/equipamento/pessoa e a janela
//!   recorrente em que cada um está disponível; ausência de regra = sempre disponível.
//! - [`manifesto`] — a identidade declarativa do módulo, validada.
//!
//! E a **amarração ao motor** ([`ModuloAgenda`]): manifesto, migrações
//! (`agenda_recurso`/`agenda_disponibilidade`/`agenda_compromisso`/
//! `agenda_compromisso_recurso` — `agenda_compromisso.cliente` referencia `clientes_pessoa`
//! de verdade, por isso o módulo depende de `clientes`) e sete comandos: cadastro
//! (**[`CriarRecurso`]**, **[`DefinirDisponibilidade`]**) e o ciclo do compromisso
//! (**[`CriarCompromisso`]** — recusa sobreposição no mesmo recurso por padrão e exige
//! confirmação explícita para um horário fora da `DisponibilidadeRecurso` cadastrada —,
//! **[`ConfirmarCompromisso`]**, **[`IniciarCompromisso`]**, **[`ConcluirCompromisso`]**,
//! **[`CancelarCompromisso`]** — nunca apaga, só marca `Cancelado`). Mais
//! **[`registrar_nao_comparecimento_pendentes`]**, pronta para um agendador chamar (não é
//! `Comando`: `docs/modulos/agenda.md` §5 já descrevia `RegistrarNaoComparecimento` como
//! "tarefa agendada, sem permissão de usuário", mesmo padrão de
//! `mod_financeiro::materializar_recorrencias_pendentes`). As quatro consultas do §6
//! (`AgendaDoRecurso`, `DisponibilidadeNoPeriodo`, `ConflitosDeAgenda`,
//! `ProximosCompromissos`) — esta última é a consulta `"agenda.proximos_compromissos.v1"`
//! que a tela de agenda (`cardeal-desktop`) já espera.
//!
//! `agenda` **não lança dinheiro** (§7): `os`/`hotelaria` chamam `CriarCompromisso` para
//! reservar um recurso e lançam o próprio receituário à parte, correlacionando pelo par
//! `origem_modulo`/`origem_id` do compromisso — o mesmo padrão de correlação que
//! `financeiro::Titulo::origem_modulo`/`origem_id` já usa com `os`/`vendas`/`compras`.
//!
//! ## O que falta (ver `docs/17-roadmap.md`)
//!
//! `Lembrete`/`agenda_lembrete` (submódulo `lembrete` declarado no manifesto, sem domínio
//! nem comando ainda — nenhum consumidor real precisa disso hoje) e o fluxo de exceção
//! explícita para forçar um `ConflitoDeAgenda` (`docs/modulos/agenda.md` §11.1 menciona uma
//! permissão de exceção; esta fatia recusa todo conflito, sem exceção, o comportamento mais
//! seguro por padrão). `agenda.conflito_detectado.v1` é do fluxo de reconciliação offline
//! (§12) — não publicado ainda, porque não existe `cardeal-sync`. `os`/`hotelaria` ainda não
//! chamam `CriarCompromisso` de verdade — a integração de fato (visita técnica → compromisso
//! → agenda do técnico) é o próximo passo natural quando um desses módulos for revisitado.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]
#![allow(clippy::result_large_err)]

mod comandos;
mod compromisso;
mod consultas;
mod erros;
pub mod eventos;
mod manifesto;
pub mod migracoes;
mod modulo;
mod recurso;
mod repositorio;

pub use comandos::{
    registrar_nao_comparecimento_pendentes, CancelarCompromisso, CompromissoAgendado,
    ConcluirCompromisso, ConfirmarCompromisso, CriarCompromisso, CriarRecurso,
    DefinirDisponibilidade, DisponibilidadeDefinida, IniciarCompromisso, RecursoCriado,
};
pub use compromisso::{Compromisso, EstadoCompromisso, TipoCompromisso};
pub use consultas::{
    AgendaDoRecurso, CompromissosNoPeriodo, ConflitosDeAgenda, DisponibilidadeNoPeriodo,
    ProximosCompromissos, Recursos,
};
pub use erros::ErroAgenda;
pub use manifesto::{manifesto, MANIFESTO};
pub use modulo::ModuloAgenda;
pub use recurso::{DisponibilidadeRecurso, Recurso, TipoRecurso};
pub use repositorio::RepositorioAgenda;
