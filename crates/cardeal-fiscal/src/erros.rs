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

    /// O arquivo não é um certificado A1 legível (`.pfx`/`.p12`).
    #[error("O arquivo não é um certificado A1 válido: {0}")]
    CertificadoInvalido(String),

    /// A senha não abre o certificado.
    #[error("A senha do certificado está incorreta")]
    SenhaDoCertificadoIncorreta,

    /// A API fiscal está indisponível (sem rede, fora do ar, sobrecarregada). Tentar de novo
    /// depois resolve — por isso o fiscal é assíncrono e a venda não espera.
    #[error("A API fiscal não respondeu — confira a internet e tente de novo em instantes")]
    ServicoIndisponivel,

    /// A API fiscal recusou a chave de acesso (`CARDEAL_FISCAL_TOKEN`).
    #[error("A chave de acesso da API fiscal foi recusada — fale com o suporte")]
    CredencialDaApiFiscal,

    /// A API fiscal recusou o pedido, com o motivo dela.
    #[error("A API fiscal recusou o pedido: {0}")]
    ApiFiscalRecusou(String),

    /// O provedor fiscal configurado ainda não oferece esta operação.
    #[error("O provedor fiscal ainda não oferece {0}")]
    NaoSuportadoPeloProvedor(String),
}

impl ErroDominio for ErroFiscal {
    fn codigo(&self) -> CodigoErro {
        match self {
            Self::ChaveDeAcessoInvalida
            | Self::XmlInvalido(_)
            | Self::CertificadoInvalido(_)
            | Self::SenhaDoCertificadoIncorreta => CodigoErro::ENTRADA_INVALIDA,
            Self::ServicoIndisponivel => CodigoErro::FISCAL_INDISPONIVEL,
            Self::CredencialDaApiFiscal => CodigoErro::CREDENCIAL_INVALIDA,
            Self::ApiFiscalRecusou(_) | Self::NaoSuportadoPeloProvedor(_) => {
                CodigoErro::REGRA_VIOLADA
            }
        }
    }

    fn detalhes(&self) -> Option<Detalhes> {
        None
    }
}
