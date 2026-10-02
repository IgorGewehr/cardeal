//! `GET /v1/e/{empresa}/eventos` — o fluxo de alterações (SSE). Ver [`crate::tempo_real`].
//!
//! Cada conexão é uma tarefa que dorme até o sinal da empresa tocar (ou o batimento), relê o
//! outbox do seu último `seq` e manda **quais módulos** mudaram — nunca os dados: a tela
//! recarrega pela consulta de sempre, com a autorização de sempre. Só aparecem módulos em
//! que o usuário tem alguma permissão.

use std::collections::BTreeSet;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue};
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Response};
use cardeal_kernel::{CodigoErro, Erro, Id, Resultado};
use cardeal_protocol::{versao_aceita, EVENTO_MUDOU, EVENTO_RECARREGAR, EVENTO_SESSAO_ENCERRADA};
use serde::Deserialize;
use tokio::sync::{mpsc, watch};

use super::credencial::exigir_token;
use super::despacho::{empresa_da_rota, entrar_na_empresa};
use super::resposta::{bloqueante, ErroHttp};
use crate::tempo_real::Vaga;
use crate::Servidor;

/// De quanto em quanto tempo a conexão parada dá sinal de vida (e revalida a sessão). Abaixo
/// dos 100 s em que o Cloudflare derruba uma resposta muda.
const BATIMENTO: Duration = Duration::from_secs(25);

/// Quantas alterações lidas por ida ao banco.
const LOTE: usize = 1000;

#[derive(Deserialize)]
pub(super) struct Parametros {
    v: Option<u16>,
    desde: Option<u64>,
}

pub(super) async fn eventos(
    State(servidor): State<Arc<Servidor>>,
    Path(empresa): Path<String>,
    Query(p): Query<Parametros>,
    headers: HeaderMap,
) -> Result<Response, ErroHttp> {
    match p.v {
        Some(v) if versao_aceita(v) => {}
        Some(v) => {
            return Err(ErroHttp(Erro::novo(
                CodigoErro::VERSAO_DESATUALIZADA,
                format!("protocolo {v} não é aceito por este servidor — atualize o Cardeal"),
            )))
        }
        None => {
            return Err(ErroHttp(Erro::novo(
                CodigoErro::ENTRADA_INVALIDA,
                "informe a versão do protocolo (?v=)",
            )))
        }
    }
    let token = exigir_token(&headers)?;
    let empresa = empresa_da_rota(&empresa)?;
    // O navegador devolve `Last-Event-ID` sozinho ao reconectar; `?desde=` é para o primeiro.
    let desde = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
        .or(p.desde);
    let vaga = servidor.tempo_real.reservar().ok_or_else(|| {
        ErroHttp(Erro::novo(
            CodigoErro::LIMITE_EXCEDIDO,
            "servidor no limite de conexões de tempo real — tente de novo em instantes",
        ))
    })?;
    // Assina **antes** de ler o ponto de partida: o que for gravado entre uma coisa e outra
    // acorda a conexão em vez de se perder.
    let sinal = servidor.tempo_real.assinar(empresa);
    let (s, t) = (Arc::clone(&servidor), token.clone());
    let base = bloqueante(move || {
        let (aberta, _) = entrar_na_empresa(&s, &t, empresa)?;
        aberta.motor().ultima_alteracao()
    })
    .await?;

    let (tx, rx) = mpsc::channel::<Event>(8);
    let conexao = Conexao {
        servidor,
        token,
        empresa,
        tx,
        _vaga: vaga,
    };
    tokio::spawn(conexao.acompanhar(sinal, desde, base));

    let fluxo = futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|e| (Ok::<_, Infallible>(e), rx))
    });
    let mut resposta = Sse::new(fluxo).into_response();
    let h = resposta.headers_mut();
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache, no-transform"),
    );
    h.insert("x-accel-buffering", HeaderValue::from_static("no"));
    Ok(resposta)
}

struct Conexao {
    servidor: Arc<Servidor>,
    token: String,
    empresa: Id,
    tx: mpsc::Sender<Event>,
    _vaga: Vaga,
}

/// O que uma leitura do outbox rendeu.
struct Lido {
    eventos: Vec<Event>,
    ultimo: u64,
}

fn encerra_a_conexao(e: &Erro) -> bool {
    e.codigo == CodigoErro::SESSAO_INVALIDA || e.codigo == CodigoErro::SEM_PERMISSAO
}

impl Conexao {
    async fn acompanhar(self, mut sinal: watch::Receiver<u64>, desde: Option<u64>, base: u64) {
        let mut ultimo = desde.unwrap_or(base);
        let mut ler = desde.is_some_and(|d| d != base);
        if !ler
            && self
                .enviar(Event::default().id(ultimo.to_string()).comment("pronto"))
                .await
                .is_err()
        {
            return;
        }
        let mut batida =
            tokio::time::interval_at(tokio::time::Instant::now() + BATIMENTO, BATIMENTO);
        loop {
            if std::mem::take(&mut ler) {
                match self.ler(ultimo).await {
                    Ok(lido) => {
                        ultimo = lido.ultimo;
                        for e in lido.eventos {
                            if self.enviar(e).await.is_err() {
                                return;
                            }
                        }
                    }
                    Err(e) if encerra_a_conexao(&e.0) => return self.encerrar().await,
                    // Falha passageira (base fechando, disco): o próximo despertar relê.
                    Err(e) => {
                        tracing::warn!(empresa = %self.empresa, erro = %e.0.mensagem, "leitura de alterações falhou");
                    }
                }
            }
            tokio::select! {
                () = self.tx.closed() => return,
                mudou = sinal.changed() => {
                    if mudou.is_err() {
                        return;
                    }
                    ler = true;
                }
                _ = batida.tick() => {
                    if let Err(e) = self.revalidar().await {
                        if encerra_a_conexao(&e.0) {
                            return self.encerrar().await;
                        }
                    }
                    if self.enviar(Event::default().comment("")).await.is_err() {
                        return;
                    }
                    // Rede de segurança: relê mesmo sem sinal (barato quando não há nada).
                    ler = true;
                }
            }
        }
    }

    async fn enviar(&self, e: Event) -> Result<(), ()> {
        self.tx.send(e).await.map_err(|_| ())
    }

    async fn encerrar(&self) {
        let _ = self
            .enviar(Event::default().event(EVENTO_SESSAO_ENCERRADA).data("1"))
            .await;
    }

    /// A sessão ainda vale e ainda tem acesso à empresa — **sem** abrir a empresa: uma
    /// conexão parada não pode segurar uma base aberta na memória.
    async fn revalidar(&self) -> Result<(), ErroHttp> {
        let (s, t, empresa) = (Arc::clone(&self.servidor), self.token.clone(), self.empresa);
        bloqueante(move || {
            s.sessoes
                .resolver(&s.diretorio, &t)?
                .usuario_em(empresa)
                .map(|_| ())
        })
        .await
    }

    async fn ler(&self, desde: u64) -> Result<Lido, ErroHttp> {
        let (s, t, empresa) = (Arc::clone(&self.servidor), self.token.clone(), self.empresa);
        bloqueante(move || ler_alteracoes(&s, &t, empresa, desde)).await
    }
}

/// Bloqueante: entra na empresa (as permissões vêm frescas a cada leitura) e transforma o
/// outbox depois de `desde` em eventos.
fn ler_alteracoes(servidor: &Servidor, token: &str, empresa: Id, desde: u64) -> Resultado<Lido> {
    let (aberta, sessao) = entrar_na_empresa(servidor, token, empresa)?;
    let permitidos: BTreeSet<&str> = sessao
        .permissoes()
        .filter_map(|p| p.split_once('.').map(|(m, _)| m))
        .collect();
    let mut ultimo = desde;
    let mut modulos = BTreeSet::new();
    loop {
        let a = aberta.motor().alteracoes_desde(empresa, ultimo, LOTE)?;
        if a.lacuna {
            let e = Event::default()
                .id(a.ultimo.to_string())
                .event(EVENTO_RECARREGAR)
                .data("1");
            return Ok(Lido {
                eventos: vec![e],
                ultimo: a.ultimo,
            });
        }
        let n = a.itens.len();
        for (seq, tipo) in a.itens {
            ultimo = seq;
            if let Some(m) = tipo.split('.').next().filter(|m| permitidos.contains(m)) {
                modulos.insert(m.to_owned());
            }
        }
        if n < LOTE {
            break;
        }
    }
    let mut eventos = Vec::new();
    if ultimo != desde {
        let e = Event::default().id(ultimo.to_string());
        eventos.push(if modulos.is_empty() {
            // Nada visível a este usuário, mas o ponto avança.
            e.comment("")
        } else {
            e.event(EVENTO_MUDOU)
                .data(modulos.into_iter().collect::<Vec<_>>().join(","))
        });
    }
    Ok(Lido { eventos, ultimo })
}
