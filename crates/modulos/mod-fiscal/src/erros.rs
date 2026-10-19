//! Erros do módulo fiscal.

use cardeal_kernel::{CodigoErro, Detalhes, ErroDominio};

/// Tudo que pode dar errado ao configurar o fiscal da empresa.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErroModuloFiscal {
    /// O certificado é de pessoa física (e-CPF) ou não traz CNPJ.
    #[error("Este certificado não é de empresa (e-CNPJ) — para emitir nota é preciso o A1 do CNPJ da empresa")]
    CertificadoSemCnpj,

    /// O CNPJ do certificado não é o da empresa (o `sefaz-api` recusaria a emissão).
    #[error("O certificado é do CNPJ {do_certificado}, mas esta empresa é {da_empresa}")]
    CnpjDiferente {
        /// O CNPJ do certificado.
        do_certificado: String,
        /// O CNPJ da empresa.
        da_empresa: String,
    },

    /// O certificado já venceu.
    #[error("Este certificado venceu em {em} — renove com a certificadora e instale o novo")]
    CertificadoVencido {
        /// A data do vencimento, formatada.
        em: String,
    },

    /// Nenhum certificado instalado.
    #[error("Instale o certificado digital A1 da empresa antes")]
    SemCertificado,

    /// Faltam IE/endereço/código IBGE da empresa.
    #[error("Complete os dados fiscais da empresa (inscrição estadual e endereço com código IBGE) antes")]
    DadosFiscaisIncompletos,

    /// Esta instalação não tem a chave da API fiscal.
    #[error("A API fiscal não está configurada nesta instalação (variável CARDEAL_FISCAL_TOKEN) — fale com o suporte")]
    ProvedorNaoConfigurado,
}

impl ErroDominio for ErroModuloFiscal {
    fn codigo(&self) -> CodigoErro {
        match self {
            Self::ProvedorNaoConfigurado => CodigoErro::ESTADO_INVALIDO,
            _ => CodigoErro::REGRA_VIOLADA,
        }
    }

    fn detalhes(&self) -> Option<Detalhes> {
        None
    }
}
