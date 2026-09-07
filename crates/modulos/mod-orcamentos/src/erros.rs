//! Erros do módulo de orçamentos.

use cardeal_kernel::{CodigoErro, ErroDominio};

/// Tudo que pode dar errado num orçamento comercial.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErroOrcamentos {
    /// Assunto do orçamento vazio.
    #[error("O assunto do orçamento não pode ser vazio")]
    AssuntoVazio,

    /// Nome do cliente vazio (cadastrado ou avulso, o nome é obrigatório no documento).
    #[error("O nome do cliente não pode ser vazio")]
    ClienteSemNome,

    /// Validade anterior à data de emissão.
    #[error("A validade não pode ser anterior à emissão")]
    ValidadeAnteriorAEmissao,

    /// Descrição de um item vazia.
    #[error("A descrição de todo item do orçamento é obrigatória")]
    DescricaoDeItemVazia,

    /// Quantidade de item não positiva.
    #[error("A quantidade de um item precisa ser maior que zero")]
    QuantidadeInvalida,

    /// Transição de estado não permitida a partir do estado atual.
    #[error("O orçamento está \"{atual}\", não \"{esperado}\"")]
    EstadoInvalido {
        /// O estado atual.
        atual: &'static str,
        /// O estado que a operação exigia.
        esperado: &'static str,
    },

    /// `EnviarOrcamento` sem nenhum item.
    #[error("O orçamento está vazio — adicione ao menos um item antes de enviar")]
    OrcamentoVazio,

    /// Enviar/aprovar um orçamento cuja validade já passou.
    #[error("O orçamento venceu em {0} — reabra a validade antes de seguir")]
    OrcamentoVencido(String),

    /// Aprovar sem identificar quem aprovou.
    #[error("A aprovação exige identificar quem aprovou (nome e documento, ou assinatura)")]
    DecisaoSemIdentificacao,

    /// Registrar decisão sobre um orçamento que não está aguardando decisão.
    #[error("Este orçamento já foi decidido")]
    OrcamentoJaDecidido,

    /// Converter em OS um orçamento de cliente avulso (sem cadastro).
    #[error("Para virar OS o orçamento precisa de um cliente cadastrado, não avulso")]
    ClienteAvulsoNaoConverte,

    /// Converter em OS com o módulo de OS desligado.
    #[error("O módulo de Ordens de Serviço está desligado nesta empresa")]
    ModuloOsInativo,
}

impl ErroDominio for ErroOrcamentos {
    fn codigo(&self) -> CodigoErro {
        match self {
            Self::AssuntoVazio
            | Self::ClienteSemNome
            | Self::DescricaoDeItemVazia
            | Self::QuantidadeInvalida
            | Self::OrcamentoVazio => CodigoErro::ENTRADA_INVALIDA,
            Self::ValidadeAnteriorAEmissao => CodigoErro::DATA_INVALIDA,
            Self::DecisaoSemIdentificacao => CodigoErro::CAMPO_OBRIGATORIO,
            Self::EstadoInvalido { .. } => CodigoErro::ESTADO_INVALIDO,
            Self::OrcamentoVencido(_)
            | Self::OrcamentoJaDecidido
            | Self::ClienteAvulsoNaoConverte
            | Self::ModuloOsInativo => CodigoErro::REGRA_VIOLADA,
        }
    }
}
