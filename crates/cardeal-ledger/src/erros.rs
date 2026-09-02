//! Erros do Razão.

use cardeal_kernel::{AcaoSugerida, CodigoErro, Data, Detalhes, Dinheiro, ErroDominio, Id};

use crate::conta::PapelConta;
use crate::lancamento::EstadoLancamento;

/// Tudo que pode dar errado ao montar ou registrar um lançamento.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErroRazao {
    /// As partidas não somam zero.
    #[error("As partidas não se equilibram: falta(m) {diferenca} entre {partidas} partida(s)")]
    Desbalanceado {
        /// O que falta para zerar. Positivo = falta crédito; negativo = falta débito.
        diferenca: Dinheiro,
        /// Quantas partidas foram informadas.
        partidas: usize,
    },

    /// Um lançamento precisa de ao menos duas partidas.
    #[error("Um lançamento precisa de ao menos duas partidas")]
    PartidasInsuficientes,

    /// Uma partida foi informada com valor zero.
    #[error("Uma partida não pode ter valor zero")]
    ValorZerado,

    /// `construir()` foi chamado sem `criado_por`.
    #[error("Todo lançamento precisa de autor e dispositivo")]
    AutoriaAusente,

    /// A conta referenciada não existe no plano da empresa.
    #[error("Conta não encontrada")]
    ContaNaoEncontrada(Id),

    /// A conta é sintética — só agrupa, não recebe partida.
    #[error("A conta \"{codigo}\" é sintética e não pode receber lançamento")]
    ContaSintetica {
        /// A conta.
        conta: Id,
        /// O código, para a mensagem.
        codigo: String,
    },

    /// A conta está inativa.
    #[error("A conta está inativa")]
    ContaInativa(Id),

    /// Nenhuma conta do plano tem este papel semântico.
    #[error("Nenhuma conta está mapeada para o papel {0:?}")]
    PapelNaoMapeado(PapelConta),

    /// O período contábil da competência já foi fechado.
    #[error("O período contábil até {ate} está fechado")]
    PeriodoFechado {
        /// A data até a qual o período está fechado.
        ate: Data,
    },

    /// Tentativa de estornar um lançamento já estornado.
    #[error("Este lançamento já foi estornado")]
    JaEstornado(Id),

    /// O lançamento referenciado não existe.
    #[error("Lançamento não encontrado")]
    LancamentoNaoEncontrado(Id),

    /// As partidas referenciam contas de empresas diferentes.
    #[error("As partidas de um lançamento devem pertencer todas à mesma empresa")]
    EmpresaDivergente,

    /// O motivo do estorno é curto demais para servir de auditoria.
    #[error("O motivo do estorno precisa de ao menos 10 caracteres")]
    MotivoDeEstornoCurto,

    /// Nenhuma conta com este código no plano da empresa.
    #[error("Nenhuma conta encontrada com o código \"{0}\"")]
    CodigoNaoEncontrado(String),

    /// A transição de estado pedida não é permitida a partir do estado atual.
    #[error("Não é possível passar de {de:?} para {para:?}")]
    TransicaoInvalida {
        /// O estado atual do lançamento.
        de: EstadoLancamento,
        /// O estado que se tentou aplicar.
        para: EstadoLancamento,
    },
}

impl ErroDominio for ErroRazao {
    fn codigo(&self) -> CodigoErro {
        match self {
            Self::Desbalanceado { .. } => CodigoErro::RAZAO_DESBALANCEADO,
            Self::PartidasInsuficientes | Self::ValorZerado | Self::MotivoDeEstornoCurto => {
                CodigoErro::ENTRADA_INVALIDA
            }
            Self::AutoriaAusente => CodigoErro::CAMPO_OBRIGATORIO,
            Self::ContaNaoEncontrada(_) | Self::LancamentoNaoEncontrado(_) => {
                CodigoErro::NAO_ENCONTRADO
            }
            Self::ContaSintetica { .. }
            | Self::ContaInativa(_)
            | Self::PapelNaoMapeado(_)
            | Self::EmpresaDivergente => CodigoErro::REGRA_VIOLADA,
            Self::PeriodoFechado { .. } => CodigoErro::PERIODO_FECHADO,
            Self::JaEstornado(_) | Self::TransicaoInvalida { .. } => CodigoErro::ESTADO_INVALIDO,
            Self::CodigoNaoEncontrado(_) => CodigoErro::NAO_ENCONTRADO,
        }
    }

    fn detalhes(&self) -> Option<Detalhes> {
        match self {
            Self::PeriodoFechado { ate } => Some(
                Detalhes::nova(
                    "Não é possível lançar neste período",
                    format!(
                        "O período contábil até {} já foi fechado. Lançamentos retroativos \
                         exigem reabertura do período por um administrador.",
                        ate.formatar()
                    ),
                )
                .com(AcaoSugerida::primaria(
                    "Solicitar reabertura",
                    "razao.reabrir_periodo",
                )),
            ),
            Self::ContaSintetica { codigo, .. } => Some(Detalhes::nova(
                "Esta conta não aceita lançamento",
                format!(
                    "\"{codigo}\" é uma conta sintética — ela agrupa outras contas, mas não \
                     recebe partida diretamente. Escolha uma conta analítica da árvore."
                ),
            )),
            _ => None,
        }
    }
}
