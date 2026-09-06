//! Erros do módulo de agenda.

use cardeal_kernel::{CodigoErro, Detalhes, ErroDominio, Id};

/// Tudo que pode dar errado ao marcar ou conduzir um compromisso.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ErroAgenda {
    /// Título do compromisso vazio.
    #[error("O compromisso precisa de um título")]
    TituloVazio,

    /// `fim` não é depois de `inicio`.
    #[error("O fim do compromisso precisa ser depois do início")]
    PeriodoInvalido,

    /// Nome de recurso vazio.
    #[error("O recurso precisa de um nome")]
    NomeVazio,

    /// `CriarRecurso` com nome já usado por um recurso ativo da empresa.
    #[error("Já existe um recurso ativo com este nome")]
    NomeDuplicado,

    /// `DefinirDisponibilidade` com janela de horário inconsistente.
    #[error("O intervalo de disponibilidade é inválido")]
    IntervaloInvalido,

    /// `CriarCompromisso` encontrou outro compromisso ativo no mesmo recurso, no mesmo
    /// intervalo (`docs/modulos/agenda.md` §11.1).
    #[error("O recurso já está reservado neste horário")]
    ConflitoDeAgenda {
        /// O recurso em conflito.
        recurso: Id,
        /// O compromisso já existente que colide.
        compromisso_existente: Id,
    },

    /// `CriarCompromisso` fora da `DisponibilidadeRecurso` declarada, sem a confirmação
    /// explícita (`docs/modulos/agenda.md` §11.2).
    #[error("O recurso não está disponível neste horário — confirme explicitamente para forçar")]
    RecursoIndisponivel {
        /// O recurso fora da disponibilidade.
        recurso: Id,
    },

    /// Ação pedida sobre um compromisso já cancelado.
    #[error("Este compromisso está cancelado")]
    CompromissoCancelado,

    /// `CancelarCompromisso` pedido sobre um compromisso já concluído.
    #[error("Este compromisso já foi concluído")]
    CompromissoConcluido,

    /// Transição de estado inválida.
    #[error("O compromisso está \"{atual}\", não \"{esperado}\"")]
    EstadoInvalido {
        /// O estado atual.
        atual: &'static str,
        /// O estado que a operação exigia.
        esperado: &'static str,
    },
}

impl ErroDominio for ErroAgenda {
    fn codigo(&self) -> CodigoErro {
        match self {
            Self::TituloVazio
            | Self::PeriodoInvalido
            | Self::NomeVazio
            | Self::IntervaloInvalido => CodigoErro::ENTRADA_INVALIDA,
            Self::NomeDuplicado
            | Self::ConflitoDeAgenda { .. }
            | Self::RecursoIndisponivel { .. } => CodigoErro::REGRA_VIOLADA,
            Self::CompromissoCancelado
            | Self::CompromissoConcluido
            | Self::EstadoInvalido { .. } => CodigoErro::ESTADO_INVALIDO,
        }
    }

    fn detalhes(&self) -> Option<Detalhes> {
        match self {
            Self::ConflitoDeAgenda { .. } => Some(Detalhes::nova(
                "Horário já reservado",
                "Escolha outro horário ou outro recurso — dois compromissos não podem \
                 reservar o mesmo recurso ao mesmo tempo.",
            )),
            Self::RecursoIndisponivel { .. } => Some(Detalhes::nova(
                "Fora do horário habitual",
                "Este horário está fora da disponibilidade cadastrada para o recurso. Marque \
                 mesmo assim só se tiver certeza.",
            )),
            _ => None,
        }
    }
}
