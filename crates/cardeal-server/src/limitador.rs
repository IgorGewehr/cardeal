//! Limite de tentativas de login por IP — a segunda camada contra força bruta, somada ao
//! bloqueio progressivo por conta (que sozinho não para quem tenta *muitas contas* com a mesma
//! senha).
//!
//! Janela fixa por IP, em memória, com teto de entradas: memória limitada mesmo sob ataque
//! distribuído. Com o mapa cheio, um IP novo passa (falha aberta) — o semáforo de logins já
//! impede que isso vire falta de RAM ou CPU, e travar todo mundo seria o ataque bem-sucedido.

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

/// O limitador.
pub struct LimitadorLogin {
    janela: Duration,
    maximo: u32,
    teto_entradas: usize,
    mapa: Mutex<HashMap<IpAddr, (u32, Instant)>>,
}

impl LimitadorLogin {
    /// `maximo` tentativas por `janela`, por IP, guardando no máximo `teto_entradas` IPs.
    #[must_use]
    pub fn novo(maximo: u32, janela: Duration, teto_entradas: usize) -> Self {
        Self {
            janela,
            maximo,
            teto_entradas,
            mapa: Mutex::new(HashMap::new()),
        }
    }

    /// Registra uma tentativa; `false` se o IP passou do limite na janela.
    pub fn permitir(&self, ip: IpAddr) -> bool {
        let agora = Instant::now();
        let mut mapa = self.mapa.lock();
        if !mapa.contains_key(&ip) && mapa.len() >= self.teto_entradas {
            let janela = self.janela;
            mapa.retain(|_, (_, inicio)| agora.duration_since(*inicio) < janela);
            if mapa.len() >= self.teto_entradas {
                tracing::warn!(%ip, "limitador de login cheio — deixando passar");
                return true;
            }
        }
        let entrada = mapa.entry(ip).or_insert((0, agora));
        if agora.duration_since(entrada.1) >= self.janela {
            *entrada = (0, agora);
        }
        entrada.0 += 1;
        entrada.0 <= self.maximo
    }

    /// Esquece as janelas vencidas.
    pub fn podar(&self) {
        let agora = Instant::now();
        let janela = self.janela;
        self.mapa
            .lock()
            .retain(|_, (_, inicio)| agora.duration_since(*inicio) < janela);
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn bloqueia_acima_do_maximo_e_isola_ips() {
        let l = LimitadorLogin::novo(3, Duration::from_secs(60), 100);
        let a: IpAddr = "10.0.0.1".parse().unwrap();
        let b: IpAddr = "10.0.0.2".parse().unwrap();
        assert!((0..3).all(|_| l.permitir(a)));
        assert!(!l.permitir(a));
        assert!(l.permitir(b), "outro IP não paga pelo primeiro");
    }

    #[test]
    fn janela_vencida_libera_e_mapa_cheio_falha_aberto() {
        let l = LimitadorLogin::novo(1, Duration::ZERO, 1);
        let a: IpAddr = "10.0.0.1".parse().unwrap();
        assert!(l.permitir(a));
        assert!(l.permitir(a), "janela zero: sempre renovada");

        let cheio = LimitadorLogin::novo(1, Duration::from_secs(60), 1);
        assert!(cheio.permitir(a));
        assert!(
            cheio.permitir("10.0.0.9".parse().unwrap()),
            "mapa cheio: falha aberto"
        );
    }
}
