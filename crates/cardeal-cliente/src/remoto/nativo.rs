//! O transporte do desktop: HTTP(S) síncrono com `reqwest`, conexão reaproveitada (keep-alive),
//! TLS pelo `rustls` (sem OpenSSL), resposta comprimida.

use std::time::Duration;

use reqwest::blocking::Client;

use super::protocolo::{Metodo, Pedido, Resposta};
use super::{Fluxo, Transporte};

/// HTTP contra uma base (`https://app.cardeal.com.br`).
pub struct TransporteHttp {
    base: String,
    cliente: Client,
    /// Para o fluxo de tempo real: sem prazo total (a resposta não termina); uma conexão
    /// morta em silêncio é descoberta pelo keepalive do TCP.
    fluxo: Client,
}

impl TransporteHttp {
    /// Um transporte para a base dada (sem barra no fim).
    ///
    /// # Errors
    /// Falha ao montar o cliente HTTP (TLS indisponível).
    pub fn novo(base: &str) -> Result<Self, String> {
        let cliente = Client::builder()
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(5))
            .pool_idle_timeout(Duration::from_secs(90))
            .user_agent(concat!("cardeal-desktop/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| e.to_string())?;
        let fluxo = Client::builder()
            .timeout(None)
            .connect_timeout(Duration::from_secs(5))
            .tcp_keepalive(Duration::from_secs(20))
            .tcp_keepalive_interval(Duration::from_secs(10))
            .tcp_keepalive_retries(3)
            .user_agent(concat!("cardeal-desktop/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            base: base.trim_end_matches('/').to_owned(),
            cliente,
            fluxo,
        })
    }
}

impl Transporte for TransporteHttp {
    fn enviar(&self, pedido: &Pedido, token: Option<&str>) -> Result<Resposta, String> {
        let url = format!("{}{}", self.base, pedido.caminho);
        let mut req = match pedido.metodo {
            Metodo::Get => self.cliente.get(url),
            Metodo::Post => self.cliente.post(url),
            Metodo::Delete => self.cliente.delete(url),
        };
        for (k, v) in &pedido.cabecalhos {
            req = req.header(*k, v);
        }
        if let Some(t) = token {
            req = req.bearer_auth(t);
        }
        if !pedido.corpo.is_empty() {
            req = req.body(pedido.corpo.clone());
        }
        let resp = req.send().map_err(|e| e.to_string())?;
        let status = resp.status().as_u16();
        let corpo = resp.bytes().map_err(|e| e.to_string())?.to_vec();
        Ok(Resposta { status, corpo })
    }

    fn abrir_fluxo(
        &self,
        caminho: &str,
        token: Option<&str>,
        ultimo_id: Option<u64>,
    ) -> Result<Fluxo, String> {
        let mut req = self
            .fluxo
            .get(format!("{}{caminho}", self.base))
            .header("accept", "text/event-stream");
        if let Some(t) = token {
            req = req.bearer_auth(t);
        }
        if let Some(id) = ultimo_id {
            req = req.header("last-event-id", id.to_string());
        }
        let resp = req.send().map_err(|e| e.to_string())?;
        if resp.status().is_success() {
            return Ok(Fluxo::Aberto(Box::new(resp)));
        }
        let status = resp.status().as_u16();
        let corpo = resp.bytes().map_err(|e| e.to_string())?.to_vec();
        Ok(Fluxo::Recusado(Resposta { status, corpo }))
    }
}
