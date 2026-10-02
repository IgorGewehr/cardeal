//! O transporte do navegador: `fetch` assíncrono, mesma origem (o cookie `HttpOnly` vai
//! sozinho; nenhum token passa pelo JavaScript). O protocolo — rotas, cabeçalhos, leitura do
//! `Erro` — é o mesmo do desktop (`cardeal_cliente::remoto::protocolo`).

use std::sync::mpsc;

use cardeal_cliente::remoto::protocolo::{self, Metodo, Pedido, Resposta};
use cardeal_cliente::remoto::TENTATIVAS;
use cardeal_kernel::{CodigoErro, Erro, Resultado};
use js_sys::Uint8Array;
use serde::de::DeserializeOwned;
use wasm_bindgen::{JsCast as _, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{RequestCredentials, RequestInit};

fn js(e: &JsValue) -> String {
    e.as_string().unwrap_or_else(|| format!("{e:?}"))
}

async fn fetch(pedido: &Pedido) -> Result<Resposta, String> {
    let init = RequestInit::new();
    init.set_method(match pedido.metodo {
        Metodo::Get => "GET",
        Metodo::Post => "POST",
        Metodo::Delete => "DELETE",
    });
    init.set_credentials(RequestCredentials::SameOrigin);
    if !pedido.corpo.is_empty() {
        init.set_body(&Uint8Array::from(pedido.corpo.as_slice()));
    }
    let req =
        web_sys::Request::new_with_str_and_init(&pedido.caminho, &init).map_err(|e| js(&e))?;
    for (k, v) in &pedido.cabecalhos {
        req.headers().set(k, v).map_err(|e| js(&e))?;
    }
    let janela = web_sys::window().ok_or("sem window")?;
    let resp: web_sys::Response = JsFuture::from(janela.fetch_with_request(&req))
        .await
        .map_err(|e| js(&e))?
        .dyn_into()
        .map_err(|e| js(&e))?;
    let buf = JsFuture::from(resp.array_buffer().map_err(|e| js(&e))?)
        .await
        .map_err(|e| js(&e))?;
    Ok(Resposta {
        status: resp.status(),
        corpo: Uint8Array::new(&buf).to_vec(),
    })
}

/// Espera sem travar o quadro (um `setTimeout` como `Promise`).
async fn esperar(ms: i32) {
    let promessa = js_sys::Promise::new(&mut |resolver, _| {
        if let Some(janela) = web_sys::window() {
            let _ = janela.set_timeout_with_callback_and_timeout_and_arguments_0(&resolver, ms);
        }
    });
    let _ = JsFuture::from(promessa).await;
}

/// Envia, reenviando numa falha de rede como o desktop: até [`TENTATIVAS`], 200 ms e 600 ms
/// de recuo. É seguro para comando porque toda tentativa leva o **mesmo** pedido — a mesma
/// chave de idempotência; se só a resposta se perdeu, o servidor devolve a original.
async fn enviar(pedido: &Pedido) -> Resultado<Resposta> {
    let tentativas = if pedido.reenviavel { TENTATIVAS } else { 1 };
    let mut ultima = String::new();
    for tentativa in 0..tentativas {
        if tentativa > 0 {
            esperar(200 * 3_i32.pow(tentativa - 1)).await;
        }
        match fetch(pedido).await {
            Ok(r) => return Ok(r),
            Err(e) => ultima = e,
        }
    }
    Err(Erro::novo(
        CodigoErro::SEM_CONEXAO,
        format!("sem conexão com o servidor — verifique a internet ({ultima})"),
    ))
}

/// Envia e interpreta. Falha de rede vira `SEM_CONEXAO` — a mesma mensagem do desktop.
pub async fn pedir<T: DeserializeOwned>(pedido: Pedido) -> Resultado<T> {
    protocolo::interpretar(&enviar(&pedido).await?)
}

/// Um resultado que ainda vai chegar. A tela consulta [`Pendente::pronto`] a cada quadro; a
/// chegada pede um quadro novo (`request_repaint`) — sem polling, sem quadro desperdiçado.
pub struct Pendente<T>(mpsc::Receiver<Resultado<T>>);

impl<T> Pendente<T> {
    /// O resultado, se já chegou.
    pub fn pronto(&self) -> Option<Resultado<T>> {
        self.0.try_recv().ok()
    }
}

/// Dispara o pedido em segundo plano.
pub fn disparar<T: DeserializeOwned + 'static>(ctx: &egui::Context, pedido: Pedido) -> Pendente<T> {
    let (tx, rx) = mpsc::channel();
    let ctx = ctx.clone();
    wasm_bindgen_futures::spawn_local(async move {
        let _ = tx.send(pedir(pedido).await);
        ctx.request_repaint();
    });
    Pendente(rx)
}

/// Como [`disparar`], para respostas sem corpo (`204`, o logout).
pub fn disparar_sem_corpo(ctx: &egui::Context, pedido: Pedido) -> Pendente<()> {
    let (tx, rx) = mpsc::channel();
    let ctx = ctx.clone();
    wasm_bindgen_futures::spawn_local(async move {
        let r = match enviar(&pedido).await {
            Ok(resposta) => protocolo::interpretar_vazio(&resposta),
            Err(e) => Err(e),
        };
        let _ = tx.send(r);
        ctx.request_repaint();
    });
    Pendente(rx)
}
