//! Erros de autenticação e autorização.

use cardeal_kernel::{AcaoSugerida, CodigoErro, Detalhes, ErroDominio, Instante};

/// Tudo que pode dar errado em senha, bloqueio e autorização.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErroAuth {
    /// A senha tem menos caracteres que o mínimo da política.
    #[error("A senha precisa de ao menos {minimo} caracteres")]
    SenhaCurta {
        /// O mínimo exigido.
        minimo: u8,
    },

    /// A senha está entre as mais comuns em vazamentos públicos.
    #[error("Esta senha é comum demais — escolha outra")]
    SenhaComum,

    /// Falha interna do Argon2id (parâmetros inválidos). Não deveria acontecer em
    /// produção — os parâmetros são constantes do módulo.
    #[error("Falha interna ao gerar o hash da senha")]
    FalhaDeHash,

    /// Login ou senha incorretos. Deliberadamente genérico — não diz qual dos dois
    /// errou, para não ajudar enumeração de usuários válidos.
    #[error("Login ou senha incorretos")]
    CredencialInvalida,

    /// A conta está temporariamente bloqueada por excesso de tentativas.
    #[error("Conta bloqueada até {ate}")]
    ContaBloqueada {
        /// Até quando.
        ate: Instante,
    },

    /// A ação exigia um escopo que a sessão atual não cobre.
    #[error("Fora do escopo autorizado")]
    ForaDoEscopo,

    /// A sessão não concede a permissão exigida pela ação.
    #[error("Sem permissão para \"{permissao}\"")]
    SemPermissao {
        /// A chave de permissão que faltou.
        permissao: String,
    },

    /// A sessão já foi encerrada (logout ou revogação).
    #[error("Sessão encerrada")]
    SessaoEncerrada,

    /// Tentativa de alterar um papel de fábrica, que é imutável.
    #[error("Papéis de fábrica não podem ser alterados")]
    PapelDoSistema,

    /// A senha atual informada para a troca não confere.
    #[error("A senha atual não confere")]
    SenhaAtualIncorreta,

    /// Falha de infraestrutura ao ler ou gravar (SQLite, disco).
    #[error("Falha ao acessar o armazenamento: {0}")]
    FalhaDePersistencia(String),
}

impl ErroDominio for ErroAuth {
    fn codigo(&self) -> CodigoErro {
        match self {
            Self::SenhaCurta { .. } | Self::SenhaComum => CodigoErro::ENTRADA_INVALIDA,
            Self::FalhaDeHash | Self::FalhaDePersistencia(_) => CodigoErro::FALHA_INTERNA,
            Self::CredencialInvalida | Self::SenhaAtualIncorreta => CodigoErro::CREDENCIAL_INVALIDA,
            Self::ContaBloqueada { .. } => CodigoErro::CONTA_BLOQUEADA,
            Self::ForaDoEscopo | Self::SemPermissao { .. } => CodigoErro::SEM_PERMISSAO,
            Self::SessaoEncerrada => CodigoErro::SESSAO_INVALIDA,
            Self::PapelDoSistema => CodigoErro::ESTADO_INVALIDO,
        }
    }

    fn detalhes(&self) -> Option<Detalhes> {
        match self {
            Self::ContaBloqueada { ate } => Some(
                Detalhes::nova(
                    "Muitas tentativas incorretas",
                    format!(
                        "Por segurança, o acesso ficou temporariamente bloqueado até {}. \
                         Um administrador pode liberar antes do prazo.",
                        ate.formatar(cardeal_kernel::Fuso::BRASILIA)
                    ),
                )
                .com(AcaoSugerida::nova(
                    "Pedir liberação ao administrador",
                    "auth.desbloquear",
                )),
            ),
            Self::CredencialInvalida => Some(Detalhes::nova(
                "Login ou senha incorretos",
                "Confira os dados e tente novamente.",
            )),
            _ => None,
        }
    }
}
