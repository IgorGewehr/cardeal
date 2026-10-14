//! [`ApiFiscalHttp`] — o adaptador real de [`PortaFiscal`]: cliente do `sefaz-api`, o motor
//! fiscal próprio da operação (ADR-0006, `docs/modulos/fiscal.md`).
//!
//! O contrato do `sefaz-api` (branch `main`, a publicada): `Authorization: Bearer <chave>`, e o
//! certificado A1 (`pfxBase64` + `password`) **no corpo de toda chamada** — o serviço não
//! guarda nada. Por isso o adaptador é montado por empresa, já com o certificado aberto do
//! cofre, e descartado depois do uso.
//!
//! As chamadas são síncronas (a porta é síncrona — ver `porta`), feitas numa thread própria:
//! o cliente HTTP bloqueante não pode rodar dentro do runtime assíncrono do servidor.

use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use cardeal_kernel::{Cnpj, Erro, Resultado, Uf};
use cardeal_storage::Segredo;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::chave::ChaveAcesso;
use crate::erros::ErroFiscal;
use crate::porta::{
    AmbienteFiscal, ModeloFiscal, Nsu, PortaFiscal, ResumoDfe, StatusSefaz, XmlNota,
};

/// Onde está o `sefaz-api` e com que chave falar com ele.
#[derive(Debug, Clone)]
pub struct ConfigApiFiscal {
    /// A URL base (sem barra no fim).
    pub url: String,
    /// A chave de API (`Bearer`).
    pub token: Segredo<String>,
    /// Quanto esperar por uma resposta. A emissão de NF-e faz polling na SEFAZ do lado do
    /// `sefaz-api` (até ~30 s), então o padrão é folgado.
    pub tempo_limite: Duration,
}

impl ConfigApiFiscal {
    /// O `sefaz-api` publicado da operação.
    pub const URL_PADRAO: &'static str = "https://emissao.tensorroot.com";
    /// A variável de ambiente com a chave de API.
    pub const VAR_TOKEN: &'static str = "CARDEAL_FISCAL_TOKEN";
    /// A variável de ambiente que troca a URL (homologação local, outro servidor).
    pub const VAR_URL: &'static str = "CARDEAL_FISCAL_URL";

    /// Uma configuração com a URL e a chave dadas.
    #[must_use]
    pub fn nova(url: impl Into<String>, token: impl Into<String>) -> Self {
        Self {
            url: url.into().trim_end_matches('/').to_string(),
            token: Segredo::novo(token.into()),
            tempo_limite: Duration::from_secs(60),
        }
    }

    /// A configuração do ambiente do processo: [`Self::VAR_TOKEN`] obrigatória,
    /// [`Self::VAR_URL`] opcional. `None` sem chave — o fiscal fica no simulado.
    #[must_use]
    pub fn do_ambiente() -> Option<Self> {
        let token = std::env::var(Self::VAR_TOKEN).ok()?;
        if token.trim().is_empty() {
            return None;
        }
        let url = std::env::var(Self::VAR_URL).unwrap_or_else(|_| Self::URL_PADRAO.to_string());
        Some(Self::nova(url, token.trim()))
    }
}

/// O certificado A1 aberto do cofre — só vive o tempo de uma conversa com a API.
#[derive(Clone)]
pub struct CertificadoA1 {
    /// Os bytes do `.pfx`.
    pub pfx: Segredo<Vec<u8>>,
    /// A senha.
    pub senha: Segredo<String>,
}

impl std::fmt::Debug for CertificadoA1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CertificadoA1([oculto])")
    }
}

/// O cliente do `sefaz-api` para uma empresa.
#[derive(Debug, Clone)]
pub struct ApiFiscalHttp {
    config: Arc<ConfigApiFiscal>,
    certificado: CertificadoA1,
    ambiente: AmbienteFiscal,
}

impl ApiFiscalHttp {
    /// Monta o cliente para o certificado e o ambiente da empresa.
    #[must_use]
    pub const fn novo(
        config: Arc<ConfigApiFiscal>,
        certificado: CertificadoA1,
        ambiente: AmbienteFiscal,
    ) -> Self {
        Self {
            config,
            certificado,
            ambiente,
        }
    }

    fn corpo_base(&self) -> serde_json::Map<String, Value> {
        let pfx = base64::engine::general_purpose::STANDARD.encode(self.certificado.pfx.expor());
        let mut corpo = serde_json::Map::new();
        corpo.insert(
            "certificado".into(),
            json!({ "pfxBase64": pfx, "password": self.certificado.senha.expor() }),
        );
        corpo.insert(
            "ambiente".into(),
            Value::from(match self.ambiente {
                AmbienteFiscal::Homologacao => "homologacao",
                AmbienteFiscal::Producao => "producao",
            }),
        );
        corpo
    }

    /// Faz um `POST` e devolve a resposta no formato `SefazResponse` do `sefaz-api`.
    fn postar(&self, caminho: &str, corpo: &Value) -> Result<RespostaSefazApi, ErroFiscal> {
        let url = format!("{}{caminho}", self.config.url);
        let config = &self.config;
        let enviar = || -> Result<(u16, String), ErroFiscal> {
            let cliente = reqwest::blocking::Client::builder()
                .timeout(config.tempo_limite)
                .build()
                .map_err(|_| ErroFiscal::ServicoIndisponivel)?;
            let resposta = cliente
                .post(&url)
                .bearer_auth(config.token.expor())
                .json(corpo)
                .send()
                .map_err(|e| {
                    tracing::warn!(erro = %e, "sefaz-api não respondeu");
                    ErroFiscal::ServicoIndisponivel
                })?;
            let status = resposta.status().as_u16();
            let texto = resposta
                .text()
                .map_err(|_| ErroFiscal::ServicoIndisponivel)?;
            Ok((status, texto))
        };
        let (status, texto) = std::thread::scope(|s| {
            s.spawn(enviar)
                .join()
                .unwrap_or(Err(ErroFiscal::ServicoIndisponivel))
        })?;
        interpretar_resposta(status, &texto)
    }
}

/// O corpo de resposta do `sefaz-api` (`SefazResponse`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct RespostaSefazApi {
    pub success: bool,
    pub status: String,
    pub codigo_status: String,
    pub motivo_status: String,
}

#[derive(Deserialize)]
struct RespostaDeErro {
    error: String,
}

/// Traduz o status HTTP e o corpo numa resposta ou num [`ErroFiscal`].
fn interpretar_resposta(status: u16, texto: &str) -> Result<RespostaSefazApi, ErroFiscal> {
    let mensagem = || {
        serde_json::from_str::<RespostaDeErro>(texto)
            .map_or_else(|_| texto.chars().take(200).collect(), |e| e.error)
    };
    match status {
        // 422: a SEFAZ (ou a validação do sefaz-api) recusou — vem no mesmo formato.
        200 | 422 => serde_json::from_str(texto).map_err(|_| {
            ErroFiscal::ApiFiscalRecusou("resposta da API fiscal em formato inesperado".into())
        }),
        401 => Err(ErroFiscal::CredencialDaApiFiscal),
        403 => {
            let m = mensagem();
            if m.contains("API key") {
                Err(ErroFiscal::CredencialDaApiFiscal)
            } else {
                Err(ErroFiscal::ApiFiscalRecusou(m))
            }
        }
        429 | 500..=599 => Err(ErroFiscal::ServicoIndisponivel),
        _ => Err(ErroFiscal::ApiFiscalRecusou(mensagem())),
    }
}

impl PortaFiscal for ApiFiscalHttp {
    fn distribuicao_dfe(&self, _cnpj: &Cnpj, _desde: Nsu) -> Resultado<Vec<ResumoDfe>> {
        Err(Erro::de_dominio(&ErroFiscal::NaoSuportadoPeloProvedor(
            "a distribuição de notas de entrada (DFe)".into(),
        )))
    }

    fn baixar_xml(&self, _chave: &ChaveAcesso) -> Resultado<XmlNota> {
        Err(Erro::de_dominio(&ErroFiscal::NaoSuportadoPeloProvedor(
            "o download de notas de entrada (DFe)".into(),
        )))
    }

    fn status_servico(&self, uf: Uf, modelo: ModeloFiscal) -> Resultado<StatusSefaz> {
        let mut corpo = self.corpo_base();
        corpo.insert("ufEmitente".into(), Value::from(uf.sigla()));
        corpo.insert("modelo".into(), Value::from(modelo.codigo()));
        let r = self
            .postar("/nfe/status", &Value::Object(corpo))
            .map_err(|e| Erro::de_dominio(&e))?;
        Ok(StatusSefaz {
            em_operacao: r.success && r.codigo_status == "107",
            codigo: r.codigo_status,
            motivo: r.motivo_status,
        })
    }
}

#[cfg(test)]
mod testes {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;

    use super::*;

    /// Um servidor HTTP de uma requisição só: devolve `(status, corpo)` e entrega o que
    /// recebeu (cabeçalhos + corpo).
    fn servidor(status: u16, corpo: &'static str) -> (String, std::thread::JoinHandle<String>) {
        let ouvinte = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", ouvinte.local_addr().unwrap());
        let tarefa = std::thread::spawn(move || {
            let (conexao, _) = ouvinte.accept().unwrap();
            let mut leitor = BufReader::new(conexao.try_clone().unwrap());
            let mut cabecalhos = String::new();
            let mut tamanho = 0usize;
            loop {
                let mut linha = String::new();
                leitor.read_line(&mut linha).unwrap();
                if let Some(v) = linha.to_ascii_lowercase().strip_prefix("content-length:") {
                    tamanho = v.trim().parse().unwrap();
                }
                if linha == "\r\n" {
                    break;
                }
                cabecalhos.push_str(&linha);
            }
            let mut recebido = vec![0u8; tamanho];
            leitor.read_exact(&mut recebido).unwrap();
            let mut escrita = conexao;
            write!(
                escrita,
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{corpo}",
                corpo.len()
            )
            .unwrap();
            format!("{cabecalhos}\n{}", String::from_utf8(recebido).unwrap())
        });
        (url, tarefa)
    }

    fn cliente(url: &str) -> ApiFiscalHttp {
        ApiFiscalHttp::novo(
            Arc::new(ConfigApiFiscal::nova(format!("{url}/"), "chave-de-teste")),
            CertificadoA1 {
                pfx: Segredo::novo(b"PFX".to_vec()),
                senha: Segredo::novo("s3nha".into()),
            },
            AmbienteFiscal::Homologacao,
        )
    }

    #[test]
    fn status_manda_o_contrato_do_sefaz_api_e_le_a_resposta() {
        let (url, tarefa) = servidor(
            200,
            r#"{"success":true,"status":"autorizado","codigoStatus":"107","motivoStatus":"Servico em Operacao","dataRecebimento":"2026-10-14T17:00:00-03:00"}"#,
        );
        let status = cliente(&url)
            .status_servico(Uf::Sc, ModeloFiscal::Nfce)
            .unwrap();
        assert!(status.em_operacao);
        assert_eq!(status.motivo, "Servico em Operacao");

        let recebido = tarefa.join().unwrap();
        assert!(recebido.starts_with("POST /nfe/status "), "{recebido}");
        assert!(recebido
            .to_ascii_lowercase()
            .contains("authorization: bearer chave-de-teste"));
        let corpo: Value = serde_json::from_str(recebido.rsplit('\n').next().unwrap()).unwrap();
        assert_eq!(corpo["certificado"]["pfxBase64"], "UEZY");
        assert_eq!(corpo["certificado"]["password"], "s3nha");
        assert_eq!(corpo["ufEmitente"], "SC");
        assert_eq!(corpo["modelo"], "65");
        assert_eq!(corpo["ambiente"], "homologacao");
    }

    #[test]
    fn erros_da_api_viram_erros_que_a_tela_explica() {
        assert_eq!(
            interpretar_resposta(403, r#"{"error":"API key inválida"}"#).unwrap_err(),
            ErroFiscal::CredencialDaApiFiscal
        );
        assert_eq!(
            interpretar_resposta(
                403,
                r#"{"error":"CNPJ do certificado não corresponde ao CNPJ do emitente"}"#
            )
            .unwrap_err(),
            ErroFiscal::ApiFiscalRecusou(
                "CNPJ do certificado não corresponde ao CNPJ do emitente".into()
            )
        );
        assert_eq!(
            interpretar_resposta(502, "Bad gateway").unwrap_err(),
            ErroFiscal::ServicoIndisponivel
        );
        assert_eq!(
            interpretar_resposta(429, "{}").unwrap_err(),
            ErroFiscal::ServicoIndisponivel
        );
        let rejeitado = interpretar_resposta(
            422,
            r#"{"success":false,"status":"rejeitado","codigoStatus":"539","motivoStatus":"Duplicidade"}"#,
        )
        .unwrap();
        assert_eq!(rejeitado.codigo_status, "539");
    }

    #[test]
    fn api_fora_do_ar_e_servico_indisponivel() {
        // Uma porta que ninguém escuta.
        let porta = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let erro = cliente(&format!("http://127.0.0.1:{porta}"))
            .status_servico(Uf::Sc, ModeloFiscal::Nfe)
            .unwrap_err();
        assert_eq!(erro.codigo, cardeal_kernel::CodigoErro::FISCAL_INDISPONIVEL);
    }

    /// A conferência de verdade, em homologação, contra o `sefaz-api` publicado. Precisa de
    /// `CARDEAL_FISCAL_TOKEN`, `CARDEAL_TESTE_PFX` (caminho do `.pfx`),
    /// `CARDEAL_TESTE_PFX_SENHA` e `CARDEAL_TESTE_UF`. Rodar com `--ignored`.
    #[test]
    #[ignore = "fala com a SEFAZ de verdade (homologação)"]
    fn homologacao_status_real() {
        let config = ConfigApiFiscal::do_ambiente().expect("CARDEAL_FISCAL_TOKEN");
        let pfx =
            std::fs::read(std::env::var("CARDEAL_TESTE_PFX").expect("CARDEAL_TESTE_PFX")).unwrap();
        let senha = std::env::var("CARDEAL_TESTE_PFX_SENHA").expect("CARDEAL_TESTE_PFX_SENHA");
        let uf =
            Uf::de_sigla(&std::env::var("CARDEAL_TESTE_UF").expect("CARDEAL_TESTE_UF")).unwrap();
        let preparado = crate::preparar_certificado_a1(&pfx, &senha).unwrap();
        eprintln!(
            "certificado: {} até {:?} (convertido: {})",
            preparado.info.titular, preparado.info.valido_ate, preparado.convertido
        );
        if let Ok(destino) = std::env::var("CARDEAL_TESTE_GRAVAR") {
            std::fs::write(destino, &preparado.pfx).unwrap();
        }
        let pfx = preparado.pfx;
        let api = ApiFiscalHttp::novo(
            Arc::new(config),
            CertificadoA1 {
                pfx: Segredo::novo(pfx),
                senha: Segredo::novo(senha),
            },
            AmbienteFiscal::Homologacao,
        );
        for modelo in [ModeloFiscal::Nfe, ModeloFiscal::Nfce] {
            let status = api.status_servico(uf, modelo).unwrap();
            eprintln!("{modelo:?}: {} {}", status.codigo, status.motivo);
            assert!(status.em_operacao, "{status:?}");
        }
    }
}
