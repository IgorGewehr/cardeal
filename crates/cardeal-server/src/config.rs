//! Parâmetros do servidor e o layout do diretório de dados.
//!
//! ```text
//! <dados>/diretorio.db          contas, empresas, vínculos, sessões
//! <dados>/empresas/<id>.db      uma base SQLite por empresa
//! <dados>/backup/<base>/…       snapshots zstd de hora em hora (ver `backup`)
//! <dados>/replica/<base>/…      replicação contínua do WAL (ver `cardeal_storage::Replicador`)
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
    /// Máximo de conexões de tempo real (SSE) simultâneas — cada uma custa alguns KB.
    pub teto_tempo_real: usize,
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
    /// Vale também para as alterações do tempo real: um cliente desconectado por mais que
    /// isso recebe "recarregar" em vez da lista do que perdeu.
    pub retencao_idempotencia: Duration,
    /// A pasta do cliente do navegador (`cargo xtask construir-web` → `dist/web`). `None`: o
    /// servidor só atende a API.
    pub web: Option<PathBuf>,
    /// Token do `GET /metricas` (Prometheus). `None`: a rota não existe.
    pub token_metricas: Option<String>,
    /// Replicação contínua do WAL de cada base (e do diretório) para `<dados>/replica`.
    /// Ligada por padrão: perda máxima de dados = o último commit.
    pub replicar: bool,
}

impl ConfigServidor {
    /// Padrões da ADR-0016.
    #[must_use]
    pub fn em(dados: impl Into<PathBuf>) -> Self {
        Self {
            dados: dados.into(),
            ociosidade: Duration::from_secs(10 * 60),
            teto_empresas: 256,
            teto_tempo_real: 20_000,
            logins_simultaneos: 2,
            validade_sessao: Duration::from_secs(14 * 24 * 60 * 60),
            logins_por_ip: 30,
            janela_login: Duration::from_secs(5 * 60),
            confiar_cloudflare: false,
            retencao_idempotencia: Duration::from_secs(7 * 24 * 60 * 60),
            web: None,
            token_metricas: None,
            replicar: true,
        }
    }

    /// O arquivo do diretório global.
    #[must_use]
    pub fn caminho_diretorio(&self) -> PathBuf {
        self.dados.join("diretorio.db")
    }

    /// A pasta dos backups (`<dados>/backup/<base>/<carimbo>.db.zst`).
    #[must_use]
    pub fn pasta_backup(&self) -> PathBuf {
        self.dados.join("backup")
    }

    /// A raiz das réplicas (`<dados>/replica/<base>/…`).
    #[must_use]
    pub fn pasta_replica(&self) -> PathBuf {
        self.dados.join("replica")
    }

    /// A configuração de armazenamento de uma empresa: perfil de servidor + réplica.
    #[must_use]
    pub fn armazenamento_empresa(&self, empresa: Id) -> cardeal_storage::ConfigArmazenamento {
        let cfg = cardeal_storage::ConfigArmazenamento::servidor(self.caminho_empresa(empresa));
        if self.replicar {
            cfg.com_replicacao(cardeal_storage::ConfigReplicacao::em(
                self.pasta_replica().join(empresa.to_string()),
            ))
        } else {
            cfg
        }
    }

    /// A réplica do diretório, se a replicação está ligada.
    #[must_use]
    pub fn replicacao_diretorio(&self) -> Option<cardeal_storage::ConfigReplicacao> {
        self.replicar
            .then(|| cardeal_storage::ConfigReplicacao::em(self.pasta_replica().join("diretorio")))
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
