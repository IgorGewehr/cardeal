//! # cardeal-server
//!
//! O Cardeal online: **um processo, muitas empresas, um SQLite por empresa** (ADR-0016).
//!
//! ```text
//! HTTP (axum) ─► sessão de conta (cache) ─► vínculo conta→empresa ─► Frota ─► MotorLocal
//!                                                                     │        └─ Despachante
//!                                                                     └─ abre sob demanda,
//!                                                                        fecha ociosas
//! ```
//!
//! O domínio não sabe que está num servidor: cada empresa roda exatamente o mesmo
//! `cardeal_motor::MotorLocal` do desktop, com o mesmo despacho, autorização e idempotência.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]
#![allow(clippy::result_large_err)] // `Erro` é grande de propósito (detalhes ao usuário)

pub mod autenticacao;
pub mod backup;
mod config;
pub mod diretorio;
pub mod frota;
mod http;
mod limitador;
mod metricas;
pub mod provisionamento;
pub mod restauracao;
pub mod sessoes;
mod token;

use std::sync::Arc;
use std::time::Duration;

use cardeal_kernel::{Instante, Resultado};
use cardeal_motor::Plano;

pub use config::ConfigServidor;
pub use http::roteador;

use crate::diretorio::Diretorio;
use crate::frota::Frota;
use crate::sessoes::Sessoes;

/// O estado compartilhado por todas as requisições.
pub struct Servidor {
    config: ConfigServidor,
    plano: Arc<Plano>,
    diretorio: Diretorio,
    frota: Frota,
    sessoes: Sessoes,
    logins: tokio::sync::Semaphore,
    limitador: limitador::LimitadorLogin,
    metricas: metricas::Metricas,
}

impl Servidor {
    /// Abre o diretório e prepara a frota (nenhuma empresa é aberta aqui — boot instantâneo
    /// e RAM mínima, independentemente de quantas empresas o servidor hospeda).
    ///
    /// # Errors
    /// Falha ao abrir/migrar o diretório.
    pub fn abrir(config: ConfigServidor) -> Resultado<Arc<Self>> {
        let diretorio =
            Diretorio::abrir(&config.caminho_diretorio(), config.replicacao_diretorio())?;
        let plano = Plano::preparar(
            &cardeal_distribuicao::modulos(),
            &cardeal_distribuicao::pedido_ativacao(),
        )?;
        let frota = Frota::nova(Arc::clone(&plano), config.clone());
        Ok(Arc::new(Self {
            logins: tokio::sync::Semaphore::new(config.logins_simultaneos.max(1)),
            limitador: limitador::LimitadorLogin::novo(
                config.logins_por_ip,
                config.janela_login,
                100_000,
            ),
            metricas: metricas::Metricas::default(),
            config,
            plano,
            diretorio,
            frota,
            sessoes: Sessoes::default(),
        }))
    }

    /// A configuração.
    #[must_use]
    pub const fn config(&self) -> &ConfigServidor {
        &self.config
    }

    /// A distribuição preparada, compartilhada por todas as empresas.
    #[must_use]
    pub const fn plano(&self) -> &Arc<Plano> {
        &self.plano
    }

    /// O diretório global.
    #[must_use]
    pub const fn diretorio(&self) -> &Diretorio {
        &self.diretorio
    }

    /// A frota de empresas abertas.
    #[must_use]
    pub const fn frota(&self) -> &Frota {
        &self.frota
    }

    /// Uma rodada de manutenção: fecha empresas ociosas, apaga sessões expiradas e poda o
    /// cache. **Bloqueante.**
    pub fn manutencao(&self) {
        let retencao =
            i64::try_from(self.config.retencao_idempotencia.as_secs()).unwrap_or(i64::MAX);
        self.frota
            .despejar_ociosas(Instante::agora().mais_segundos(-retencao));
        self.limitador.podar();
        if let Err(e) = self.diretorio.limpar_sessoes_expiradas(Instante::agora()) {
            tracing::warn!(erro = %e.mensagem, "limpeza de sessões falhou");
        }
        self.sessoes.podar();
    }

    /// Fecha todas as empresas abertas (desligamento): cada base termina o lote em curso e
    /// sai limpa. **Bloqueante.**
    pub fn encerrar(&self) {
        let n = self.frota.fechar_todas();
        tracing::info!(fechadas = n, "empresas fechadas no desligamento");
    }

    /// Uma rodada de backup (ver [`backup`]). **Bloqueante.**
    ///
    /// # Errors
    /// A pasta de backup não pôde ser criada.
    pub fn fazer_backup(&self, reter: usize) -> Resultado<backup::RelatorioBackup> {
        backup::executar(&self.config, reter)
    }

    /// Faz backup a cada `intervalo`, para sempre (a primeira rodada logo ao subir).
    pub async fn manter_backup(self: Arc<Self>, intervalo: Duration, reter: usize) {
        let mut relogio = tokio::time::interval(intervalo);
        relogio.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            relogio.tick().await;
            let s = Arc::clone(&self);
            let _ = tokio::task::spawn_blocking(move || s.fazer_backup(reter)).await;
        }
    }

    /// Roda [`Self::manutencao`] a cada `intervalo`, para sempre. Para usar com `tokio::spawn`.
    pub async fn manter(self: Arc<Self>, intervalo: Duration) {
        let mut relogio = tokio::time::interval(intervalo);
        relogio.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            relogio.tick().await;
            let s = Arc::clone(&self);
            let _ = tokio::task::spawn_blocking(move || s.manutencao()).await;
        }
    }
}
