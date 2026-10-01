//! A camada HTTP: só traduz requisição ↔ chamada. Nenhuma regra de negócio mora aqui.

mod camadas;
mod credencial;
mod despacho;
mod origem;
mod resposta;
mod sessao;

use std::sync::Arc;
use std::time::Duration;

use axum::extract::{DefaultBodyLimit, Request};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use cardeal_kernel::{CodigoErro, Erro};
use cardeal_protocol::{
    versao_aceita, CABECALHO_PROTOCOLO, ROTA_SAUDE, ROTA_SESSAO, TETO_CORPO_BYTES,
};
use tower_http::timeout::TimeoutLayer;

use crate::Servidor;
use resposta::ErroHttp;

/// Tempo máximo de uma requisição. Um comando real leva milissegundos; isto só corta o
/// patológico.
const TEMPO_MAXIMO: Duration = Duration::from_secs(30);

/// O roteador completo.
pub fn roteador(servidor: Arc<Servidor>) -> Router {
    let v1 = Router::new()
        .route(ROTA_SESSAO, post(sessao::entrar).delete(sessao::sair))
        .route("/v1/e/:empresa/cmd/:nome", post(despacho::comando))
        .route("/v1/e/:empresa/qry/:nome", post(despacho::consulta))
        .layer(middleware::from_fn(exigir_protocolo));
    Router::new()
        .route(ROTA_SAUDE, get(saude))
        .merge(v1)
        .layer(DefaultBodyLimit::max(TETO_CORPO_BYTES))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::GATEWAY_TIMEOUT,
            TEMPO_MAXIMO,
        ))
        .layer(middleware::from_fn(camadas::seguranca))
        .layer(middleware::from_fn(camadas::registro))
        .with_state(servidor)
}

async fn saude() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

/// Toda chamada `/v1` declara a versão do protocolo. Além de versionar, o cabeçalho próprio
/// força o preflight de CORS — um site de terceiros não dispara comando com o cookie do
/// usuário.
async fn exigir_protocolo(req: Request, next: Next) -> Response {
    let versao = req
        .headers()
        .get(CABECALHO_PROTOCOLO)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u16>().ok());
    match versao {
        Some(v) if versao_aceita(v) => next.run(req).await,
        Some(v) => ErroHttp(Erro::novo(
            CodigoErro::VERSAO_DESATUALIZADA,
            format!("protocolo {v} não é aceito por este servidor — atualize o Cardeal"),
        ))
        .into_response(),
        None => ErroHttp(Erro::novo(
            CodigoErro::ENTRADA_INVALIDA,
            format!("cabeçalho {CABECALHO_PROTOCOLO} ausente"),
        ))
        .into_response(),
    }
}
