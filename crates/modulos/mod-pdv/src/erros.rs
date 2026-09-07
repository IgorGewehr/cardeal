//! Erros do módulo PDV.

use cardeal_kernel::{CodigoErro, Detalhes, Dinheiro, ErroDominio, Percentual};

/// Tudo que pode dar errado no cupom, no item e no pagamento.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErroPdv {
    /// Item com quantidade não positiva.
    #[error("A quantidade do item precisa ser maior que zero")]
    QuantidadeInvalida,

    /// Desconto acima do teto do papel do operador (`docs/modulos/pdv.md` §5, `F5`).
    #[error(
        "Desconto de {pedido} acima do limite de {limite} — precisa de autorização de supervisor"
    )]
    DescontoAcimaDoLimite {
        /// O desconto pedido.
        pedido: Percentual,
        /// O teto do papel.
        limite: Percentual,
    },

    /// `CancelarItem` sobre um item já cancelado.
    #[error("Este item já está cancelado")]
    ItemJaCancelado,

    /// Transição de estado inválida no cupom.
    #[error("O cupom está \"{atual}\", não aceita esta operação (esperado \"{esperado}\")")]
    CupomEmEstadoInvalido {
        /// Estado atual.
        atual: &'static str,
        /// Estado exigido.
        esperado: &'static str,
    },

    /// `FinalizarVenda`/`CancelarCupom` sem nenhum item ativo.
    #[error("O cupom não tem itens")]
    CupomSemItens,

    /// `CancelarCupom` sobre um cupom já finalizado — irreversível nesta fatia (sem estorno
    /// pós-fechamento ainda, ver `docs/modulos/pdv.md` §4).
    #[error("Cupom já finalizado não se cancela nesta versão")]
    CupomJaFinalizado,

    /// `FinalizarVenda` sem nenhuma forma de pagamento informada.
    #[error("Informe ao menos uma forma de pagamento")]
    PagamentoSemFormas,

    /// A soma dos pagamentos não fecha exatamente com o total do cupom (§11 regra 5).
    #[error("A soma dos pagamentos ({recebido}) não fecha com o total ({total})")]
    SomaDePagamentosDivergente {
        /// O que foi somado dos pagamentos informados.
        recebido: Dinheiro,
        /// O total do cupom.
        total: Dinheiro,
    },

    /// `CancelarCupom` exige motivo e identificação de quem autorizou (§5, `F8`).
    #[error("Cancelar o cupom inteiro exige motivo")]
    MotivoObrigatorio,
}

impl ErroDominio for ErroPdv {
    fn codigo(&self) -> CodigoErro {
        match self {
            Self::QuantidadeInvalida
            | Self::PagamentoSemFormas
            | Self::CupomSemItens
            | Self::MotivoObrigatorio => CodigoErro::ENTRADA_INVALIDA,
            Self::DescontoAcimaDoLimite { .. } => CodigoErro::SEM_PERMISSAO,
            Self::ItemJaCancelado
            | Self::CupomEmEstadoInvalido { .. }
            | Self::CupomJaFinalizado => CodigoErro::ESTADO_INVALIDO,
            Self::SomaDePagamentosDivergente { .. } => CodigoErro::REGRA_VIOLADA,
        }
    }

    fn detalhes(&self) -> Option<Detalhes> {
        match self {
            Self::DescontoAcimaDoLimite { limite, .. } => Some(Detalhes::nova(
                "Desconto acima do seu limite",
                format!(
                    "O seu papel autoriza até {}. Um supervisor pode autorizar na hora.",
                    limite.formatar_com_simbolo()
                ),
            )),
            Self::CupomJaFinalizado => Some(Detalhes::nova(
                "Venda já finalizada",
                "Assim como no Razão, uma venda finalizada não se desfaz por aqui — a correção \
                 é uma devolução (ver mod-vendas), não um cancelamento de cupom.",
            )),
            _ => None,
        }
    }
}
