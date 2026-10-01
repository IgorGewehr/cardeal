//! Camadas que valem para toda resposta: cabeçalhos de segurança e o registro de requisições.

use std::time::{Duration, Instant};

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::metricas::Tipo;
use crate::Servidor;

/// Acima disto, uma requisição bem-sucedida também vai para o log (como aviso).
const LENTA: Duration = Duration::from_millis(250);

/// Cabeçalhos de segurança. A API só fala `postcard` — nenhuma resposta dela é página: o CSP
/// nega tudo e o `no-store` mantém dado de empresa fora de qualquer cache intermediário.
pub(super) async fn seguranca(req: Request, next: Next) -> Response {
    let api = req.uri().path().starts_with("/v1/");
    let mut resp = next.run(req).await;
    let h = resp.headers_mut();
    h.insert(
        header::STRICT_TRANSPORT_SECURITY,
        HeaderValue::from_static("max-age=63072000; includeSubDomains"),
    );
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    if api {
        h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        h.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("default-src 'none'; frame-ancestors 'none'"),
        );
    }
    resp
}

/// Registro de requisições: alimenta as métricas e o log — falha (5xx) e lentidão sobem como
/// aviso; o resto fica em `debug` (log não pode virar custo de CPU/disco no caminho quente).
pub(super) async fn registro(
    State(servidor): State<Arc<Servidor>>,
    req: Request,
    next: Next,
) -> Response {
    let inicio = Instant::now();
    let metodo = req.method().clone();
    let caminho = req.uri().path().to_owned();
    let resp = next.run(req).await;
    let duracao = inicio.elapsed();
    let status = resp.status().as_u16();
    servidor
        .metricas
        .registrar(Tipo::de_caminho(&caminho), status, duracao);
    let ms = duracao.as_secs_f64() * 1000.0;
    if status >= 500 || duracao >= LENTA {
        tracing::warn!(%metodo, %caminho, status, ms, "requisição");
    } else {
        tracing::debug!(%metodo, %caminho, status, ms, "requisição");
    }
    resp
}

/// `GET /metricas` — texto do Prometheus, com `Authorization: Bearer <token>`.
pub(super) async fn metricas(
    State(servidor): State<Arc<Servidor>>,
    headers: HeaderMap,
) -> Response {
    let Some(esperado) = servidor.config.token_metricas.as_deref() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let recebido = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or_default();
    // Compara os hashes: o tempo da comparação não revela quanto do token acertou.
    if blake3::hash(recebido.as_bytes()) != blake3::hash(esperado.as_bytes()) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let s = Arc::clone(&servidor);
    let texto = tokio::task::spawn_blocking(move || {
        s.metricas.exportar(s.frota.abertas(), s.sessoes.em_cache())
    })
    .await
    .unwrap_or_default();
    (
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; version=0.0.4"),
        )],
        texto,
    )
        .into_response()
}
