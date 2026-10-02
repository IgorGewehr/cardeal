//! A camada HTTP: só traduz requisição ↔ chamada. Nenhuma regra de negócio mora aqui.

mod camadas;
mod credencial;
mod despacho;
mod eventos;
mod membros;
mod organizacao;
mod origem;
mod resposta;
mod sessao;
mod web;

use std::sync::Arc;
use std::time::Duration;

use axum::extract::{DefaultBodyLimit, Request};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::Router;
use cardeal_kernel::{CodigoErro, Erro};
use cardeal_protocol::{
    versao_aceita, CABECALHO_PROTOCOLO, ROTA_SAUDE, ROTA_SENHA, ROTA_SESSAO, TETO_CORPO_BYTES,
};
use tower_http::compression::predicate::{NotForContentType, Predicate as _, SizeAbove};
use tower_http::compression::{CompressionLayer, CompressionLevel};
use tower_http::timeout::TimeoutLayer;

use crate::Servidor;
use resposta::ErroHttp;

/// Tempo máximo de uma requisição. Um comando real leva milissegundos; isto só corta o
/// patológico.
const TEMPO_MAXIMO: Duration = Duration::from_secs(30);

/// O roteador completo.
pub fn roteador(servidor: Arc<Servidor>) -> Router {
    let v1 = Router::new()
        .route(
            ROTA_SESSAO,
            post(sessao::entrar).get(sessao::atual).delete(sessao::sair),
        )
        .route(ROTA_SENHA, post(sessao::trocar_senha))
        .route("/v1/e/:empresa/cmd/:nome", post(despacho::comando))
        .route("/v1/e/:empresa/qry/:nome", post(despacho::consulta))
        .route("/v1/e/:empresa/sessao", get(despacho::sessao))
        .route("/v1/e/:empresa/membros", post(membros::adicionar))
        .route("/v1/e/:empresa/membros/:usuario", delete(membros::remover))
        .route("/v1/e/:empresa/empresas", post(organizacao::adicionar))
        .layer(middleware::from_fn(exigir_protocolo))
        // Fora do `exigir_protocolo`: o `EventSource` não manda cabeçalho próprio (a versão
        // vem em `?v=`), e um GET sem efeito não precisa da defesa de CORS do cabeçalho.
        .route("/v1/e/:empresa/eventos", get(eventos::eventos));
    let mut app = Router::new()
        .route(ROTA_SAUDE, get(saude))
        .route("/metricas", get(camadas::metricas))
        .merge(v1);
    if let Some(pasta) = servidor.config.web.clone() {
        app = app.merge(web::rotas(&pasta));
    }
    // Respostas da API acima de 1 KB (listas) saem comprimidas no nível rápido — o Cloudflare
    // não comprime `postcard`. Estáticos já vêm prontos (`.br`/`.gz`) e a camada pula o que
    // já tem `Content-Encoding`.
    let compressao = CompressionLayer::new()
        .quality(CompressionLevel::Fastest)
        .compress_when(
            SizeAbove::new(1024)
                .and(NotForContentType::IMAGES)
                // SSE comprimido ficaria preso no buffer do compressor até encher.
                .and(NotForContentType::const_new("text/event-stream")),
        );
    app.layer(compressao)
        .layer(DefaultBodyLimit::max(TETO_CORPO_BYTES))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::GATEWAY_TIMEOUT,
            TEMPO_MAXIMO,
        ))
        .layer(middleware::from_fn(camadas::seguranca))
        .layer(middleware::from_fn_with_state(
            Arc::clone(&servidor),
            camadas::registro,
        ))
        .with_state(servidor)
}

/// Sempre 200 enquanto o servidor atende — reiniciar o contêiner não conserta um disco de
/// réplica cheio. O corpo diz se a replicação está em falha (`degradado`), para um monitor
/// externo com palavra-chave alertar; os detalhes ficam nas métricas e no log.
async fn saude() -> impl IntoResponse {
    if cardeal_storage::saude_replicacao().bases_em_falha > 0 {
        (StatusCode::OK, "degradado")
    } else {
        (StatusCode::OK, "ok")
    }
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
