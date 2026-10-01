//! Corpo `postcard` de ida e volta, erro como `cardeal_kernel::Erro`, e a ponte para o trabalho
//! bloqueante (SQLite, Argon2id) fora das threads do runtime.

use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use cardeal_kernel::{CodigoErro, Erro, Id, Resultado};
use cardeal_protocol::{status_http, TIPO_POSTCARD};
use serde::de::DeserializeOwned;
use serde::Serialize;

/// Bytes já em `postcard`.
pub(super) struct Postcard(pub Vec<u8>);

impl IntoResponse for Postcard {
    fn into_response(self) -> Response {
        (
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static(TIPO_POSTCARD),
            )],
            self.0,
        )
            .into_response()
    }
}

/// Um valor a serializar em `postcard`.
pub(super) fn postcard<T: Serialize>(valor: &T) -> Result<Postcard, ErroHttp> {
    postcard::to_stdvec(valor)
        .map(Postcard)
        .map_err(|e| ErroHttp(Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string())))
}

/// Um corpo `postcard` a desserializar.
pub(super) fn ler<T: DeserializeOwned>(corpo: &[u8]) -> Result<T, ErroHttp> {
    postcard::from_bytes(corpo).map_err(|e| {
        ErroHttp(Erro::novo(
            CodigoErro::ENTRADA_INVALIDA,
            format!("corpo inválido: {e}"),
        ))
    })
}

/// O erro na resposta: status pela faixa do código, corpo = o `Erro` em `postcard`.
pub(super) struct ErroHttp(pub Erro);

impl From<Erro> for ErroHttp {
    fn from(e: Erro) -> Self {
        Self(e)
    }
}

impl IntoResponse for ErroHttp {
    fn into_response(self) -> Response {
        let mut erro = self.0;
        let status = status_http(erro.codigo);
        // Falha interna nunca vaza detalhe técnico (SQL, caminho de arquivo) para fora:
        // vai inteira para o log, e o cliente recebe a referência para o suporte.
        if status >= 500 {
            let referencia = Id::novo();
            tracing::error!(%referencia, codigo = erro.codigo.0, mensagem = %erro.mensagem, "falha");
            erro = Erro::novo(
                erro.codigo,
                format!("falha no servidor — informe ao suporte a referência {referencia}"),
            );
        }
        let corpo = postcard::to_stdvec(&erro).unwrap_or_default();
        (
            StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static(TIPO_POSTCARD),
            )],
            corpo,
        )
            .into_response()
    }
}

/// Roda trabalho bloqueante no pool de bloqueio do tokio.
pub(super) async fn bloqueante<T, F>(f: F) -> Result<T, ErroHttp>
where
    F: FnOnce() -> Resultado<T> + Send + 'static,
    T: Send + 'static,
{
    match tokio::task::spawn_blocking(f).await {
        Ok(r) => r.map_err(ErroHttp),
        Err(e) => Err(ErroHttp(Erro::novo(
            CodigoErro::FALHA_INTERNA,
            e.to_string(),
        ))),
    }
}
