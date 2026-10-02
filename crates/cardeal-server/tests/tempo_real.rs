//! O tempo real de ponta a ponta: o comando de um usuário acorda o fluxo SSE dos outros da
//! mesma empresa, nunca de outra; quem reconecta recebe o que perdeu; quem perdeu o fio é
//! mandado recarregar.

mod comum;

use std::time::Duration;

use axum::body::{Body, BodyDataStream};
use axum::http::{header, Request, StatusCode};
use axum::Router;
use cardeal_kernel::Id;
use cardeal_protocol::{modulos_alterados, rota_eventos, EVENTO_MUDOU, EVENTO_RECARREGAR};
use comum::*;
use futures_util::StreamExt as _;
use tower::ServiceExt as _;

/// Um bloco SSE já separado: (id, evento, data).
#[derive(Debug, Default)]
struct Bloco {
    id: Option<u64>,
    evento: Option<String>,
    data: String,
}

struct Fluxo {
    corpo: BodyDataStream,
    pendente: String,
}

impl Fluxo {
    /// O próximo bloco, ou `None` se nada chegar em `espera`.
    async fn proximo(&mut self, espera: Duration) -> Option<Bloco> {
        loop {
            if let Some(fim) = self.pendente.find("\n\n") {
                let bruto: String = self.pendente.drain(..fim + 2).collect();
                let mut b = Bloco::default();
                for linha in bruto.lines() {
                    if let Some(v) = linha.strip_prefix("id:") {
                        b.id = v.trim().parse().ok();
                    } else if let Some(v) = linha.strip_prefix("event:") {
                        b.evento = Some(v.trim().to_owned());
                    } else if let Some(v) = linha.strip_prefix("data:") {
                        b.data.push_str(v.trim());
                    }
                }
                return Some(b);
            }
            let pedaco = tokio::time::timeout(espera, self.corpo.next())
                .await
                .ok()??;
            self.pendente
                .push_str(std::str::from_utf8(&pedaco.unwrap()).unwrap());
        }
    }

    /// O próximo evento com nome (pula comentários e batimentos).
    async fn evento(&mut self, espera: Duration) -> Option<Bloco> {
        loop {
            let b = self.proximo(espera).await?;
            if b.evento.is_some() {
                return Some(b);
            }
        }
    }
}

async fn abrir(app: &Router, token: &str, empresa: Id, ultimo: Option<u64>) -> (StatusCode, Fluxo) {
    let mut req = Request::get(rota_eventos(empresa))
        .header(header::AUTHORIZATION, format!("Bearer {token}"));
    if let Some(u) = ultimo {
        req = req.header("last-event-id", u.to_string());
    }
    let resp = app
        .clone()
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    if status == StatusCode::OK {
        assert_eq!(resp.headers()[header::CONTENT_TYPE], "text/event-stream");
        assert!(
            resp.headers().get(header::CONTENT_ENCODING).is_none(),
            "SSE nunca comprimido"
        );
    }
    (
        status,
        Fluxo {
            corpo: resp.into_body().into_data_stream(),
            pendente: String::new(),
        },
    )
}

const CURTO: Duration = Duration::from_millis(300);
const LONGO: Duration = Duration::from_secs(5);

async fn criar(app: &Router, token: &str, empresa: Id, nome: &str) {
    let r = comando(
        app,
        token,
        empresa,
        "clientes.criar_pessoa.v1",
        &criar_pessoa(nome),
        Id::novo(),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn comando_acorda_so_a_propria_empresa_e_quem_volta_recebe_o_que_perdeu() {
    let a = ambiente(Duration::from_secs(600));
    let ana = token(&a.app, "ana@x.com").await;
    let beto = token(&a.app, "beto@x.com").await;

    let (s, mut fluxo_ana) = abrir(&a.app, &ana, a.empresa_ana, None).await;
    assert_eq!(s, StatusCode::OK);
    let pronto = fluxo_ana
        .proximo(LONGO)
        .await
        .expect("o fluxo anuncia o ponto de partida");
    let inicio = pronto.id.expect("com id");
    let (_, mut fluxo_beto) = abrir(&a.app, &beto, a.empresa_beto, None).await;
    fluxo_beto.proximo(LONGO).await.unwrap();

    criar(&a.app, &ana, a.empresa_ana, "Carla").await;
    let e = fluxo_ana
        .evento(LONGO)
        .await
        .expect("o comando acorda o fluxo");
    assert_eq!(e.evento.as_deref(), Some(EVENTO_MUDOU));
    assert!(modulos_alterados(&e.data).any(|m| m == "clientes"), "{e:?}");
    let visto = e.id.unwrap();
    assert!(visto > inicio);
    assert!(
        fluxo_beto.evento(CURTO).await.is_none(),
        "outra empresa não fica sabendo de nada"
    );

    // Desconecta; dois comandos acontecem; ao voltar com Last-Event-ID, chega um evento só.
    drop(fluxo_ana);
    criar(&a.app, &ana, a.empresa_ana, "Davi").await;
    criar(&a.app, &ana, a.empresa_ana, "Edu").await;
    let (_, mut de_volta) = abrir(&a.app, &ana, a.empresa_ana, Some(visto)).await;
    let e = de_volta
        .evento(LONGO)
        .await
        .expect("o que perdeu chega ao reconectar");
    assert_eq!(e.evento.as_deref(), Some(EVENTO_MUDOU));
    assert!(e.id.unwrap() > visto);
    assert!(
        de_volta.evento(CURTO).await.is_none(),
        "uma rajada vira um evento"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ponto_desconhecido_manda_recarregar_e_entrada_invalida_e_recusada() {
    let a = ambiente(Duration::from_secs(600));
    let ana = token(&a.app, "ana@x.com").await;

    let (_, mut f) = abrir(&a.app, &ana, a.empresa_ana, Some(9_999_999)).await;
    let e = f.evento(LONGO).await.unwrap();
    assert_eq!(e.evento.as_deref(), Some(EVENTO_RECARREGAR));

    // Sem sessão; empresa alheia; sem versão do protocolo.
    assert_eq!(
        abrir(&a.app, "lixo", a.empresa_ana, None).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        abrir(&a.app, &ana, a.empresa_beto, None).await.0,
        StatusCode::FORBIDDEN
    );
    let sem_versao = a
        .app
        .clone()
        .oneshot(
            Request::get(format!("/v1/e/{}/eventos", a.empresa_ana))
                .header(header::AUTHORIZATION, format!("Bearer {ana}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(sem_versao.status(), StatusCode::UNPROCESSABLE_ENTITY);
}
