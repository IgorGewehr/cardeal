//! Erros do módulo de vendas.

use cardeal_kernel::{CodigoErro, Detalhes, ErroDominio, Percentual};

/// Tudo que pode dar errado no preço, no orçamento, no pedido, na devolução e na comissão.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErroVendas {
    /// Nenhuma regra de preço vigente para o produto na tabela e quantidade dadas.
    #[error("Nenhum preço vigente encontrado para este item")]
    PrecoNaoEncontrado,

    /// Uma regra de preço referencia produto e grupo ao mesmo tempo (ou nenhum).
    #[error("A regra de preço deve mirar um produto OU um grupo, não os dois nem nenhum")]
    RegraPrecoAmbigua,

    /// Item com quantidade não positiva.
    #[error("A quantidade do item precisa ser maior que zero")]
    QuantidadeInvalida,

    /// Desconto acima do teto do papel do vendedor (`docs/modulos/vendas.md` §11.4).
    #[error(
        "Desconto de {pedido} acima do limite de {limite} — precisa de autorização de supervisor"
    )]
    DescontoAcimaDoLimite {
        /// O desconto pedido.
        pedido: Percentual,
        /// O teto do papel.
        limite: Percentual,
    },

    /// Transição de estado inválida no orçamento.
    #[error("O orçamento está \"{atual}\", não aceita esta operação (esperado \"{esperado}\")")]
    OrcamentoEmEstadoInvalido {
        /// Estado atual.
        atual: &'static str,
        /// Estado exigido.
        esperado: &'static str,
    },

    /// Orçamento expirado (validade vencida).
    #[error("O orçamento expirou em {0}")]
    OrcamentoExpirado(String),

    /// Transição de estado inválida no pedido.
    #[error("O pedido está \"{atual}\", não aceita esta operação (esperado \"{esperado}\")")]
    PedidoEmEstadoInvalido {
        /// Estado atual.
        atual: &'static str,
        /// Estado exigido.
        esperado: &'static str,
    },

    /// Faturar/editar um pedido sem itens.
    #[error("O pedido não tem itens")]
    PedidoSemItens,

    /// `CancelarPedido` sobre um pedido já faturado — a correção é devolução (§11.3).
    #[error("Pedido já faturado não se cancela — registre uma devolução")]
    PedidoJaFaturado,

    /// Devolução de quantidade maior que a comprada.
    #[error("A quantidade a devolver passa da comprada")]
    QuantidadeDevolvidaExcede,

    /// Devolução sem nenhum item.
    #[error("A devolução precisa de ao menos um item")]
    DevolucaoVazia,

    /// Transição de estado inválida na devolução.
    #[error("A devolução está \"{atual}\", não aceita esta operação (esperado \"{esperado}\")")]
    DevolucaoEmEstadoInvalido {
        /// Estado atual.
        atual: &'static str,
        /// Estado exigido.
        esperado: &'static str,
    },

    /// Apuração/pagamento de comissão inválido para o estado atual.
    #[error("A comissão está \"{atual}\", não aceita esta operação")]
    ComissaoEmEstadoInvalido {
        /// Estado atual.
        atual: &'static str,
    },
}

impl ErroDominio for ErroVendas {
    fn codigo(&self) -> CodigoErro {
        match self {
            Self::PrecoNaoEncontrado => CodigoErro::NAO_ENCONTRADO,
            Self::RegraPrecoAmbigua | Self::PedidoJaFaturado | Self::QuantidadeDevolvidaExcede => {
                CodigoErro::REGRA_VIOLADA
            }
            Self::QuantidadeInvalida | Self::DevolucaoVazia | Self::PedidoSemItens => {
                CodigoErro::ENTRADA_INVALIDA
            }
            Self::DescontoAcimaDoLimite { .. } => CodigoErro::SEM_PERMISSAO,
            Self::OrcamentoExpirado(_)
            | Self::OrcamentoEmEstadoInvalido { .. }
            | Self::PedidoEmEstadoInvalido { .. }
            | Self::DevolucaoEmEstadoInvalido { .. }
            | Self::ComissaoEmEstadoInvalido { .. } => CodigoErro::ESTADO_INVALIDO,
        }
    }

    fn detalhes(&self) -> Option<Detalhes> {
        match self {
            Self::DescontoAcimaDoLimite { limite, .. } => Some(Detalhes::nova(
                "Desconto acima do seu limite",
                format!(
                    "O seu papel autoriza até {}. Um supervisor pode autorizar na hora — não \
                     fica pendente para depois (§11.4).",
                    limite.formatar_com_simbolo()
                ),
            )),
            Self::PedidoJaFaturado => Some(Detalhes::nova(
                "Faturar é irreversível",
                "Assim como o Razão não faz UPDATE, um pedido faturado não se cancela: a \
                 correção é sempre uma devolução, com estorno coerente de estoque e comissão.",
            )),
            _ => None,
        }
    }
}
