//! Parâmetros do servidor e o layout do diretório de dados.
//!
//! ```text
//! <dados>/diretorio.db          contas, empresas, vínculos, sessões
//! <dados>/empresas/<id>.db      uma base SQLite por empresa
//! ```

use std::path::{Path, PathBuf};
use std::time::Duration;

use cardeal_kernel::Id;

/// Configuração de um [`Servidor`](crate::Servidor).
#[derive(Debug, Clone)]
pub struct ConfigServidor {
    /// Raiz dos dados.
    pub dados: PathBuf,
    /// Tempo sem uso depois do qual uma empresa é fechada e sai da memória.
    pub ociosidade: Duration,
    /// Máximo de empresas abertas ao mesmo tempo; acima disso, a menos usada é despejada.
    pub teto_empresas: usize,
    /// Quantos logins (Argon2id, 19 MiB cada) podem rodar ao mesmo tempo — o pico de RAM de
    /// hash fica fixo, não cresce com a carga.
    pub logins_simultaneos: usize,
    /// Validade de uma sessão.
    pub validade_sessao: Duration,
    /// Tentativas de login por IP dentro de [`Self::janela_login`].
    pub logins_por_ip: u32,
    /// A janela do limite de login por IP.
    pub janela_login: Duration,
    /// Confiar no cabeçalho `CF-Connecting-IP` para saber o IP do cliente — **só** quando o
    /// servidor está atrás do Cloudflare Tunnel (senão qualquer um forja o cabeçalho).
    pub confiar_cloudflare: bool,
    /// Por quanto tempo uma resposta idempotente é guardada (reenvios depois disso executam de
    /// novo). Reenvio por queda de rede acontece em segundos; uma semana é folga de sobra.
    pub retencao_idempotencia: Duration,
}

impl ConfigServidor {
    /// Padrões da ADR-0016.
    #[must_use]
    pub fn em(dados: impl Into<PathBuf>) -> Self {
        Self {
            dados: dados.into(),
            ociosidade: Duration::from_secs(10 * 60),
            teto_empresas: 256,
            logins_simultaneos: 2,
            validade_sessao: Duration::from_secs(14 * 24 * 60 * 60),
            logins_por_ip: 30,
            janela_login: Duration::from_secs(5 * 60),
            confiar_cloudflare: false,
            retencao_idempotencia: Duration::from_secs(7 * 24 * 60 * 60),
        }
    }

    /// O arquivo do diretório global.
    #[must_use]
    pub fn caminho_diretorio(&self) -> PathBuf {
        self.dados.join("diretorio.db")
    }

    /// A pasta das bases de empresa.
    #[must_use]
    pub fn pasta_empresas(&self) -> PathBuf {
        self.dados.join("empresas")
    }

    /// O arquivo da base de uma empresa.
    #[must_use]
    pub fn caminho_empresa(&self, empresa: Id) -> PathBuf {
        caminho_empresa(&self.pasta_empresas(), empresa)
    }
}

pub(crate) fn caminho_empresa(pasta: &Path, empresa: Id) -> PathBuf {
    pasta.join(format!("{empresa}.db"))
}
