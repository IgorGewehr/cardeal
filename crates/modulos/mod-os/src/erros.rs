//! Erros do módulo de ordens de serviço.

use cardeal_kernel::{CodigoErro, Detalhes, ErroDominio};

/// Tudo que pode dar errado na ordem de serviço, no orçamento e na execução.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErroOs {
    /// O equipamento (descrição livre) veio vazio.
    #[error("A descrição do equipamento não pode ser vazia")]
    EquipamentoVazio,

    /// Descrição do problema vazia no laudo.
    #[error("A descrição do problema não pode ser vazia")]
    DescricaoDoProblemaVazia,

    /// Descrição vazia num item de mão de obra.
    #[error("A descrição do serviço não pode ser vazia")]
    DescricaoDeServicoVazia,

    /// Transição de estado não permitida a partir do estado atual.
    #[error("A ordem de serviço está \"{atual}\", não \"{esperado}\"")]
    EstadoInvalido {
        /// O estado atual.
        atual: &'static str,
        /// O estado que a operação exigia.
        esperado: &'static str,
    },

    /// `EnviarParaAprovacao` sem nenhum item no orçamento.
    #[error("O orçamento está vazio — adicione ao menos uma peça ou mão de obra")]
    OrcamentoVazio,

    /// `AprovarOrcamentoOs`/`ReprovarOrcamentoOs` sobre um orçamento já decidido.
    #[error("Este orçamento já foi decidido")]
    OrcamentoJaDecidido,

    /// `AprovarOrcamentoOs` sem identificar quem aprovou — a aprovação nunca é implícita.
    #[error("A aprovação exige identificar quem aprovou (nome e documento, ou assinatura)")]
    AprovacaoSemIdentificacao,

    /// `IniciarExecucao` sem orçamento aprovado.
    #[error("O orçamento ainda não foi aprovado")]
    OrcamentoNaoAprovado,

    /// A peça referenciada não pertence à ordem de serviço informada.
    #[error("Este item de peça não pertence a esta ordem de serviço")]
    ItemNaoPertenceAOrdem,

    /// Peça já aplicada não pode ser aplicada de novo.
    #[error("Esta peça já foi aplicada")]
    PecaJaAplicada,

    /// `ConcluirExecucao` com peça do orçamento ainda não aplicada.
    #[error("Ainda há {0} peça(s) do orçamento pendente(s) de aplicação")]
    PecaPendenteDeAplicacao(usize),

    /// `FaturarOrdemServico` sobre uma OS que não está `Concluida`.
    #[error("A ordem de serviço ainda não foi concluída")]
    OsNaoConcluida,

    /// `FaturarOrdemServico` sobre uma OS já faturada.
    #[error("Esta ordem de serviço já foi faturada")]
    OsJaFaturada,

    /// `IniciarApontamento` quando o técnico já tem outro apontamento aberto — ele não pode
    /// estar "trabalhando" em duas ordens de serviço ao mesmo tempo.
    #[error("Este técnico já tem um apontamento de tempo em aberto (em outra ordem de serviço)")]
    TecnicoJaTemApontamentoAberto,

    /// `EncerrarApontamento` sobre um apontamento que já foi encerrado.
    #[error("Este apontamento de tempo já foi encerrado")]
    ApontamentoJaEncerrado,

    /// `EncerrarApontamento`/`AjustarApontamento` com fim anterior ao início.
    #[error("O fim do apontamento não pode ser antes do início")]
    FimAntesDoInicio,

    /// `AjustarApontamento` sem motivo — correção pós-fato nunca é silenciosa.
    #[error("Ajustar um apontamento de tempo exige informar o motivo")]
    MotivoDeAjusteObrigatorio,
}

impl ErroDominio for ErroOs {
    fn codigo(&self) -> CodigoErro {
        match self {
            Self::EquipamentoVazio
            | Self::DescricaoDoProblemaVazia
            | Self::DescricaoDeServicoVazia
            | Self::OrcamentoVazio => CodigoErro::ENTRADA_INVALIDA,
            Self::EstadoInvalido { .. } => CodigoErro::ESTADO_INVALIDO,
            Self::AprovacaoSemIdentificacao | Self::MotivoDeAjusteObrigatorio => {
                CodigoErro::CAMPO_OBRIGATORIO
            }
            Self::OrcamentoJaDecidido
            | Self::OrcamentoNaoAprovado
            | Self::ItemNaoPertenceAOrdem
            | Self::PecaJaAplicada
            | Self::PecaPendenteDeAplicacao(_)
            | Self::OsNaoConcluida
            | Self::OsJaFaturada
            | Self::TecnicoJaTemApontamentoAberto
            | Self::ApontamentoJaEncerrado
            | Self::FimAntesDoInicio => CodigoErro::REGRA_VIOLADA,
        }
    }

    fn detalhes(&self) -> Option<Detalhes> {
        match self {
            Self::PecaPendenteDeAplicacao(n) => Some(Detalhes::nova(
                "Peças do orçamento ainda não aplicadas",
                format!(
                    "{n} peça(s) do orçamento aprovado ainda não foram aplicadas ao estoque — \
                     aplique todas antes de concluir a execução, para não deixar consumo órfão."
                ),
            )),
            _ => None,
        }
    }
}
