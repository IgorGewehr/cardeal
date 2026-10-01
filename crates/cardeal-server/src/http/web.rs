//! O cliente do navegador (`cardeal xtask construir-web` → `dist/web/`), servido pelo próprio
//! servidor: um processo, uma origem — o cookie de sessão é `SameSite=Strict` e o `fetch` do
//! app é sempre da mesma origem.
//!
//! - `/` → `index.html`, **sem cache**: é ele que aponta para a versão atual.
//! - `/app/<hash>/…` → arquivos versionados pelo conteúdo, **cache imutável de um ano**; o
//!   `.br`/`.gz` gerado no build é entregue pronto (zero compressão por requisição).

use std::path::Path;

use axum::extract::Request;
use axum::http::{header, HeaderValue};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::Router;
use tower_http::services::{ServeDir, ServeFile};

/// CSP da página: só o que vem da própria origem; `wasm-unsafe-eval` é o mínimo para
/// instanciar WebAssembly (não libera `eval` de JavaScript).
const CSP_PAGINA: &str = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; \
    style-src 'self'; img-src 'self' data: blob:; connect-src 'self'; \
    frame-ancestors 'none'; base-uri 'none'; form-action 'none'";

/// As rotas do cliente web sobre a pasta `dist/web`.
pub(super) fn rotas<S: Clone + Send + Sync + 'static>(pasta: &Path) -> Router<S> {
    let app = ServeDir::new(pasta.join("app"))
        .precompressed_br()
        .precompressed_gzip();
    let index = ServeFile::new(pasta.join("index.html"))
        .precompressed_br()
        .precompressed_gzip();
    Router::new()
        .nest_service("/app", app)
        .route_service("/", index)
        .layer(middleware::from_fn(cabecalhos))
}

async fn cabecalhos(req: Request, next: Next) -> Response {
    let versionado = req.uri().path().starts_with("/app/");
    let mut resp = next.run(req).await;
    let h = resp.headers_mut();
    if versionado {
        h.insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        );
    } else {
        h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
        h.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(CSP_PAGINA),
        );
    }
    resp
}
