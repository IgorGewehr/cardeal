//! Erros do cliente fiscal.

use cardeal_kernel::{CodigoErro, Detalhes, ErroDominio};

/// Tudo que pode dar errado ao falar com a API fiscal ou ao interpretar o que ela devolve.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErroFiscal {
    /// A chave de acesso não tem 44 dígitos.
    #[error("A chave de acesso precisa ter 44 dígitos")]
    ChaveDeAcessoInvalida,

    /// O XML da nota não pôde ser interpretado — só os campos essenciais são exigidos
    /// (`docs/modulos/compras.md` §5); um XML fora do padrão NF-e 4.00 cai aqui.
    #[error("Não foi possível interpretar o XML da nota: {0}")]
    XmlInvalido(String),

    /// A API fiscal (real) está indisponível — nunca produzido pelo `FiscalSimulado`.
    #[error("A API fiscal está indisponível")]
    ServicoIndisponivel,
}

impl ErroDominio for ErroFiscal {
    fn codigo(&self) -> CodigoErro {
        match self {
            Self::ChaveDeAcessoInvalida | Self::XmlInvalido(_) => CodigoErro::ENTRADA_INVALIDA,
            Self::ServicoIndisponivel => CodigoErro::FISCAL_INDISPONIVEL,
        }
    }

    fn detalhes(&self) -> Option<Detalhes> {
        None
    }
}
