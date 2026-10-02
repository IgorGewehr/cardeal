//! Métricas no formato texto do Prometheus — sem dependência nova: contadores atômicos e um
//! histograma de latência por tipo de requisição. Custo no caminho quente: dois `fetch_add`.
//!
//! `GET /metricas` só existe com `CARDEAL_METRICAS_TOKEN` definido e pede
//! `Authorization: Bearer <token>` — atrás do túnel, todo pedido chega do mesmo IP, então
//! "só de localhost" não protegeria nada.

use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Limites (em ms) dos baldes do histograma de latência.
const BALDES_MS: [f64; 11] = [
    0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 1000.0,
];

/// O tipo de uma requisição, para separar as séries.
#[derive(Clone, Copy)]
pub(crate) enum Tipo {
    Comando,
    Consulta,
    Sessao,
    Estatico,
    Outro,
}

impl Tipo {
    const TODOS: [Self; 5] = [
        Self::Comando,
        Self::Consulta,
        Self::Sessao,
        Self::Estatico,
        Self::Outro,
    ];

    pub(crate) fn de_caminho(caminho: &str) -> Self {
        if caminho.starts_with("/v1/e/") && caminho.ends_with("/eventos") {
            Self::Outro
        } else if caminho.starts_with("/v1/e/") {
            if caminho.contains("/cmd/") {
                Self::Comando
            } else {
                Self::Consulta
            }
        } else if caminho.starts_with("/v1/sessao") {
            Self::Sessao
        } else if caminho == "/" || caminho.starts_with("/app/") {
            Self::Estatico
        } else {
            Self::Outro
        }
    }

    const fn rotulo(self) -> &'static str {
        match self {
            Self::Comando => "comando",
            Self::Consulta => "consulta",
            Self::Sessao => "sessao",
            Self::Estatico => "estatico",
            Self::Outro => "outro",
        }
    }
}

#[derive(Default)]
struct Serie {
    total: AtomicU64,
    erros_4xx: AtomicU64,
    erros_5xx: AtomicU64,
    soma_us: AtomicU64,
    baldes: [AtomicU64; BALDES_MS.len()],
}

/// Os contadores do processo.
#[derive(Default)]
pub struct Metricas {
    series: [Serie; 5],
    logins_recusados: AtomicU64,
    limite_ip: AtomicU64,
}

impl Metricas {
    pub(crate) fn registrar(&self, tipo: Tipo, status: u16, duracao: Duration) {
        let s = &self.series[tipo as usize];
        s.total.fetch_add(1, Ordering::Relaxed);
        match status {
            400..=499 => s.erros_4xx.fetch_add(1, Ordering::Relaxed),
            500..=599 => s.erros_5xx.fetch_add(1, Ordering::Relaxed),
            _ => 0,
        };
        let us = u64::try_from(duracao.as_micros()).unwrap_or(u64::MAX);
        s.soma_us.fetch_add(us, Ordering::Relaxed);
        #[allow(clippy::cast_precision_loss)]
        let ms = us as f64 / 1000.0;
        if let Some(i) = BALDES_MS.iter().position(|&b| ms <= b) {
            s.baldes[i].fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(crate) fn login_recusado(&self) {
        self.logins_recusados.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn limite_ip_atingido(&self) {
        self.limite_ip.fetch_add(1, Ordering::Relaxed);
    }

    /// O texto do Prometheus, com os medidores instantâneos que só o chamador conhece.
    pub(crate) fn exportar(
        &self,
        empresas_abertas: usize,
        sessoes_em_cache: usize,
        conexoes_tempo_real: usize,
    ) -> String {
        let mut o = String::with_capacity(4096);
        let _ = writeln!(o, "# TYPE cardeal_requisicoes_total counter");
        let _ = writeln!(o, "# TYPE cardeal_requisicoes_erros_total counter");
        let _ = writeln!(o, "# TYPE cardeal_requisicao_segundos histogram");
        for t in Tipo::TODOS {
            let s = &self.series[t as usize];
            let r = t.rotulo();
            let total = s.total.load(Ordering::Relaxed);
            let _ = writeln!(o, "cardeal_requisicoes_total{{tipo=\"{r}\"}} {total}");
            let _ = writeln!(
                o,
                "cardeal_requisicoes_erros_total{{tipo=\"{r}\",classe=\"4xx\"}} {}",
                s.erros_4xx.load(Ordering::Relaxed)
            );
            let _ = writeln!(
                o,
                "cardeal_requisicoes_erros_total{{tipo=\"{r}\",classe=\"5xx\"}} {}",
                s.erros_5xx.load(Ordering::Relaxed)
            );
            let mut acumulado = 0;
            for (i, limite) in BALDES_MS.iter().enumerate() {
                acumulado += s.baldes[i].load(Ordering::Relaxed);
                let _ = writeln!(
                    o,
                    "cardeal_requisicao_segundos_bucket{{tipo=\"{r}\",le=\"{}\"}} {acumulado}",
                    limite / 1000.0
                );
            }
            let _ = writeln!(
                o,
                "cardeal_requisicao_segundos_bucket{{tipo=\"{r}\",le=\"+Inf\"}} {total}"
            );
            #[allow(clippy::cast_precision_loss)]
            let soma = s.soma_us.load(Ordering::Relaxed) as f64 / 1e6;
            let _ = writeln!(o, "cardeal_requisicao_segundos_sum{{tipo=\"{r}\"}} {soma}");
            let _ = writeln!(
                o,
                "cardeal_requisicao_segundos_count{{tipo=\"{r}\"}} {total}"
            );
        }
        let _ = writeln!(o, "# TYPE cardeal_logins_recusados_total counter");
        let _ = writeln!(
            o,
            "cardeal_logins_recusados_total {}",
            self.logins_recusados.load(Ordering::Relaxed)
        );
        let _ = writeln!(o, "# TYPE cardeal_limite_login_ip_total counter");
        let _ = writeln!(
            o,
            "cardeal_limite_login_ip_total {}",
            self.limite_ip.load(Ordering::Relaxed)
        );
        let _ = writeln!(o, "# TYPE cardeal_empresas_abertas gauge");
        let _ = writeln!(o, "cardeal_empresas_abertas {empresas_abertas}");
        let _ = writeln!(o, "# TYPE cardeal_tempo_real_conexoes gauge");
        let _ = writeln!(o, "cardeal_tempo_real_conexoes {conexoes_tempo_real}");
        let _ = writeln!(o, "# TYPE cardeal_sessoes_em_cache gauge");
        let _ = writeln!(o, "cardeal_sessoes_em_cache {sessoes_em_cache}");
        let replicacao = cardeal_storage::saude_replicacao();
        let _ = writeln!(o, "# TYPE cardeal_replicacao_bases_em_falha gauge");
        let _ = writeln!(
            o,
            "cardeal_replicacao_bases_em_falha {}",
            replicacao.bases_em_falha
        );
        let _ = writeln!(o, "# TYPE cardeal_replicacao_falhas_total counter");
        let _ = writeln!(
            o,
            "cardeal_replicacao_falhas_total {}",
            replicacao.falhas_total
        );
        if let Some(rss) = memoria_residente() {
            let _ = writeln!(o, "# TYPE process_resident_memory_bytes gauge");
            let _ = writeln!(o, "process_resident_memory_bytes {rss}");
        }
        o
    }
}

/// RSS do processo (Linux: `/proc/self/statm`, em páginas de 4 KiB).
fn memoria_residente() -> Option<u64> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let paginas: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    Some(paginas * 4096)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn classifica_caminhos_e_acumula_baldes() {
        assert!(matches!(
            Tipo::de_caminho("/v1/e/x/cmd/a.v1"),
            Tipo::Comando
        ));
        assert!(matches!(
            Tipo::de_caminho("/v1/e/x/qry/a.v1"),
            Tipo::Consulta
        ));
        assert!(matches!(Tipo::de_caminho("/v1/sessao"), Tipo::Sessao));
        assert!(matches!(
            Tipo::de_caminho("/app/abc/x.wasm"),
            Tipo::Estatico
        ));

        let m = Metricas::default();
        m.registrar(Tipo::Consulta, 200, Duration::from_micros(300));
        m.registrar(Tipo::Consulta, 503, Duration::from_millis(30));
        let t = m.exportar(3, 7, 2);
        assert!(t.contains("cardeal_requisicoes_total{tipo=\"consulta\"} 2"));
        assert!(t.contains("cardeal_requisicoes_erros_total{tipo=\"consulta\",classe=\"5xx\"} 1"));
        assert!(t.contains("cardeal_requisicao_segundos_bucket{tipo=\"consulta\",le=\"0.0005\"} 1"));
        assert!(t.contains("cardeal_requisicao_segundos_bucket{tipo=\"consulta\",le=\"0.05\"} 2"));
        assert!(t.contains("cardeal_empresas_abertas 3"));
        assert!(t.contains("cardeal_tempo_real_conexoes 2"));
    }
}
