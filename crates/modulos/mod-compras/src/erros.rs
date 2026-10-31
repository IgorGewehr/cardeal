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

    /// O motivo da devolução vai à NF-e: ao menos 10 caracteres.
    #[error("Explique o motivo da devolução (ao menos 10 caracteres)")]
    MotivoDaDevolucaoCurto,

    /// Nada a devolver.
    #[error("Escolha o que volta ao fornecedor (ao menos um item com quantidade)")]
    DevolucaoVazia,

    /// Mais do que entrou (descontado o que já voltou).
    #[error("{item}: só restam {resta} a devolver desta nota")]
    DevolveMaisQueEntrou {
        /// A descrição do item.
        item: String,
        /// O que ainda pode voltar.
        resta: String,
    },

    /// A entrada com devolução não se estorna: a devolução já mexeu no estoque e na conta.
    #[error(
        "Esta nota tem devolução ao fornecedor — a entrada não se estorna mais (corrija com \
         outra devolução)"
    )]
    NotaComDevolucao,

    /// Pedido (ou edição) sem nenhum item com quantidade.
    #[error("O pedido precisa de ao menos um item com quantidade")]
    PedidoVazio,

    /// A ação não cabe no estado do pedido.
    #[error("Não dá para {acao} um pedido \"{atual}\"")]
    EstadoDoPedidoInvalido {
        /// O estado atual, para a tela.
        atual: &'static str,
        /// O que se tentou.
        acao: &'static str,
    },

    /// O pedido já recebeu mercadoria: não se cancela, encerra-se o saldo.
    #[error("Este pedido já recebeu mercadoria — encerre o saldo em vez de cancelar")]
    PedidoComRecebimento,

    /// O motivo de cancelar ou encerrar vai ao histórico.
    #[error("Explique o motivo (ao menos 10 caracteres)")]
    MotivoDoPedidoCurto,

    /// A nota é de um fornecedor, o pedido de outro.
    #[error("O pedido é de outro fornecedor")]
    PedidoDeOutroFornecedor,

    /// O mesmo produto duas vezes no pedido ou na cotação.
    #[error("{0} aparece duas vezes — junte as quantidades numa linha só")]
    ProdutoRepetido(String),

    /// Cotação sem item.
    #[error("A cotação precisa de ao menos um item com quantidade")]
    CotacaoSemItens,

    /// Cotação sem fornecedor.
    #[error("Convide ao menos um fornecedor para a cotação")]
    CotacaoSemFornecedores,

    /// A cotação já foi decidida ou cancelada.
    #[error("Esta cotação já foi decidida ou cancelada")]
    CotacaoFechada,

    /// Decidir sem escolher nada.
    #[error("Escolha de quem comprar ao menos um item")]
    DecisaoVazia,

    /// O fornecedor escolhido não cotou o item.
    #[error("{0}: o fornecedor escolhido não cotou este item")]
    EscolhaSemPreco(String),
}

impl ErroDominio for ErroCompras {
    fn codigo(&self) -> CodigoErro {
        match self {
            Self::ChaveJaImportada => CodigoErro::DUPLICADO,
            Self::ItemNaoCasado(_)
            | Self::NotaJaConfirmada
            | Self::ReferenciaInvalida
            | Self::EntradaComPagamento(_)
            | Self::NotaComDevolucao
            | Self::PedidoComRecebimento
            | Self::PedidoDeOutroFornecedor => CodigoErro::REGRA_VIOLADA,
            Self::EstadoDeNotaInvalido { .. }
            | Self::EstadoDoPedidoInvalido { .. }
            | Self::CotacaoFechada => CodigoErro::ESTADO_INVALIDO,
            Self::ItemNaoPertenceANota
            | Self::NaoECompra(_)
            | Self::ParcelasNaoFecham { .. }
            | Self::ParcelaInvalida(_)
            | Self::MotivoDoEstornoCurto
            | Self::MotivoDaDevolucaoCurto
            | Self::DevolucaoVazia
            | Self::DevolveMaisQueEntrou { .. }
            | Self::PedidoVazio
            | Self::MotivoDoPedidoCurto
            | Self::ProdutoRepetido(_)
            | Self::CotacaoSemItens
            | Self::CotacaoSemFornecedores
            | Self::DecisaoVazia
            | Self::EscolhaSemPreco(_) => CodigoErro::ENTRADA_INVALIDA,
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
