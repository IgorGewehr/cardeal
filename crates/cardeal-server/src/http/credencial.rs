//! De onde vem o token (cookie do navegador ou `Authorization: Bearer` do nativo) e como o
//! cookie é montado.

use axum::http::{header, HeaderMap, HeaderValue};
use cardeal_kernel::{CodigoErro, Erro};
use cardeal_protocol::COOKIE_SESSAO;

use super::resposta::ErroHttp;

/// O token da requisição, se houver.
pub(super) fn token(headers: &HeaderMap) -> Option<String> {
    if let Some(bearer) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    {
        return Some(bearer.trim().to_owned());
    }
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|par| par.trim().split_once('='))
        .find(|(nome, _)| *nome == COOKIE_SESSAO)
        .map(|(_, valor)| valor.to_owned())
}

/// O token, ou 401.
pub(super) fn exigir_token(headers: &HeaderMap) -> Result<String, ErroHttp> {
    token(headers).ok_or_else(|| {
        ErroHttp(Erro::novo(
            CodigoErro::SESSAO_INVALIDA,
            "faça login para continuar",
        ))
    })
}

/// O `Set-Cookie` da sessão: inacessível a script, só por HTTPS, nunca enviado por outro site.
pub(super) fn cookie(token: &str, max_age_segundos: u64) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{COOKIE_SESSAO}={token}; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age={max_age_segundos}"
    ))
    .unwrap_or_else(|_| HeaderValue::from_static(""))
}

/// O `Set-Cookie` que apaga a sessão no navegador.
pub(super) fn cookie_apagado() -> HeaderValue {
    cookie("", 0)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn bearer_tem_precedencia_e_cookie_e_lido_entre_outros() {
        let mut h = HeaderMap::new();
        h.insert(
            header::COOKIE,
            HeaderValue::from_static("tema=escuro; cardeal_sessao=abc; x=1"),
        );
        assert_eq!(token(&h).as_deref(), Some("abc"));
        h.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer xyz"),
        );
        assert_eq!(token(&h).as_deref(), Some("xyz"));
    }
}
