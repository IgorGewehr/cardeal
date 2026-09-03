//! Erros do módulo de cadastro de pessoas.

use cardeal_kernel::{CodigoErro, Detalhes, ErroDominio};

use crate::pessoa::Papel;

/// Tudo que pode dar errado no cadastro, nos papéis, no crédito e na deduplicação.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErroClientes {
    /// O nome (ou razão social) veio vazio.
    #[error("O nome não pode ser vazio")]
    NomeVazio,

    /// `nome_fantasia` só faz sentido para pessoa jurídica.
    #[error("Nome fantasia só se aplica a pessoa jurídica")]
    FantasiaEmPessoaFisica,

    /// O documento informado não passou na validação de dígito verificador.
    #[error("Documento inválido: {0}")]
    DocumentoInvalido(&'static str),

    /// Um contato (e-mail, telefone) não bate com o formato do seu tipo.
    #[error("{0}")]
    ContatoInvalido(&'static str),

    /// CEP fora do formato de 8 dígitos.
    #[error("O CEP precisa ter 8 dígitos")]
    CepInvalido,

    /// UF não é uma das 27 siglas válidas.
    #[error("UF inválida")]
    UfInvalida,

    /// A pessoa já tem esse papel ativo.
    #[error("A pessoa já tem o papel {0:?}")]
    PapelJaExiste(Papel),

    /// Tentativa de remover um papel que ainda tem pendência (título em aberto).
    #[error("O papel {0:?} tem pendência em aberto e não pode ser removido agora")]
    PapelComPendencia(Papel),

    /// Operação de cadastro sobre uma pessoa já anonimizada (estado terminal).
    #[error("Esta pessoa foi anonimizada (LGPD) e não aceita mais alterações cadastrais")]
    PessoaAnonimizada,

    /// Limite de crédito negativo.
    #[error("O limite de crédito não pode ser negativo")]
    LimiteNegativo,

    /// Liberação de crédito sem motivo.
    #[error("A liberação de crédito exige um motivo")]
    LiberacaoSemMotivo,

    /// Tentativa de liberar um crédito que não está bloqueado.
    #[error("O crédito não está bloqueado")]
    CreditoNaoBloqueado,
}

impl ErroDominio for ErroClientes {
    fn codigo(&self) -> CodigoErro {
        match self {
            Self::NomeVazio
            | Self::FantasiaEmPessoaFisica
            | Self::ContatoInvalido(_)
            | Self::CepInvalido
            | Self::UfInvalida
            | Self::LimiteNegativo
            | Self::LiberacaoSemMotivo => CodigoErro::ENTRADA_INVALIDA,
            Self::DocumentoInvalido(_) => CodigoErro::DOCUMENTO_INVALIDO,
            Self::PapelJaExiste(_) => CodigoErro::DUPLICADO,
            Self::PapelComPendencia(_) | Self::CreditoNaoBloqueado => CodigoErro::REGRA_VIOLADA,
            Self::PessoaAnonimizada => CodigoErro::ESTADO_INVALIDO,
        }
    }

    fn detalhes(&self) -> Option<Detalhes> {
        match self {
            Self::PessoaAnonimizada => Some(Detalhes::nova(
                "Cadastro anonimizado",
                "Os dados pessoais desta pessoa foram apagados a pedido do titular (LGPD). \
                 Os títulos e lançamentos ligados a ela continuam válidos, mas o cadastro \
                 não pode mais ser editado.",
            )),
            Self::PapelComPendencia(_) => Some(Detalhes::nova(
                "Ainda há pendências neste papel",
                "Zere os títulos em aberto ligados a este papel antes de removê-lo — o \
                 histórico continua acessível depois.",
            )),
            _ => None,
        }
    }
}
