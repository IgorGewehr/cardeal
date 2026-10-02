//! `POST /v1/sessao` (login) e `DELETE /v1/sessao` (logout).

use std::sync::Arc;

use axum::body::Bytes;
use std::net::SocketAddr;

use axum::extract::{ConnectInfo, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use cardeal_kernel::{CodigoErro, Erro};
use cardeal_protocol::{PedidoLogin, TipoCliente};

use super::resposta::{bloqueante, ler, postcard, ErroHttp};
use super::{credencial, origem};
use crate::{autenticacao, Servidor};

pub(super) async fn entrar(
    State(servidor): State<Arc<Servidor>>,
    par: Option<ConnectInfo<SocketAddr>>,
    headers: HeaderMap,
    corpo: Bytes,
) -> Result<Response, ErroHttp> {
    let ip = origem::ip_do_cliente(
        &headers,
        par.map(|ConnectInfo(p)| p),
        servidor.config.confiar_cloudflare,
    );
    if let Some(ip) = ip {
        if !servidor.limitador.permitir(ip) {
            servidor.metricas.limite_ip_atingido();
            tracing::warn!(%ip, "limite de login por IP atingido");
            return Err(ErroHttp(Erro::novo(
                CodigoErro::MUITAS_TENTATIVAS,
                "tentativas de login demais a partir desta rede — aguarde alguns minutos",
            )));
        }
    }
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
    let aceito = bloqueante(move || autenticacao::entrar(&s.diretorio, &email, &senha, validade))
        .await
        .inspect_err(|_| servidor.metricas.login_recusado())?;

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

/// `GET /v1/sessao`: a sessão do cookie/token ainda vale? Devolve o mesmo que o login (sem o
/// token) — é o que deixa o navegador recarregar a página sem pedir a senha de novo.
pub(super) async fn atual(
    State(servidor): State<Arc<Servidor>>,
    headers: HeaderMap,
) -> Result<Response, ErroHttp> {
    let token = credencial::exigir_token(&headers)?;
    let s = Arc::clone(&servidor);
    let resposta = bloqueante(move || {
        let sessao = s.sessoes.resolver(&s.diretorio, &token)?;
        let nome = s.diretorio.nome_da_conta(sessao.conta)?.unwrap_or_default();
        let empresas = s
            .diretorio
            .vinculos(sessao.conta)?
            .into_iter()
            .map(|v| cardeal_protocol::EmpresaAcessivel {
                id: v.empresa,
                nome: v.nome,
            })
            .collect();
        Ok(cardeal_protocol::RespostaLogin {
            nome,
            empresas,
            token: None,
        })
    })
    .await?;
    Ok(postcard(&resposta)?.into_response())
}

/// `POST /v1/sessao/senha`.
pub(super) async fn trocar_senha(
    State(servidor): State<Arc<Servidor>>,
    headers: HeaderMap,
    corpo: Bytes,
) -> Result<Response, ErroHttp> {
    let token = credencial::exigir_token(&headers)?;
    let pedido: cardeal_protocol::PedidoTrocaSenha = ler(&corpo)?;
    // Dois Argon2id: o mesmo semáforo do login segura o pico de memória.
    let _licenca = servidor
        .logins
        .acquire()
        .await
        .map_err(|e| ErroHttp(Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string())))?;
    let s = Arc::clone(&servidor);
    bloqueante(move || {
        let sessao = s.sessoes.resolver(&s.diretorio, &token)?;
        let encerradas =
            autenticacao::trocar_senha(&s.diretorio, sessao.conta, &token, &pedido.atual, &pedido.nova)?;
        s.sessoes.esquecer_hashes(&encerradas);
        tracing::info!(conta = %sessao.conta, outras_sessoes_encerradas = encerradas.len(), "senha trocada");
        Ok(())
    })
    .await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}
