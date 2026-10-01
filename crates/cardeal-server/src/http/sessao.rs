//! `POST /v1/sessao` (login) e `DELETE /v1/sessao` (logout).

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use cardeal_kernel::{CodigoErro, Erro};
use cardeal_protocol::{PedidoLogin, TipoCliente};

use super::credencial;
use super::resposta::{bloqueante, ler, postcard, ErroHttp};
use crate::{autenticacao, Servidor};

pub(super) async fn entrar(
    State(servidor): State<Arc<Servidor>>,
    corpo: Bytes,
) -> Result<Response, ErroHttp> {
    let pedido: PedidoLogin = ler(&corpo)?;
    // O semáforo limita quantos Argon2id (19 MiB cada) rodam ao mesmo tempo.
    let _licenca = servidor
        .logins
        .acquire()
        .await
        .map_err(|e| ErroHttp(Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string())))?;
    let s = Arc::clone(&servidor);
    let validade = servidor.config.validade_sessao;
    let (email, senha) = (pedido.email, pedido.senha);
    let aceito =
        bloqueante(move || autenticacao::entrar(&s.diretorio, &email, &senha, validade)).await?;

    let mut resposta = aceito.resposta;
    let mut headers = HeaderMap::new();
    match pedido.cliente {
        TipoCliente::Nativo => resposta.token = Some(aceito.token),
        _ => {
            headers.insert(
                header::SET_COOKIE,
                credencial::cookie(&aceito.token, validade.as_secs()),
            );
        }
    }
    Ok((headers, postcard(&resposta)?).into_response())
}

pub(super) async fn sair(
    State(servidor): State<Arc<Servidor>>,
    headers: HeaderMap,
) -> Result<Response, ErroHttp> {
    if let Some(token) = credencial::token(&headers) {
        servidor.sessoes.esquecer(&token);
        let s = Arc::clone(&servidor);
        bloqueante(move || autenticacao::sair(&s.diretorio, &token)).await?;
    }
    Ok((
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, credencial::cookie_apagado())],
    )
        .into_response())
}
