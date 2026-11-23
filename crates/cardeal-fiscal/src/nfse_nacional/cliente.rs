//! O cliente da **Sefin Nacional** (API do Emissor Público Nacional): HTTPS com **TLS mútuo**
//! (o próprio A1 da empresa autentica a conexão) e JSON com o XML assinado em gzip+base64.
//!
//! - `POST /nfse` `{dpsXmlGZipB64}` → `{chaveAcesso, nfseXmlGZipB64, alertas}` (201) ou
//!   `{erros: [{codigo, descricao, complemento}]}` (400);
//! - `GET /dps/{id}` → `{chaveAcesso}` (só para quem é ator da nota) ou 404;
//! - `GET /nfse/{chave}` → `{nfseXmlGZipB64}`;
//! - `POST /nfse/{chave}/eventos` `{pedidoRegistroEventoXmlGZipB64}` → `{eventoXmlGZipB64}`.
//!
//! O servidor é verificado pelas raízes públicas (a Sefin usa uma AC sob a `GlobalSign R46`).

use std::fmt::Write as _;
use std::io::{Read, Write};
use std::time::Duration;

use base64::Engine as _;
use serde_json::Value;

use crate::certificado::ChaveDeAssinatura;
use crate::erros::ErroFiscal;
use crate::nfse::{RetornoNfse, SituacaoNfse};
use crate::porta::AmbienteFiscal;

/// As URLs da Sefin Nacional.
#[must_use]
pub fn url_sefin(ambiente: AmbienteFiscal) -> &'static str {
    match ambiente {
        AmbienteFiscal::Producao => "https://sefin.nfse.gov.br/SefinNacional",
        AmbienteFiscal::Homologacao => "https://sefin.producaorestrita.nfse.gov.br/SefinNacional",
    }
}

/// Uma resposta HTTP: o status e o corpo.
pub(crate) type Resposta = (u16, String);

/// A conexão com a Sefin para um certificado.
pub(crate) struct ClienteSefin {
    http: reqwest::blocking::Client,
    base: String,
}

impl ClienteSefin {
    /// Monta o cliente com o certificado para o TLS mútuo.
    ///
    /// O TLS é o **nativo** do sistema (`SChannel` no Windows, `OpenSSL` no Linux, `Security` no
    /// macOS): o IIS da Sefin só pede o certificado do cliente numa **renegociação** TLS 1.2,
    /// depois de receber a requisição — e o rustls não renegocia (a conexão cai com "reset").
    pub(crate) fn novo(
        chave: &ChaveDeAssinatura,
        ambiente: AmbienteFiscal,
    ) -> Result<Self, ErroFiscal> {
        let base = url_sefin(ambiente).to_string();
        let certificados = chave.pem_certificados();
        let chave_pem = chave.pem_chave();
        // O cliente bloqueante não pode nascer dentro do runtime assíncrono do servidor.
        let http = std::thread::scope(|s| {
            s.spawn(|| {
                let identidade = reqwest::Identity::from_pkcs8_pem(
                    certificados.as_bytes(),
                    chave_pem.as_bytes(),
                )
                .map_err(|e| {
                    ErroFiscal::CertificadoInvalido(format!(
                        "o certificado não serve para a conexão segura ({})",
                        cadeia_do_erro(&e)
                    ))
                })?;
                reqwest::blocking::Client::builder()
                    .use_native_tls()
                    .identity(identidade)
                    .https_only(true)
                    .connect_timeout(Duration::from_secs(15))
                    .timeout(Duration::from_secs(60))
                    .user_agent(concat!("Cardeal/", env!("CARGO_PKG_VERSION")))
                    .build()
                    .map_err(|_| ErroFiscal::ServicoIndisponivel)
            })
            .join()
            .unwrap_or(Err(ErroFiscal::ServicoIndisponivel))
        })?;
        Ok(Self { http, base })
    }

    fn pedir(
        &self,
        metodo: reqwest::Method,
        caminho: &str,
        corpo: Option<&Value>,
    ) -> Result<Resposta, ErroFiscal> {
        let url = format!("{}{caminho}", self.base);
        std::thread::scope(|s| {
            s.spawn(|| {
                let mut req = self.http.request(metodo, &url).header("Accept", "application/json");
                if let Some(c) = corpo {
                    req = req.json(c);
                }
                let resposta = req.send().map_err(|e| {
                    let cadeia = cadeia_do_erro(&e);
                    tracing::warn!(erro = %cadeia, url = %url, "Sefin Nacional não respondeu");
                    erro_de_conexao(&cadeia)
                })?;
                let status = resposta.status().as_u16();
                let texto = resposta.text().map_err(|_| ErroFiscal::ServicoIndisponivel)?;
                if status >= 400 {
                    tracing::warn!(status, corpo = %texto.chars().take(500).collect::<String>(), "Sefin Nacional recusou");
                }
                Ok((status, texto))
            })
            .join()
            .unwrap_or(Err(ErroFiscal::ServicoIndisponivel))
        })
    }

    pub(crate) fn emitir(&self, dps_assinada: &str) -> Result<Resposta, ErroFiscal> {
        let corpo = serde_json::json!({ "dpsXmlGZipB64": gzip_base64(dps_assinada) });
        self.pedir(reqwest::Method::POST, "/nfse", Some(&corpo))
    }

    pub(crate) fn chave_da_dps(&self, id: &str) -> Result<Resposta, ErroFiscal> {
        self.pedir(reqwest::Method::GET, &format!("/dps/{id}"), None)
    }

    pub(crate) fn nfse(&self, chave: &str) -> Result<Resposta, ErroFiscal> {
        self.pedir(reqwest::Method::GET, &format!("/nfse/{chave}"), None)
    }

    pub(crate) fn evento(
        &self,
        chave: &str,
        pedido_assinado: &str,
    ) -> Result<Resposta, ErroFiscal> {
        let corpo =
            serde_json::json!({ "pedidoRegistroEventoXmlGZipB64": gzip_base64(pedido_assinado) });
        self.pedir(
            reqwest::Method::POST,
            &format!("/nfse/{chave}/eventos"),
            Some(&corpo),
        )
    }
}

/// O erro e as causas, numa linha (o `reqwest` esconde o alerta do TLS nas causas).
fn cadeia_do_erro(e: &dyn std::error::Error) -> String {
    let mut s = e.to_string();
    let mut causa = e.source();
    while let Some(c) = causa {
        let _ = write!(s, ": {c}");
        causa = c.source();
    }
    s
}

/// A falha de conexão: o servidor recusou o certificado no aperto de mão (alerta TLS) é
/// problema do certificado — não adianta tentar de novo; o resto é rede.
pub(crate) fn erro_de_conexao(cadeia: &str) -> ErroFiscal {
    let c = cadeia.to_lowercase();
    let recusou = [
        "certificaterequired",
        "certificate required",
        "badcertificate",
        "bad certificate",
        "unknownca",
        "unknown ca",
        "certificaterevoked",
        "certificateexpired",
        "certificateunknown",
        "accessdenied",
        "handshakefailure",
    ]
    .iter()
    .any(|t| c.contains(t));
    if recusou {
        ErroFiscal::CertificadoInvalido(format!(
            "a Sefin Nacional recusou o certificado na conexão segura — ele precisa ser ICP-Brasil, \
             válido, não revogado e permitir autenticação de cliente ({})",
            cadeia.chars().take(200).collect::<String>()
        ))
    } else {
        ErroFiscal::ServicoIndisponivel
    }
}

/// gzip + base64 (o `…XmlGZipB64`).
pub(crate) fn gzip_base64(xml: &str) -> String {
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    // Escrever num Vec não falha.
    let _ = gz.write_all(xml.as_bytes());
    let bytes = gz.finish().unwrap_or_default();
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// O inverso, com teto (um XML de NFS-e tem dezenas de KB; 8 MB é folga e barra a bomba de gzip).
pub(crate) fn de_gzip_base64(b64: &str) -> Option<String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .ok()?;
    let mut texto = String::new();
    flate2::read::GzDecoder::new(bytes.as_slice())
        .take(8 * 1024 * 1024)
        .read_to_string(&mut texto)
        .ok()?;
    Some(texto)
}

/// As chaves do JSON em minúsculas (a Sefin já respondeu `Codigo` e `codigo`).
fn minusculo(v: &Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .map(|(k, v)| (k.to_lowercase(), minusculo(v)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(minusculo).collect()),
        outro => outro.clone(),
    }
}

fn texto_de(v: &Value, chave: &str) -> Option<String> {
    v.get(chave).and_then(|x| match x {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    })
}

/// As mensagens de erro de qualquer forma que a Sefin devolva ("E0014 — descrição (complemento)").
pub(crate) fn mensagens(corpo: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<Value>(corpo) else {
        // Página de erro do IIS/gateway: o título basta.
        if corpo.to_lowercase().contains("<html") {
            return entre(corpo, "<title>", "</title>")
                .map(|t| vec![t.trim().to_string()])
                .unwrap_or_default();
        }
        let t: String = corpo.trim().chars().take(300).collect();
        return if t.is_empty() { Vec::new() } else { vec![t] };
    };
    let v = minusculo(&v);
    let ler = |e: &Value| -> Option<String> {
        let codigo = texto_de(e, "codigo").unwrap_or_default();
        let descricao = texto_de(e, "descricao")
            .or_else(|| texto_de(e, "mensagem"))
            .unwrap_or_default();
        let mut m = match (codigo.is_empty(), descricao.is_empty()) {
            (false, false) => format!("{codigo} — {descricao}"),
            (true, false) => descricao,
            (false, true) => codigo,
            (true, true) => return None,
        };
        if let Some(c) = texto_de(e, "complemento") {
            let _ = write!(m, " ({c})");
        }
        Some(m)
    };
    let lista = |chave: &str| -> Vec<String> {
        match v.get(chave) {
            Some(Value::Array(a)) => a.iter().filter_map(ler).collect(),
            Some(o @ Value::Object(_)) => ler(o).into_iter().collect(),
            _ => Vec::new(),
        }
    };
    let mut saida = lista("erros");
    saida.extend(lista("erro"));
    if saida.is_empty() {
        saida = lista("alertas");
    }
    if saida.is_empty() {
        if let Some(m) = texto_de(&v, "detail")
            .or_else(|| texto_de(&v, "message"))
            .or_else(|| texto_de(&v, "title"))
        {
            saida.push(m);
        }
    }
    saida
}

/// Os códigos das mensagens (o que vem antes do " — ").
pub(crate) fn codigos(mensagens: &[String]) -> Vec<&str> {
    mensagens
        .iter()
        .filter_map(|m| m.split(" — ").next())
        .collect()
}

/// O número e a chave lidos do XML da NFS-e.
pub(crate) fn dados_do_xml(xml: &str) -> (Option<u64>, Option<String>) {
    let numero = entre(xml, "<nNFSe>", "</nNFSe>").and_then(|n| n.trim().parse().ok());
    let chave = xml
        .find("Id=\"NFS")
        .and_then(|i| xml.get(i + 7..i + 57))
        .filter(|c| c.len() == 50)
        .map(str::to_string);
    (numero, chave)
}

fn entre<'a>(s: &'a str, abre: &str, fecha: &str) -> Option<&'a str> {
    let i = s.find(abre)? + abre.len();
    let j = s[i..].find(fecha)? + i;
    Some(&s[i..j])
}

fn retorno(situacao: SituacaoNfse, mensagens: Vec<String>) -> RetornoNfse {
    RetornoNfse {
        situacao,
        numero: None,
        chave: None,
        codigo_verificacao: None,
        xml: None,
        mensagens,
    }
}

/// A resposta do `POST /nfse` (ou do `GET /nfse/{chave}`).
pub(crate) fn interpretar_nfse((status, corpo): &Resposta) -> RetornoNfse {
    if (200..300).contains(status) {
        let v = serde_json::from_str::<Value>(corpo).map_or(Value::Null, |v| minusculo(&v));
        let xml = texto_de(&v, "nfsexmlgzipb64").and_then(|b| de_gzip_base64(&b));
        let (numero, chave_xml) = xml.as_deref().map(dados_do_xml).unwrap_or_default();
        let chave = texto_de(&v, "chaveacesso").or(chave_xml);
        if xml.is_none() && chave.is_none() {
            return retorno(
                SituacaoNfse::Erro,
                vec![format!("{status} — resposta sem a NFS-e")],
            );
        }
        let alertas = mensagens(corpo);
        return RetornoNfse {
            situacao: SituacaoNfse::Autorizada,
            numero,
            chave,
            codigo_verificacao: None,
            xml,
            mensagens: alertas,
        };
    }
    let m = mensagens(corpo);
    match status {
        400 | 422 | 409 => retorno(SituacaoNfse::Rejeitada, if m.is_empty() { vec![format!("{status} — recusada")] } else { m }),
        404 => retorno(SituacaoNfse::Rejeitada, vec![format!("404 — não encontrada{}", m.first().map(|x| format!(": {x}")).unwrap_or_default())]),
        401 | 403 => retorno(SituacaoNfse::Rejeitada, vec![format!(
            "{status} — a Sefin recusou o certificado na conexão (vencido, revogado, cadeia incompleta ou sem permissão){}",
            m.first().map(|x| format!(": {x}")).unwrap_or_default()
        )]),
        _ => retorno(SituacaoNfse::Erro, if m.is_empty() { vec![format!("{status} — erro na Sefin Nacional")] } else { m }),
    }
}

/// A resposta do `GET /dps/{id}`: a chave, ou `None` se a DPS não virou nota (404).
///
/// # Errors
/// Outra resposta (a mensagem, para o motivo).
pub(crate) fn interpretar_chave_da_dps(
    (status, corpo): &Resposta,
) -> Result<Option<String>, String> {
    match status {
        200..=299 => {
            let v = serde_json::from_str::<Value>(corpo).map_or(Value::Null, |v| minusculo(&v));
            texto_de(&v, "chaveacesso")
                .map(Some)
                .ok_or_else(|| "resposta sem a chave de acesso".into())
        }
        404 => Ok(None),
        _ => Err(mensagens(corpo).join("; ")).map_err(|m| {
            if m.is_empty() {
                format!("HTTP {status}")
            } else {
                m
            }
        }),
    }
}

/// A resposta do registro do evento de cancelamento.
pub(crate) fn interpretar_cancelamento((status, corpo): &Resposta) -> RetornoNfse {
    if (200..300).contains(status) {
        let v = serde_json::from_str::<Value>(corpo).map_or(Value::Null, |v| minusculo(&v));
        let xml = texto_de(&v, "eventoxmlgzipb64").and_then(|b| de_gzip_base64(&b));
        return RetornoNfse {
            situacao: SituacaoNfse::Cancelada,
            numero: None,
            chave: None,
            codigo_verificacao: None,
            xml,
            mensagens: Vec::new(),
        };
    }
    let mut r = interpretar_nfse(&(*status, corpo.clone()));
    if r.situacao == SituacaoNfse::Autorizada {
        r.situacao = SituacaoNfse::Erro;
    }
    r
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn gzip_vai_e_volta() {
        let xml = "<DPS>á & ç</DPS>";
        assert_eq!(de_gzip_base64(&gzip_base64(xml)).as_deref(), Some(xml));
        assert_eq!(de_gzip_base64("isso não é base64"), None);
    }

    #[test]
    fn a_nota_autorizada_traz_numero_chave_e_xml() {
        let chave = "43149022112223330001810000000000000126110123456789";
        let xml = format!("<NFSe><infNFSe Id=\"NFS{chave}\"><nNFSe>261</nNFSe></infNFSe></NFSe>");
        let corpo = serde_json::json!({
            "tipoAmbiente": 2, "idDps": "DPS1", "chaveAcesso": chave,
            "nfseXmlGZipB64": gzip_base64(&xml), "alertas": []
        })
        .to_string();
        let r = interpretar_nfse(&(201, corpo));
        assert_eq!(r.situacao, SituacaoNfse::Autorizada);
        assert_eq!(r.numero, Some(261));
        assert_eq!(r.chave.as_deref(), Some(chave));
        assert_eq!(r.xml.as_deref(), Some(xml.as_str()));
    }

    #[test]
    fn a_recusa_le_os_erros_em_qualquer_caixa() {
        let corpo = r#"{"tipoAmbiente":2,"erros":[{"Codigo":"E0014","Descricao":"DPS já existe","Complemento":"série 1"},{"codigo":"E1235","descricao":"Falha Schema Xml"}]}"#;
        let r = interpretar_nfse(&(400, corpo.into()));
        assert_eq!(r.situacao, SituacaoNfse::Rejeitada);
        assert_eq!(
            r.mensagens,
            vec![
                "E0014 — DPS já existe (série 1)",
                "E1235 — Falha Schema Xml"
            ]
        );
        assert_eq!(codigos(&r.mensagens), vec!["E0014", "E1235"]);
    }

    #[test]
    fn a_pagina_do_iis_vira_o_titulo() {
        let html = "<!DOCTYPE html><html><head><title>403 - Forbidden: Access is denied.</title></head><body><h2>x</h2></body></html>";
        let r = interpretar_nfse(&(403, html.into()));
        assert_eq!(r.situacao, SituacaoNfse::Rejeitada);
        assert!(
            r.motivo().contains("recusou o certificado"),
            "{}",
            r.motivo()
        );
        assert!(
            r.motivo().ends_with(": 403 - Forbidden: Access is denied."),
            "{}",
            r.motivo()
        );
    }

    #[test]
    fn sem_resposta_que_decida_e_erro_e_403_e_certificado() {
        assert_eq!(
            interpretar_nfse(&(502, "<html>bad gateway</html>".into())).situacao,
            SituacaoNfse::Erro
        );
        let r = interpretar_nfse(&(403, String::new()));
        assert_eq!(r.situacao, SituacaoNfse::Rejeitada);
        assert!(r.motivo().contains("certificado"));
        assert!(interpretar_nfse(&(404, String::new())).nao_encontrada());
    }

    #[test]
    fn a_chave_da_dps() {
        assert_eq!(
            interpretar_chave_da_dps(&(200, r#"{"chaveAcesso":"123"}"#.into())),
            Ok(Some("123".into()))
        );
        assert_eq!(interpretar_chave_da_dps(&(404, String::new())), Ok(None));
        assert!(interpretar_chave_da_dps(&(500, String::new())).is_err());
    }

    #[test]
    fn o_cancelamento() {
        let ok = serde_json::json!({ "eventoXmlGZipB64": gzip_base64("<evento/>") }).to_string();
        let r = interpretar_cancelamento(&(201, ok));
        assert_eq!(r.situacao, SituacaoNfse::Cancelada);
        assert_eq!(r.xml.as_deref(), Some("<evento/>"));
        let r = interpretar_cancelamento(&(
            400,
            r#"{"erros":[{"codigo":"E0840","descricao":"Prazo expirado"}]}"#.into(),
        ));
        assert_eq!(r.situacao, SituacaoNfse::Rejeitada);
    }
}
