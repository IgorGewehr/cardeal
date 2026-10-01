//! Mede o orçamento de memória e latência da ADR-0016 com números reais, não estimados:
//! N empresas provisionadas, todas abertas por requisição HTTP (roteador em processo), RSS do
//! processo em cada fase e latência de consulta quente.
//!
//! ```text
//! MIMALLOC_ARENA_EAGER_COMMIT=0 cargo run --release -p cardeal-server --example carga -- 200
//! ```

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{header, Request};
use cardeal_kernel::Id;
use cardeal_protocol::{
    rota_consulta, PedidoLogin, RespostaLogin, TipoCliente, CABECALHO_PROTOCOLO, ROTA_SESSAO,
    VERSAO,
};
use cardeal_server::provisionamento::{provisionar, NovaEmpresa};
use cardeal_server::{roteador, ConfigServidor, Servidor};
use mod_clientes::{Papel, PessoasPorPapel};
use tower::ServiceExt as _;

#[global_allocator]
static ALOCADOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn campo_mb(nome: &str) -> f64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    status
        .lines()
        .find(|l| l.starts_with(nome))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|kb| kb.parse::<f64>().ok())
        .map_or(0.0, |kb| kb / 1024.0)
}

/// RSS total, e quanto dela é memória anônima (heap) — o que custa de verdade. Páginas de
/// arquivo (`mmap` das bases) são page cache, que o kernel reclama sob pressão.
fn sqlite_mb() -> f64 {
    // SAFETY: leitura de um contador global do SQLite, sem argumentos.
    #[allow(unsafe_code)]
    let bytes = unsafe { rusqlite::ffi::sqlite3_memory_used() };
    bytes as f64 / 1024.0 / 1024.0
}

fn rss_mb() -> String {
    format!(
        "RSS {:6.1} MB  (heap {:6.1}, arquivo {:4.1}, dentro do SQLite {:5.1}, threads {:3})",
        campo_mb("VmRSS:"),
        campo_mb("RssAnon:"),
        campo_mb("RssFile:"),
        sqlite_mb(),
        campo_mb("Threads:") * 1024.0
    )
}

fn requisicao(uri: &str, token: Option<&str>, corpo: Vec<u8>) -> Request<Body> {
    let mut r = Request::post(uri).header(CABECALHO_PROTOCOLO, VERSAO.to_string());
    if let Some(t) = token {
        r = r.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    r.body(Body::from(corpo)).unwrap()
}

fn main() {
    // Mesmo perfil de alocador do binário (`src/alocador.rs`): purge_delay aqui, e
    // MIMALLOC_ARENA_EAGER_COMMIT=0 no ambiente de quem roda a medição.
    // SAFETY: grava uma opção na tabela global do mimalloc, antes de subir threads.
    #[allow(unsafe_code)]
    unsafe {
        libmimalloc_sys::mi_option_set(15, 0);
    }
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(medir());
}

async fn medir() {
    let n: usize = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(100);
    let pasta = tempfile::tempdir().unwrap();
    let mut config = ConfigServidor::em(pasta.path());
    config.ociosidade = Duration::ZERO;
    config.teto_empresas = n + 10;
    let servidor = Servidor::abrir(config).unwrap();
    println!("processo vazio (diretório aberto) ........ {}", rss_mb());

    let t = Instant::now();
    let mut empresas: Vec<Id> = Vec::with_capacity(n);
    for i in 0..n {
        let p = provisionar(
            servidor.config(),
            servidor.plano(),
            servidor.diretorio(),
            &NovaEmpresa {
                razao_social: format!("Empresa {i:04}"),
                cnpj: "11222333000181".into(),
                email: "contador@x.com".into(),
                nome: "Contador".into(),
                senha: "senha-forte-123".into(),
            },
        )
        .unwrap();
        empresas.push(p.empresa);
    }
    println!(
        "{n} empresas provisionadas em {:.1}s ({:.0} ms cada)",
        t.elapsed().as_secs_f64(),
        t.elapsed().as_millis() as f64 / n as f64
    );
    let s = Arc::clone(&servidor);
    tokio::task::spawn_blocking(move || s.manutencao())
        .await
        .unwrap();
    println!("após provisionar e fechar tudo ........... {}", rss_mb());

    let app = roteador(Arc::clone(&servidor));
    let login = PedidoLogin {
        email: "contador@x.com".into(),
        senha: "senha-forte-123".into(),
        cliente: TipoCliente::Nativo,
    };
    let resp = app
        .clone()
        .oneshot(requisicao(
            ROTA_SESSAO,
            None,
            postcard::to_stdvec(&login).unwrap(),
        ))
        .await
        .unwrap();
    let corpo = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let login: RespostaLogin = postcard::from_bytes(&corpo).unwrap();
    let token = login.token.unwrap();
    println!(
        "login (1 conta com {} empresas) .......... {}",
        login.empresas.len(),
        rss_mb()
    );

    let consulta = postcard::to_stdvec(&PessoasPorPapel {
        papel: Papel::Cliente,
        busca: None,
    })
    .unwrap();
    let rota = |e: Id| rota_consulta(e, "clientes.pessoas_por_papel.v1");

    let mut frias = Vec::with_capacity(n);
    for &e in &empresas {
        let t = Instant::now();
        let r = app
            .clone()
            .oneshot(requisicao(&rota(e), Some(&token), consulta.clone()))
            .await
            .unwrap();
        assert!(r.status().is_success(), "{}", r.status());
        frias.push(t.elapsed());
    }
    let abertas = servidor.frota().abertas();
    let rss_abertas = rss_mb();
    println!("{abertas} empresas ABERTAS ....................... {rss_abertas}");

    let mut quentes = Vec::with_capacity(n * 20);
    for _ in 0..20 {
        for &e in &empresas {
            let t = Instant::now();
            let r = app
                .clone()
                .oneshot(requisicao(&rota(e), Some(&token), consulta.clone()))
                .await
                .unwrap();
            assert!(r.status().is_success());
            quentes.push(t.elapsed());
        }
    }
    println!(
        "após {} consultas quentes ............... {}",
        quentes.len(),
        rss_mb()
    );

    let s = Arc::clone(&servidor);
    tokio::task::spawn_blocking(move || s.manutencao())
        .await
        .unwrap();
    println!(
        "todas despejadas ({} abertas) ............ {}",
        servidor.frota().abertas(),
        rss_mb()
    );

    for ciclo in 2..=3 {
        for &e in &empresas {
            let r = app
                .clone()
                .oneshot(requisicao(&rota(e), Some(&token), consulta.clone()))
                .await
                .unwrap();
            assert!(r.status().is_success());
        }
        println!(
            "ciclo {ciclo}: {} abertas ................. {}",
            servidor.frota().abertas(),
            rss_mb()
        );
        let s = Arc::clone(&servidor);
        tokio::task::spawn_blocking(move || s.manutencao())
            .await
            .unwrap();
        println!("ciclo {ciclo}: despejadas ................ {}", rss_mb());
    }

    let pct = |v: &mut Vec<Duration>, p: f64| {
        v.sort_unstable();
        v[((v.len() as f64 - 1.0) * p) as usize].as_micros()
    };
    println!();
    println!(
        "primeira requisição (empresa fria): p50 {:>6} µs  p99 {:>6} µs",
        pct(&mut frias, 0.5),
        pct(&mut frias, 0.99)
    );
    println!(
        "consulta quente (ponta a ponta):    p50 {:>6} µs  p99 {:>6} µs",
        pct(&mut quentes, 0.5),
        pct(&mut quentes, 0.99)
    );
}
