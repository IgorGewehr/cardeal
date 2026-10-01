//! `cardeal-server` — o Cardeal online (ADR-0016).
//!
//! ```text
//! cardeal-server servir      --dados ./dados --endereco 127.0.0.1:8080
//! cardeal-server provisionar --dados ./dados --empresa "Loja" --cnpj … --email … --nome …
//!                            (senha em CARDEAL_SENHA — nunca na linha de comando, que vai
//!                             para o histórico do shell e para o `ps`)
//! ```

#![deny(unsafe_code)] // única exceção: `alocador`, auditada e testada

mod alocador;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use cardeal_server::provisionamento::{provisionar, NovaEmpresa};
use cardeal_server::{roteador, ConfigServidor, Servidor};
use clap::{Parser, Subcommand};

#[global_allocator]
static ALOCADOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Parser)]
#[command(
    name = "cardeal-server",
    version,
    about = "O Cardeal online: muitas empresas, um SQLite por empresa."
)]
struct Cli {
    /// Pasta dos dados (diretório + bases das empresas).
    #[arg(long, env = "CARDEAL_DADOS", default_value = "./dados", global = true)]
    dados: PathBuf,
    #[command(subcommand)]
    acao: Acao,
}

#[derive(Subcommand)]
enum Acao {
    /// Sobe o servidor HTTP.
    Servir {
        /// Endereço de escuta. Atrás do Cloudflare Tunnel, deixe em 127.0.0.1.
        #[arg(long, env = "CARDEAL_ENDERECO", default_value = "127.0.0.1:8080")]
        endereco: SocketAddr,
        /// Threads do runtime assíncrono (o trabalho de banco roda no pool de bloqueio).
        #[arg(long, env = "CARDEAL_TRABALHADORES", default_value_t = 2)]
        trabalhadores: usize,
        /// Minutos sem uso até uma empresa ser fechada.
        #[arg(long, env = "CARDEAL_OCIOSIDADE_MIN", default_value_t = 10)]
        ociosidade_min: u64,
        /// Máximo de empresas abertas ao mesmo tempo.
        #[arg(long, env = "CARDEAL_TETO_EMPRESAS", default_value_t = 256)]
        teto_empresas: usize,
        /// Usar `CF-Connecting-IP` como IP do cliente. Só ligue se o servidor for alcançável
        /// **apenas** pelo Cloudflare Tunnel.
        #[arg(long, env = "CARDEAL_CONFIAR_CLOUDFLARE", default_value_t = false)]
        confiar_cloudflare: bool,
        /// Pasta do cliente do navegador (`cargo xtask construir-web` → `dist/web`).
        #[arg(long, env = "CARDEAL_WEB")]
        web: Option<PathBuf>,
        /// Minutos entre backups (0 desliga). Só bases que mudaram são copiadas.
        #[arg(long, env = "CARDEAL_BACKUP_MIN", default_value_t = 60)]
        backup_min: u64,
        /// Quantas cópias de cada base guardar.
        #[arg(long, env = "CARDEAL_BACKUP_RETER", default_value_t = 48)]
        backup_reter: usize,
        /// Token do `GET /metricas` (Prometheus). Sem ele, a rota não existe.
        #[arg(long, env = "CARDEAL_METRICAS_TOKEN", hide_env_values = true)]
        metricas_token: Option<String>,
    },
    /// Uma rodada de backup agora (para cron ou antes de uma atualização).
    Backup {
        /// Quantas cópias de cada base guardar.
        #[arg(long, default_value_t = 48)]
        reter: usize,
    },
    /// Cria uma empresa nova e dá acesso a uma conta.
    Provisionar {
        /// Razão social.
        #[arg(long)]
        empresa: String,
        /// CNPJ.
        #[arg(long)]
        cnpj: String,
        /// E-mail da conta administradora.
        #[arg(long)]
        email: String,
        /// Nome da pessoa.
        #[arg(long)]
        nome: String,
        /// Senha da conta (só usada se a conta ainda não existe).
        #[arg(long, env = "CARDEAL_SENHA", hide_env_values = true)]
        senha: String,
    },
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=warn".into()),
        )
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .compact()
        .init();
    alocador::configurar();

    let cli = Cli::parse();
    match cli.acao {
        Acao::Servir {
            endereco,
            trabalhadores,
            ociosidade_min,
            teto_empresas,
            confiar_cloudflare,
            web,
            backup_min,
            backup_reter,
            metricas_token,
        } => {
            let mut config = ConfigServidor::em(cli.dados);
            config.ociosidade = Duration::from_secs(ociosidade_min * 60);
            config.teto_empresas = teto_empresas;
            config.confiar_cloudflare = confiar_cloudflare;
            config.web = web;
            config.token_metricas = metricas_token.filter(|t| !t.trim().is_empty());
            let backup =
                (backup_min > 0).then(|| (Duration::from_secs(backup_min * 60), backup_reter));
            servir(config, endereco, trabalhadores, backup)
        }
        Acao::Backup { reter } => {
            let config = ConfigServidor::em(cli.dados);
            let r = cardeal_server::backup::executar(&config, reter)
                .map_err(|e| anyhow::anyhow!(e.mensagem))?;
            println!(
                "backup: {} copiadas, {} sem mudança, {} falhas, {} KiB",
                r.copiadas,
                r.sem_mudanca,
                r.falhas,
                r.bytes / 1024
            );
            if r.falhas > 0 {
                anyhow::bail!("{} bases falharam (ver o log)", r.falhas);
            }
            Ok(())
        }
        Acao::Provisionar {
            empresa,
            cnpj,
            email,
            nome,
            senha,
        } => {
            let config = ConfigServidor::em(cli.dados);
            let servidor = Servidor::abrir(config).map_err(|e| anyhow::anyhow!(e.mensagem))?;
            let feita = provisionar(
                servidor.config(),
                servidor.plano(),
                servidor.diretorio(),
                &NovaEmpresa {
                    razao_social: empresa,
                    cnpj,
                    email,
                    nome,
                    senha,
                },
            )
            .map_err(|e| anyhow::anyhow!(e.mensagem))?;
            println!("empresa {} criada (conta {})", feita.empresa, feita.conta);
            Ok(())
        }
    }
}

fn servir(
    config: ConfigServidor,
    endereco: SocketAddr,
    trabalhadores: usize,
    backup: Option<(Duration, usize)>,
) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(trabalhadores.max(1))
        // O pool de bloqueio (SQLite, Argon2id) cresce sob demanda e encolhe sozinho depois
        // de 10 s ocioso; o teto evita que um pico vire centenas de threads.
        .max_blocking_threads(64)
        .thread_keep_alive(Duration::from_secs(10))
        .thread_name("cardeal")
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let servidor = Servidor::abrir(config).map_err(|e| anyhow::anyhow!(e.mensagem))?;
        tokio::spawn(std::sync::Arc::clone(&servidor).manter(Duration::from_secs(60)));
        if let Some((intervalo, reter)) = backup {
            tokio::spawn(std::sync::Arc::clone(&servidor).manter_backup(intervalo, reter));
        }
        let ouvinte = tokio::net::TcpListener::bind(endereco).await?;
        tracing::info!(%endereco, "cardeal-server no ar");
        axum::serve(
            ouvinte,
            roteador(std::sync::Arc::clone(&servidor))
                .into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(sinal_de_desligamento())
        .await?;
        // Requisições em curso já terminaram: fecha cada base com o lote final confirmado.
        let s = std::sync::Arc::clone(&servidor);
        tokio::task::spawn_blocking(move || s.encerrar()).await?;
        Ok(())
    })
}

/// `Ctrl+C` (terminal) ou `SIGTERM` (Docker, systemd) — os dois encerram com calma.
async fn sinal_de_desligamento() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = term => {},
    }
    tracing::info!("encerrando");
}
