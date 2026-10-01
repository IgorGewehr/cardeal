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
mod config;
pub mod diretorio;
pub mod frota;
mod http;
pub mod provisionamento;
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
}

impl Servidor {
    /// Abre o diretório e prepara a frota (nenhuma empresa é aberta aqui — boot instantâneo
    /// e RAM mínima, independentemente de quantas empresas o servidor hospeda).
    ///
    /// # Errors
    /// Falha ao abrir/migrar o diretório.
    pub fn abrir(config: ConfigServidor) -> Resultado<Arc<Self>> {
        let diretorio = Diretorio::abrir(&config.caminho_diretorio())?;
        let plano = Plano::preparar(
            &cardeal_distribuicao::modulos(),
            &cardeal_distribuicao::pedido_ativacao(),
        )?;
        let frota = Frota::nova(
            Arc::clone(&plano),
            config.pasta_empresas(),
            config.ociosidade,
            config.teto_empresas,
        );
        Ok(Arc::new(Self {
            logins: tokio::sync::Semaphore::new(config.logins_simultaneos.max(1)),
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
        self.frota.despejar_ociosas();
        if let Err(e) = self.diretorio.limpar_sessoes_expiradas(Instante::agora()) {
            tracing::warn!(erro = %e.mensagem, "limpeza de sessões falhou");
        }
        self.sessoes.podar();
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
