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
use crate::emissao::{Csc, ItemDocumento, Nfce, PedidoInutilizacao, RetornoSefaz, SituacaoRetorno};
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
        if status >= 400 {
            tracing::warn!(status, corpo = %texto.chars().take(300).collect::<String>(), "sefaz-api recusou");
            #[cfg(test)]
            eprintln!(
                "sefaz-api HTTP {status}: {}",
                texto.chars().take(300).collect::<String>()
            );
        }
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
    pub chave_acesso: Option<String>,
    pub protocolo: Option<String>,
    pub data_recebimento: Option<String>,
    pub xml: Option<String>,
    pub xml_assinado: Option<String>,
    pub xml_cancelamento: Option<String>,
    pub erros: Option<Vec<String>>,
}

impl RespostaSefazApi {
    /// Em [`RetornoSefaz`]. A validação do provedor ("erro" com código 400) é uma rejeição: o
    /// número não foi usado e a nota precisa de correção. O `sefaz-api` também chama de "erro"
    /// as rejeições com código abaixo de 200 (algumas UFs usam, ex.: SP 178) — pelo motivo
    /// ("Rejeição: ...", "Uso Denegado: ...") elas voltam ao que são.
    fn em_retorno(self) -> RetornoSefaz {
        let motivo = self.motivo_status.trim_start().to_lowercase();
        let situacao = match self.status.as_str() {
            "erro" if motivo.starts_with("rejei") => SituacaoRetorno::Rejeitado,
            "erro" if motivo.starts_with("uso denegado") => SituacaoRetorno::Denegado,
            "autorizado" => SituacaoRetorno::Autorizado,
            "rejeitado" => SituacaoRetorno::Rejeitado,
            "denegado" => SituacaoRetorno::Denegado,
            "processando" => SituacaoRetorno::Processando,
            "cancelado" => SituacaoRetorno::Cancelado,
            "erro" if self.codigo_status == "400" => SituacaoRetorno::Rejeitado,
            _ => SituacaoRetorno::Erro,
        };
        RetornoSefaz {
            situacao,
            codigo: self.codigo_status,
            motivo: self.motivo_status,
            chave: self.chave_acesso.filter(|c| !c.is_empty()),
            protocolo: self.protocolo.filter(|p| !p.is_empty()),
            recebido_em: self.data_recebimento.filter(|d| !d.is_empty()),
            xml: self.xml.filter(|x| !x.is_empty()),
            xml_assinado: self.xml_assinado.filter(|x| !x.is_empty()),
            xml_cancelamento: self.xml_cancelamento.filter(|x| !x.is_empty()),
            erros: self.erros.unwrap_or_default(),
        }
    }
}

/// Dinheiro, quantidade, preço e percentual como número JSON (o `sefaz-api` formata as
/// casas): centavos/100, 1e-4, 1e-6, pontos percentuais.
#[allow(clippy::cast_precision_loss)]
fn reais(d: cardeal_kernel::Dinheiro) -> f64 {
    d.em_centavos() as f64 / 100.0
}

#[allow(clippy::cast_precision_loss)]
fn numero_quantidade(q: cardeal_kernel::Quantidade) -> f64 {
    q.unidades_internas() as f64 / 10_000.0
}

#[allow(clippy::cast_precision_loss)]
fn numero_preco(p: cardeal_kernel::Preco) -> f64 {
    p.unidades_internas() as f64 / 1_000_000.0
}

#[allow(clippy::cast_precision_loss)]
fn numero_percentual(p: cardeal_kernel::Percentual) -> f64 {
    p.unidades_internas() as f64 / 1_000_000.0
}

fn item_json(numero: usize, item: &ItemDocumento) -> Value {
    let gtin = item.gtin.clone().unwrap_or_else(|| "SEM GTIN".into());
    let mut produto = json!({
        "codigo": item.codigo,
        "cEAN": gtin,
        "descricao": item.descricao,
        "ncm": item.ncm,
        "cfop": item.cfop,
        "unidade": item.unidade,
        "quantidade": numero_quantidade(item.quantidade),
        "valorUnitario": numero_preco(item.valor_unitario),
        "valorTotal": reais(item.valor_bruto),
        "cEANTrib": gtin,
        "unidadeTrib": item.unidade,
        "quantidadeTrib": numero_quantidade(item.quantidade),
        "valorUnitarioTrib": numero_preco(item.valor_unitario),
        "indTot": "1",
    });
    if let Some(cest) = &item.cest {
        produto["cest"] = Value::from(cest.as_str());
    }
    if !item.desconto.e_zero() {
        produto["valorDesconto"] = Value::from(reais(item.desconto));
    }
    let i = &item.impostos.icms;
    let mut icms = json!({ "orig": i.origem.to_string() });
    if let Some(cst) = &i.cst {
        icms["cst"] = Value::from(cst.as_str());
    }
    if let Some(csosn) = &i.csosn {
        icms["csosn"] = Value::from(csosn.as_str());
    }
    if let Some(base) = i.base {
        icms["modBC"] = Value::from("3");
        icms["valorBC"] = Value::from(reais(base));
    }
    if let Some(a) = i.aliquota {
        icms["aliquota"] = Value::from(numero_percentual(a));
    }
    if let Some(v) = i.valor {
        icms["valor"] = Value::from(reais(v));
    }
    if let Some(r) = i.reducao_base {
        icms["pRedBC"] = Value::from(numero_percentual(r));
    }
    let contribuicao = |c: &crate::emissao::ContribuicaoItem| {
        let mut v = json!({ "cst": c.cst });
        if let Some(b) = c.base {
            v["valorBC"] = Value::from(reais(b));
        }
        if let Some(a) = c.aliquota {
            v["aliquota"] = Value::from(numero_percentual(a));
        }
        if let Some(x) = c.valor {
            v["valor"] = Value::from(reais(x));
        }
        v
    };
    json!({
        "numero": numero,
        "produto": produto,
        "imposto": {
            "icms": icms,
            "pis": contribuicao(&item.impostos.pis),
            "cofins": contribuicao(&item.impostos.cofins),
        },
    })
}

/// O corpo do `POST /nfe/nfce/emitir` (sem certificado e ambiente, que o adaptador põe).
pub(crate) fn nfce_json(nota: &Nfce, csc: &Csc) -> serde_json::Map<String, Value> {
    let e = &nota.emitente;
    let en = &e.endereco;
    let mut endereco = json!({
        "logradouro": en.logradouro,
        "numero": en.numero,
        "bairro": en.bairro,
        "codigoMunicipio": en.codigo_municipio,
        "municipio": en.municipio,
        "uf": en.uf.sigla(),
        "cep": en.cep,
    });
    if !en.complemento.is_empty() {
        endereco["complemento"] = Value::from(en.complemento.as_str());
    }
    if let Some(t) = &en.telefone {
        endereco["telefone"] = Value::from(t.as_str());
    }
    let mut emitente = json!({
        "cnpj": e.cnpj.sem_mascara(),
        "nome": e.razao_social,
        "nomeFantasia": e.nome_fantasia,
        "inscricaoEstadual": e.inscricao_estadual,
        "crt": e.crt.codigo(),
        "endereco": endereco,
    });
    if let Some(im) = &e.inscricao_municipal {
        emitente["inscricaoMunicipal"] = Value::from(im.as_str());
    }
    let formas: Vec<Value> = nota
        .pagamentos
        .iter()
        .map(|p| {
            let mut f = json!({ "tipo": p.meio.codigo(), "valor": reais(p.valor) });
            if p.meio.exige_grupo_cartao() {
                f["cartao"] = json!({ "tipoIntegracao": "2" });
            }
            if p.meio == crate::emissao::MeioPagamento::Outros {
                f["descricao"] = Value::from("Outros");
            }
            f
        })
        .collect();
    let mut pagamento = json!({ "indicadorPagamento": "0", "formas": formas });
    if !nota.troco.e_zero() {
        pagamento["troco"] = Value::from(reais(nota.troco));
    }
    let mut corpo = serde_json::Map::new();
    corpo.insert("emitente".into(), emitente);
    corpo.insert(
        "itens".into(),
        Value::Array(
            nota.itens
                .iter()
                .enumerate()
                .map(|(i, it)| item_json(i + 1, it))
                .collect(),
        ),
    );
    corpo.insert("pagamento".into(), pagamento);
    corpo.insert("transporte".into(), json!({ "modFrete": "9" }));
    corpo.insert(
        "csc".into(),
        json!({ "id": csc.id, "token": csc.token.expor() }),
    );
    corpo.insert("serie".into(), Value::from(nota.serie.to_string()));
    corpo.insert("numero".into(), Value::from(nota.numero));
    corpo.insert("ufEmitente".into(), Value::from(en.uf.sigla()));
    if let Some(c) = &nota.consumidor {
        let mut consumidor = match &c.documento {
            crate::emissao::DocumentoConsumidor::Cpf(cpf) => json!({ "cpf": cpf.sem_mascara() }),
            crate::emissao::DocumentoConsumidor::Cnpj(cnpj) => {
                json!({ "cnpj": cnpj.sem_mascara() })
            }
        };
        if let Some(n) = &c.nome {
            consumidor["nome"] = Value::from(n.as_str());
        }
        corpo.insert("consumidor".into(), consumidor);
    }
    if let Some(info) = &nota.informacoes {
        corpo.insert(
            "informacoesAdicionais".into(),
            json!({ "contribuinte": info }),
        );
    }
    corpo
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

    fn emitir_nfce(&self, nota: &Nfce, csc: &Csc) -> Resultado<RetornoSefaz> {
        let mut corpo = self.corpo_base();
        corpo.extend(nfce_json(nota, csc));
        self.postar("/nfe/nfce/emitir", &Value::Object(corpo))
            .map(RespostaSefazApi::em_retorno)
            .map_err(|e| Erro::de_dominio(&e))
    }

    fn consultar_documento(
        &self,
        chave: &ChaveAcesso,
        uf: Uf,
        xml_assinado: Option<&str>,
    ) -> Resultado<RetornoSefaz> {
        let mut corpo = self.corpo_base();
        corpo.insert("chaveAcesso".into(), Value::from(chave.como_str()));
        corpo.insert("ufEmitente".into(), Value::from(uf.sigla()));
        if let Some(xml) = xml_assinado {
            corpo.insert("xmlAssinado".into(), Value::from(xml));
        }
        self.postar("/nfe/consultar", &Value::Object(corpo))
            .map(RespostaSefazApi::em_retorno)
            .map_err(|e| Erro::de_dominio(&e))
    }

    fn cancelar(
        &self,
        chave: &ChaveAcesso,
        protocolo: &str,
        justificativa: &str,
        uf: Uf,
    ) -> Resultado<RetornoSefaz> {
        let mut corpo = self.corpo_base();
        corpo.insert("chaveAcesso".into(), Value::from(chave.como_str()));
        corpo.insert("protocolo".into(), Value::from(protocolo));
        corpo.insert("justificativa".into(), Value::from(justificativa));
        corpo.insert("ufEmitente".into(), Value::from(uf.sigla()));
        self.postar("/nfe/cancelar", &Value::Object(corpo))
            .map(RespostaSefazApi::em_retorno)
            .map_err(|e| Erro::de_dominio(&e))
    }

    fn inutilizar(&self, p: &PedidoInutilizacao) -> Resultado<RetornoSefaz> {
        let mut corpo = self.corpo_base();
        corpo.insert("cnpj".into(), Value::from(p.cnpj.sem_mascara()));
        corpo.insert("modelo".into(), Value::from(p.modelo.codigo()));
        corpo.insert("serie".into(), Value::from(p.serie.to_string()));
        corpo.insert("numeroInicial".into(), Value::from(p.inicio));
        corpo.insert("numeroFinal".into(), Value::from(p.fim));
        corpo.insert("ano".into(), Value::from(format!("{:02}", p.ano % 100)));
        corpo.insert(
            "justificativa".into(),
            Value::from(p.justificativa.as_str()),
        );
        corpo.insert("ufEmitente".into(), Value::from(p.uf.sigla()));
        self.postar("/nfe/inutilizar", &Value::Object(corpo))
            .map(RespostaSefazApi::em_retorno)
            .map_err(|e| Erro::de_dominio(&e))
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
    fn nfce_vira_o_json_do_sefaz_api() {
        let nota = crate::emissao::exemplo();
        assert_eq!(nota.total(), cardeal_kernel::Dinheiro::centavos(6_900));
        let csc = Csc {
            id: "000001".into(),
            token: Segredo::novo("TOKEN".into()),
        };
        let j = Value::Object(nfce_json(&nota, &csc));
        assert_eq!(j["emitente"]["cnpj"], "11222333000181");
        assert_eq!(j["emitente"]["crt"], "1");
        assert_eq!(j["emitente"]["endereco"]["uf"], "RS");
        assert!(j["emitente"]["endereco"].get("complemento").is_none());
        assert_eq!(j["ufEmitente"], "RS");
        assert_eq!(j["serie"], "1");
        assert_eq!(j["numero"], 42);
        assert_eq!(j["csc"]["token"], "TOKEN");
        assert_eq!(j["consumidor"]["cpf"], "52998224725");
        let i1 = &j["itens"][0];
        assert_eq!(i1["numero"], 1);
        assert_eq!(i1["produto"]["quantidade"], 2.0);
        assert_eq!(i1["produto"]["valorUnitario"], 29.9);
        assert_eq!(i1["produto"]["valorTotal"], 59.8);
        assert_eq!(i1["produto"]["valorDesconto"], 5.8);
        assert_eq!(i1["produto"]["cEAN"], "7891000315507");
        assert_eq!(i1["imposto"]["icms"]["orig"], "0");
        assert_eq!(i1["imposto"]["icms"]["csosn"], "102");
        assert!(i1["imposto"]["icms"].get("modBC").is_none());
        assert_eq!(i1["imposto"]["pis"]["cst"], "49");
        assert_eq!(i1["imposto"]["pis"]["valorBC"], 0.0);
        let i2 = &j["itens"][1];
        assert_eq!(i2["produto"]["cEAN"], "SEM GTIN");
        assert_eq!(i2["produto"]["cest"], "1001200");
        assert!(i2["produto"].get("valorDesconto").is_none());
        let formas = &j["pagamento"]["formas"];
        assert_eq!(formas[0]["tipo"], "17");
        assert_eq!(
            formas[0]["cartao"]["tipoIntegracao"], "2",
            "Pix leva o grupo card"
        );
        assert_eq!(formas[1]["tipo"], "04");
        assert_eq!(formas[1]["cartao"]["tipoIntegracao"], "2");
        assert!(j["pagamento"].get("troco").is_none());
        assert_eq!(j["transporte"]["modFrete"], "9");
    }

    #[test]
    fn retorno_da_api_vira_situacao() {
        let r = |status: &str, codigo: &str| {
            RespostaSefazApi {
                status: status.into(),
                codigo_status: codigo.into(),
                ..RespostaSefazApi::default()
            }
            .em_retorno()
            .situacao
        };
        assert_eq!(r("autorizado", "100"), SituacaoRetorno::Autorizado);
        assert_eq!(r("rejeitado", "539"), SituacaoRetorno::Rejeitado);
        assert_eq!(r("erro", "400"), SituacaoRetorno::Rejeitado);
        assert_eq!(r("erro", "ERR"), SituacaoRetorno::Erro);
        assert_eq!(r("processando", "TIMEOUT"), SituacaoRetorno::Processando);
        assert_eq!(r("denegado", "302"), SituacaoRetorno::Denegado);
        // SP devolve rejeição com código abaixo de 200, que o sefaz-api chama de "erro".
        let sp = RespostaSefazApi {
            status: "erro".into(),
            codigo_status: "178".into(),
            motivo_status: "Rejeição: CNPJ do emitente não cadastrado na Receita Federal".into(),
            ..RespostaSefazApi::default()
        };
        assert_eq!(sp.em_retorno().situacao, SituacaoRetorno::Rejeitado);
        let completo: RespostaSefazApi = serde_json::from_str(
            r#"{"success":true,"status":"autorizado","codigoStatus":"100","motivoStatus":"Autorizado o uso da NF-e","chaveAcesso":"43261011222333000181650010000000421100000001","protocolo":"143260000000001","xml":"<nfeProc/>"}"#,
        )
        .unwrap();
        let retorno = completo.em_retorno();
        assert_eq!(retorno.protocolo.as_deref(), Some("143260000000001"));
        assert_eq!(retorno.xml.as_deref(), Some("<nfeProc/>"));
        assert_eq!(retorno.xml_assinado, None);
        // A emissão incerta devolve a nota assinada; a consulta da cancelada, o cancelamento.
        let incerta: RespostaSefazApi = serde_json::from_str(
            r#"{"success":false,"status":"erro","codigoStatus":"ERR","motivoStatus":"socket hang up","chaveAcesso":"43261011222333000181650010000000421100000001","xmlAssinado":"<NFe/>"}"#,
        )
        .unwrap();
        let incerta = incerta.em_retorno();
        assert_eq!(incerta.situacao, SituacaoRetorno::Erro);
        assert_eq!(incerta.xml_assinado.as_deref(), Some("<NFe/>"));
        let cancelada: RespostaSefazApi = serde_json::from_str(
            r#"{"success":false,"status":"cancelado","codigoStatus":"101","motivoStatus":"Cancelamento de NF-e homologado","protocolo":"143260000000001","xml":"","xmlCancelamento":"<procEventoNFe/>"}"#,
        )
        .unwrap();
        let cancelada = cancelada.em_retorno();
        assert_eq!(cancelada.situacao, SituacaoRetorno::Cancelado);
        assert_eq!(cancelada.xml, None, "XML vazio não é XML");
        assert_eq!(
            cancelada.xml_cancelamento.as_deref(),
            Some("<procEventoNFe/>")
        );
    }

    #[test]
    fn a_consulta_leva_o_xml_assinado_quando_ha() {
        let (url, tarefa) = servidor(
            200,
            r#"{"success":true,"status":"autorizado","codigoStatus":"100","motivoStatus":"Autorizado o uso da NF-e","xml":"<nfeProc/>"}"#,
        );
        let chave = ChaveAcesso::nova("43261011222333000181650010000000421100000001").unwrap();
        let r = cliente(&url)
            .consultar_documento(&chave, Uf::Rs, Some("<NFe>assinada</NFe>"))
            .unwrap();
        assert_eq!(r.xml.as_deref(), Some("<nfeProc/>"));
        let recebido = tarefa.join().unwrap();
        assert!(recebido.starts_with("POST /nfe/consultar "), "{recebido}");
        let corpo: Value = serde_json::from_str(recebido.rsplit('\n').next().unwrap()).unwrap();
        assert_eq!(corpo["chaveAcesso"], chave.como_str());
        assert_eq!(corpo["xmlAssinado"], "<NFe>assinada</NFe>");

        // Sem o XML, o campo nem vai.
        let (url, tarefa) = servidor(200, r#"{"success":true,"status":"autorizado"}"#);
        cliente(&url)
            .consultar_documento(&chave, Uf::Rs, None)
            .unwrap();
        let recebido = tarefa.join().unwrap();
        let corpo: Value = serde_json::from_str(recebido.rsplit('\n').next().unwrap()).unwrap();
        assert!(corpo.get("xmlAssinado").is_none());
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

    /// Uma NFC-e de homologação de verdade, contra o `sefaz-api` publicado: confere o caminho
    /// inteiro (JSON → XML → assinatura → SEFAZ). Além das variáveis do teste de status, pede
    /// `CARDEAL_TESTE_IE`, `CARDEAL_TESTE_IBGE`, `CARDEAL_TESTE_MUNICIPIO`, `CARDEAL_TESTE_CEP`,
    /// `CARDEAL_TESTE_CSC_ID` e `CARDEAL_TESTE_CSC_TOKEN`. Rodar com `--ignored --nocapture`.
    #[test]
    #[ignore = "emite uma NFC-e de homologação de verdade"]
    fn homologacao_nfce_real() {
        let var = |n: &str| std::env::var(n).unwrap_or_else(|_| panic!("{n}"));
        let config = ConfigApiFiscal::do_ambiente().expect("CARDEAL_FISCAL_TOKEN");
        let pfx = std::fs::read(var("CARDEAL_TESTE_PFX")).unwrap();
        let preparado =
            crate::preparar_certificado_a1(&pfx, &var("CARDEAL_TESTE_PFX_SENHA")).unwrap();
        let uf = Uf::de_sigla(&var("CARDEAL_TESTE_UF")).unwrap();
        let mut nota = crate::emissao::exemplo();
        nota.emitente.cnpj = preparado.info.cnpj.unwrap();
        nota.emitente.razao_social = preparado.info.titular.clone();
        nota.emitente.inscricao_estadual = var("CARDEAL_TESTE_IE");
        nota.emitente.endereco.uf = uf;
        nota.emitente.endereco.codigo_municipio = var("CARDEAL_TESTE_IBGE");
        nota.emitente.endereco.municipio = var("CARDEAL_TESTE_MUNICIPIO");
        nota.emitente.endereco.cep = var("CARDEAL_TESTE_CEP");
        nota.consumidor = None;
        for item in &mut nota.itens {
            item.cest = None;
        }
        nota.itens[0].descricao =
            "NOTA FISCAL EMITIDA EM AMBIENTE DE HOMOLOGACAO - SEM VALOR FISCAL".into();
        let agora = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        nota.numero = 900_000_000 + agora % 99_999_999;
        let csc = Csc {
            id: var("CARDEAL_TESTE_CSC_ID"),
            token: Segredo::novo(var("CARDEAL_TESTE_CSC_TOKEN")),
        };
        let api = ApiFiscalHttp::novo(
            Arc::new(config),
            CertificadoA1 {
                pfx: Segredo::novo(preparado.pfx),
                senha: Segredo::novo(var("CARDEAL_TESTE_PFX_SENHA")),
            },
            AmbienteFiscal::Homologacao,
        );
        let r = api.emitir_nfce(&nota, &csc).unwrap();
        eprintln!(
            "NFC-e {}: {:?} {} {} chave={:?} protocolo={:?} erros={:?}",
            nota.numero, r.situacao, r.codigo, r.motivo, r.chave, r.protocolo, r.erros
        );
        // Schema (215/225) ou recusa do provedor (400) = o Cardeal montou algo errado.
        assert!(
            !matches!(r.codigo.as_str(), "215" | "225" | "400"),
            "nota mal formada: {} {}",
            r.codigo,
            r.motivo
        );
        if let Some(chave) = &r.chave {
            let consulta = api
                .consultar_documento(&ChaveAcesso::nova(chave).unwrap(), uf, None)
                .unwrap();
            eprintln!(
                "consulta: {:?} {} {}",
                consulta.situacao, consulta.codigo, consulta.motivo
            );
        }
    }
}
