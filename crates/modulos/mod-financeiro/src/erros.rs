//! Erros do módulo financeiro.
//!
//! Todos implementam [`ErroDominio`](cardeal_kernel::ErroDominio) — é o que a UI usa para
//! transformar a falha em conversa (`docs/09-protocolo-api.md` §3), não em stack trace.

use cardeal_kernel::{AcaoSugerida, CodigoErro, Detalhes, Dinheiro, ErroDominio};

use crate::titulo::EstadoParcela;

/// Tudo que pode dar errado ao montar um título, calcular ou planejar uma baixa.
///
/// Erros que envolvem o Razão (conta inexistente, período fechado) continuam sendo
/// [`ErroRazao`](cardeal_ledger::ErroRazao) — este enum cobre só as regras próprias do
/// financeiro (`docs/modulos/financeiro.md` §11).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErroFinanceiro {
    /// Um título precisa de valor total positivo.
    #[error("O valor do título precisa ser maior que zero")]
    ValorInvalido,

    /// Número de parcelas fora da faixa aceitável (1 a 360).
    #[error("Um título tem de 1 a 360 parcelas; {0} está fora da faixa")]
    NumeroDeParcelasInvalido(u16),

    /// A política de juros exige uma taxa, e ela não foi informada (ou veio zerada).
    #[error("A política de juros \"{0}\" exige uma taxa maior que zero")]
    TaxaDeJurosAusente(&'static str),

    /// A parcela referenciada não pertence ao título informado.
    #[error("A parcela não pertence a este título")]
    ParcelaDeOutroTitulo,

    /// A parcela já foi quitada, cancelada ou renegociada — não aceita nova baixa.
    #[error("Esta parcela está \"{0:?}\" e não aceita baixa")]
    ParcelaNaoBaixavel(EstadoParcela),

    /// O valor informado na baixa passa do total devido na data (principal + juros + multa
    /// − desconto).
    #[error("O valor da baixa ({recebido}) passa do total devido ({devido})")]
    ValorSuperaSaldo {
        /// O valor que se tentou baixar.
        recebido: Dinheiro,
        /// O total efetivamente devido na data de referência.
        devido: Dinheiro,
    },

    /// Uma baixa precisa de valor positivo.
    #[error("O valor recebido na baixa precisa ser maior que zero")]
    BaixaZerada,

    /// A renegociação foi pedida sobre um título sem saldo em aberto.
    #[error("Não há saldo em aberto para renegociar")]
    SemSaldoParaRenegociar,

    /// A quebra de caixa no fechamento passou da tolerância e nenhum motivo foi informado.
    #[error("A quebra de {quebra} passa da tolerância — informe o motivo")]
    QuebraExigeMotivo {
        /// A diferença apurada (negativa = falta).
        quebra: Dinheiro,
    },

    /// Sangria (ou pagamento em espécie) de valor acima do que há no caixa.
    #[error("O valor de {valor} supera o saldo do caixa ({saldo})")]
    ValorSuperaSaldoDoCaixa {
        /// O valor pedido.
        valor: Dinheiro,
        /// O saldo disponível na sessão.
        saldo: Dinheiro,
    },

    /// A operação exige uma sessão de caixa aberta, e a informada já está fechada.
    #[error("A sessão de caixa não está aberta")]
    CaixaFechado,

    /// Tentativa de fechar (ou operar) uma sessão que não está no estado esperado.
    #[error("A sessão de caixa está \"{atual}\", não \"{esperado}\"")]
    EstadoDeSessaoInvalido {
        /// O estado atual da sessão.
        atual: &'static str,
        /// O estado que a operação exigia.
        esperado: &'static str,
    },
}

impl ErroDominio for ErroFinanceiro {
    fn codigo(&self) -> CodigoErro {
        match self {
            Self::ValorInvalido
            | Self::NumeroDeParcelasInvalido(_)
            | Self::TaxaDeJurosAusente(_)
            | Self::BaixaZerada => CodigoErro::ENTRADA_INVALIDA,
            Self::ParcelaDeOutroTitulo
            | Self::SemSaldoParaRenegociar
            | Self::ValorSuperaSaldo { .. }
            | Self::QuebraExigeMotivo { .. }
            | Self::ValorSuperaSaldoDoCaixa { .. } => CodigoErro::REGRA_VIOLADA,
            Self::ParcelaNaoBaixavel(_) | Self::EstadoDeSessaoInvalido { .. } => {
                CodigoErro::ESTADO_INVALIDO
            }
            Self::CaixaFechado => CodigoErro::CAIXA_FECHADO,
        }
    }

    fn detalhes(&self) -> Option<Detalhes> {
        match self {
            Self::QuebraExigeMotivo { quebra } => Some(Detalhes::nova(
                "A conferência do caixa não bate",
                format!(
                    "A diferença apurada foi de {}. Diferenças acima da tolerância precisam de \
                     uma justificativa antes do fechamento — ela vira um lançamento visível, \
                     nunca some silenciosamente.",
                    quebra.formatar_com_simbolo()
                ),
            )),
            Self::CaixaFechado => Some(
                Detalhes::nova(
                    "O caixa deste terminal está fechado",
                    "Recebimentos em espécie só entram com uma sessão de caixa aberta \
                     vinculada a este dispositivo.",
                )
                .com(AcaoSugerida::primaria(
                    "Abrir o caixa",
                    "financeiro.caixa.abrir",
                )),
            ),
            _ => None,
        }
    }
}
