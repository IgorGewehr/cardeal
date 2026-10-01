//! De onde veio a requisição: o IP do cliente, para o limite de login.
//!
//! Atrás do Cloudflare Tunnel, o par TCP é o `cloudflared` (sempre o mesmo IP): o IP real vem
//! em `CF-Connecting-IP`. Esse cabeçalho só é confiável quando o servidor **só** é alcançável
//! pelo túnel — por isso é opção explícita (`confiar_cloudflare`), desligada por padrão.

use std::net::{IpAddr, SocketAddr};

use axum::http::HeaderMap;

const CF_CONNECTING_IP: &str = "cf-connecting-ip";

/// O IP do cliente, se for possível sabê-lo.
pub(super) fn ip_do_cliente(
    headers: &HeaderMap,
    par: Option<SocketAddr>,
    confiar_cloudflare: bool,
) -> Option<IpAddr> {
    if confiar_cloudflare {
        if let Some(ip) = headers
            .get(CF_CONNECTING_IP)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse().ok())
        {
            return Some(ip);
        }
    }
    par.map(|p| p.ip())
}

#[cfg(test)]
mod testes {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn cabecalho_do_cloudflare_so_vale_quando_configurado() {
        let mut h = HeaderMap::new();
        h.insert(CF_CONNECTING_IP, HeaderValue::from_static("203.0.113.7"));
        let par: SocketAddr = "127.0.0.1:5000".parse().unwrap();
        assert_eq!(ip_do_cliente(&h, Some(par), false), Some(par.ip()));
        assert_eq!(
            ip_do_cliente(&h, Some(par), true),
            "203.0.113.7".parse().ok()
        );
        assert_eq!(ip_do_cliente(&HeaderMap::new(), None, true), None);
    }
}
