//! Erros do módulo de compras.

use cardeal_kernel::{CodigoErro, Detalhes, Dinheiro, ErroDominio};

/// Tudo que pode dar errado na entrada de nota, no casamento de produto e na confirmação.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErroCompras {
    /// Já existe uma nota com esta chave de acesso na empresa — `ImportarNotaEntrada`/a
    /// varredura da SEFAZ é idempotente por `(empresa, chave_acesso)`.
    #[error("Esta nota já foi importada")]
    ChaveJaImportada,

    /// `ConfirmarEntrada` com algum item ainda `NaoCasado`/`SugestaoForte`.
    #[error("Ainda há {0} item(ns) sem casamento confirmado")]
    ItemNaoCasado(usize),

    /// `ConfirmarEntrada`/`VincularProdutoManual` sobre uma nota já confirmada.
    #[error("Esta nota já foi confirmada")]
    NotaJaConfirmada,

    /// Transição de estado não permitida a partir do estado atual da nota.
    #[error("A nota está \"{atual}\", não \"{esperado}\"")]
    EstadoDeNotaInvalido {
        /// O estado atual.
        atual: &'static str,
        /// O estado que a operação exigia.
        esperado: &'static str,
    },

    /// O item de nota informado não pertence à nota informada.
    #[error("Este item não pertence a esta nota de entrada")]
    ItemNaoPertenceANota,

    /// Rateio de despesas com referência inválida (ex.: nota sem itens).
    #[error("Não é possível ratear despesas: a nota não tem itens")]
    ReferenciaInvalida,
    /// O XML não é de uma compra da empresa (saída própria, outro destinatário, devolução de
    /// cliente, nota de entrada do emitente) — nunca vira entrada de estoque com conta a pagar.
    #[error("Esta nota não é uma compra: {0}")]
    NaoECompra(&'static str),

    /// As parcelas não somam o total da nota.
    #[error("As parcelas somam {soma}, mas a nota é de {total}")]
    ParcelasNaoFecham {
        /// A soma das parcelas.
        soma: Dinheiro,
        /// O total da nota.
        total: Dinheiro,
    },

    /// Uma parcela que não vale (sem valor, vencendo antes da emissão…).
    #[error("Parcelas: {0}")]
    ParcelaInvalida(&'static str),

    /// O motivo do estorno vai ao histórico: ao menos 10 caracteres.
    #[error("Explique o motivo do estorno (ao menos 10 caracteres)")]
    MotivoDoEstornoCurto,

    /// A conta a pagar da nota já tem pagamento, e quem estorna não pediu para estorná-lo.
    #[error(
        "A conta desta nota já tem {0} pago — para estornar a entrada, estorne também o pagamento"
    )]
    EntradaComPagamento(Dinheiro),
}

impl ErroDominio for ErroCompras {
    fn codigo(&self) -> CodigoErro {
        match self {
            Self::ChaveJaImportada => CodigoErro::DUPLICADO,
            Self::ItemNaoCasado(_) | Self::NotaJaConfirmada | Self::ReferenciaInvalida => {
                CodigoErro::REGRA_VIOLADA
            }
            Self::EstadoDeNotaInvalido { .. } => CodigoErro::ESTADO_INVALIDO,
            Self::ItemNaoPertenceANota
            | Self::NaoECompra(_)
            | Self::ParcelasNaoFecham { .. }
            | Self::ParcelaInvalida(_)
            | Self::MotivoDoEstornoCurto => CodigoErro::ENTRADA_INVALIDA,
            Self::EntradaComPagamento(_) => CodigoErro::REGRA_VIOLADA,
        }
    }

    fn detalhes(&self) -> Option<Detalhes> {
        match self {
            Self::ItemNaoCasado(n) => Some(Detalhes::nova(
                "Itens sem casamento confirmado",
                format!(
                    "{n} item(ns) desta nota ainda não apontam para um produto do estoque — \
                     vincule cada um (`VincularProdutoManual`) antes de confirmar a entrada. \
                     Isso impede lançar \"REF-8821\" como se fosse um SKU do estoque."
                ),
            )),
            _ => None,
        }
    }
}
