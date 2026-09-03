//! Erros da camada de persistência.

use cardeal_kernel::{AcaoSugerida, CodigoErro, Detalhes, ErroDominio};

/// Tudo que pode dar errado ao abrir, migrar, escrever ou ler a base.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErroArmazenamento {
    /// Falha ao abrir o arquivo do banco ou configurar a conexão.
    #[error("Não foi possível abrir a base: {0}")]
    Abertura(String),

    /// Erro do SQLite ao executar uma operação.
    #[error("Falha no banco de dados: {0}")]
    Sqlite(String),

    /// Uma migração publicada foi alterada — o hash não bate com o registrado
    /// (`docs/06-modelo-de-dados.md` §4, regra 1).
    #[error("A migração {modulo} v{versao} foi alterada depois de publicada — o motor não sobe")]
    MigracaoAlterada {
        /// O módulo dono da migração.
        modulo: String,
        /// A versão da migração.
        versao: u32,
    },

    /// Um conjunto de migrações declara dependência de um módulo que não foi fornecido.
    #[error("As migrações de {modulo} dependem de {dependencia}, que não foi fornecido")]
    DependenciaDeMigracaoAusente {
        /// O módulo que declara a dependência.
        modulo: String,
        /// A dependência ausente.
        dependencia: String,
    },

    /// Falha ao rodar o SQL de uma migração.
    #[error("A migração {modulo} v{versao} ({nome}) falhou: {causa}")]
    MigracaoFalhou {
        /// O módulo.
        modulo: String,
        /// A versão.
        versao: u32,
        /// O nome.
        nome: String,
        /// A causa.
        causa: String,
    },

    /// A thread do escritor não está mais no ar.
    #[error("O escritor da base não está disponível")]
    EscritorIndisponivel,

    /// Uma tarefa de escrita entrou em pânico — foi isolada e desfeita; as demais do lote
    /// seguiram (`docs/07-persistencia-sqlite.md` §4).
    #[error("A operação de escrita falhou de forma inesperada e foi desfeita")]
    TarefaEntrouEmPanico,

    /// O recurso pedido já está travado por outra sessão.
    #[error("O recurso \"{recurso}\" está em uso por outra sessão")]
    RecursoTravado {
        /// O recurso.
        recurso: String,
    },

    /// Bloqueio otimista: a versão informada não é mais a atual.
    #[error("O registro foi alterado por outra pessoa — recarregue e tente de novo")]
    VersaoDesatualizada,
}

impl ErroArmazenamento {
    #[allow(clippy::needless_pass_by_value)] // usado como `.map_err(ErroArmazenamento::sqlite)`
    pub(crate) fn sqlite(e: rusqlite::Error) -> Self {
        Self::Sqlite(e.to_string())
    }
}

impl From<rusqlite::Error> for ErroArmazenamento {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e.to_string())
    }
}

impl ErroDominio for ErroArmazenamento {
    fn codigo(&self) -> CodigoErro {
        match self {
            Self::Abertura(_) | Self::EscritorIndisponivel => CodigoErro::BANCO_INDISPONIVEL,
            Self::Sqlite(_) | Self::MigracaoFalhou { .. } | Self::TarefaEntrouEmPanico => {
                CodigoErro::FALHA_INTERNA
            }
            Self::MigracaoAlterada { .. } | Self::DependenciaDeMigracaoAusente { .. } => {
                CodigoErro::FALHA_DE_DISCO
            }
            Self::RecursoTravado { .. } => CodigoErro::RECURSO_TRAVADO,
            Self::VersaoDesatualizada => CodigoErro::VERSAO_DESATUALIZADA,
        }
    }

    fn detalhes(&self) -> Option<Detalhes> {
        match self {
            Self::VersaoDesatualizada => Some(Detalhes::nova(
                "O registro mudou enquanto você editava",
                "Outra pessoa (ou outro terminal) salvou uma alteração neste mesmo registro. \
                 Recarregue a tela para ver a versão atual e refaça a sua alteração.",
            )),
            Self::RecursoTravado { recurso } => Some(
                Detalhes::nova(
                    "Recurso em uso",
                    format!("\"{recurso}\" está sendo usado por outra sessão neste momento."),
                )
                .com(AcaoSugerida::nova("Tentar novamente", "repetir")),
            ),
            _ => None,
        }
    }
}
