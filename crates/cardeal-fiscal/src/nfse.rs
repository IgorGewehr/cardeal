//! A **NFS-e** (Padrão Nacional, DPS) — a nota do serviço prestado. O `sefaz-api` roteia pelo
//! município (Nacional/ADN, Betha, São Paulo); daqui sai o pedido no formato DPS (`numeroDPS`,
//! `serie`, `codigoMunicipioEmissao`, `issqn`) e volta o número, a chave de 50 dígitos (que o
//! cancelamento exige), o código de verificação e o XML.
//!
//! No Simples Nacional o ISS é apurado no DAS: o DPS não leva alíquota de ISS, e o percentual
//! informado vai como `pTotTribSN` — o percentual aproximado dos tributos do Simples. No regime
//! normal é a alíquota do ISS do município.

use cardeal_kernel::{Data, Dinheiro, Percentual};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::emissao::{DocumentoConsumidor, Emitente, EnderecoEmitente};

/// Quem toma o serviço.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TomadorNfse {
    /// CPF ou CNPJ (sem documento: o consumidor que não se identifica).
    pub documento: Option<DocumentoConsumidor>,
    /// O nome.
    pub nome: String,
    /// O endereço, quando cadastrado (com o código IBGE do município).
    pub endereco: Option<EnderecoEmitente>,
    /// O e-mail.
    pub email: Option<String>,
}

/// Uma NFS-e pronta para emitir — sem certificado.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Nfse {
    /// O prestador (a empresa): CNPJ, inscrição municipal, endereço com o código do município.
    pub prestador: Emitente,
    /// O tomador.
    pub tomador: TomadorNfse,
    /// O código de tributação nacional (`cTribNac`, 6 dígitos — ex.: `140101`).
    pub codigo_tributacao_nacional: String,
    /// O item da lista da LC 116/2003 (`14.01`).
    pub item_lc116: String,
    /// O código de tributação do município, quando ele pede.
    pub codigo_tributacao_municipal: Option<String>,
    /// O código NBS (9 dígitos), quando informado.
    pub nbs: Option<String>,
    /// A discriminação do serviço.
    pub discriminacao: String,
    /// O valor dos serviços.
    pub valor_servicos: Dinheiro,
    /// O desconto incondicionado.
    pub desconto: Dinheiro,
    /// No Simples: o percentual aproximado dos tributos (`pTotTribSN`); no regime normal: a
    /// alíquota do ISS.
    pub aliquota: Percentual,
    /// O ISS é retido pelo tomador.
    pub iss_retido: bool,
    /// A série do DPS.
    pub serie: u32,
    /// O número do DPS.
    pub numero: u64,
    /// A competência (`dCompet`).
    pub competencia: Data,
    /// Informações complementares.
    pub informacoes: Option<String>,
}

/// Em que deu o pedido.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SituacaoNfse {
    /// Autorizada (a NFS-e existe).
    Autorizada,
    /// Recusada (o motivo diz por quê) — ou, na consulta, não encontrada.
    Rejeitada,
    /// Em processamento (provedores municipais assíncronos).
    Processando,
    /// Cancelada.
    Cancelada,
    /// Sem resposta que decida (erro do provedor, rede).
    Erro,
}

/// A resposta de um pedido de NFS-e.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetornoNfse {
    /// Em que deu.
    pub situacao: SituacaoNfse,
    /// O número da NFS-e.
    pub numero: Option<u64>,
    /// A chave de acesso (50 dígitos).
    pub chave: Option<String>,
    /// O código de verificação.
    pub codigo_verificacao: Option<String>,
    /// O XML da NFS-e (ou do evento, no cancelamento).
    pub xml: Option<String>,
    /// As mensagens do provedor ("código — mensagem").
    pub mensagens: Vec<String>,
}

impl RetornoNfse {
    /// As mensagens numa frase.
    #[must_use]
    pub fn motivo(&self) -> String {
        if self.mensagens.is_empty() {
            match self.situacao {
                SituacaoNfse::Autorizada => "NFS-e autorizada".into(),
                SituacaoNfse::Cancelada => "NFS-e cancelada".into(),
                SituacaoNfse::Processando => "Em processamento no município".into(),
                SituacaoNfse::Rejeitada => "Recusada pelo município".into(),
                SituacaoNfse::Erro => "Sem resposta do município".into(),
            }
        } else {
            self.mensagens.join("; ")
        }
    }

    /// A consulta disse que o DPS não existe (404): a emissão pode ser enviada de novo.
    #[must_use]
    pub fn nao_encontrada(&self) -> bool {
        self.situacao == SituacaoNfse::Rejeitada
            && self.mensagens.iter().any(|m| m.starts_with("404"))
    }
}

/// Por que se cancela (o código do evento).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MotivoCancelamentoNfse {
    /// 1 — erro na emissão.
    ErroNaEmissao,
    /// 2 — o serviço não foi prestado.
    ServicoNaoPrestado,
    /// 3 — nota em duplicidade.
    Duplicidade,
    /// 4 — erro de processamento.
    ErroDeProcessamento,
}

impl MotivoCancelamentoNfse {
    /// O código.
    #[must_use]
    pub const fn codigo(self) -> &'static str {
        match self {
            Self::ErroNaEmissao => "1",
            Self::ServicoNaoPrestado => "2",
            Self::Duplicidade => "3",
            Self::ErroDeProcessamento => "4",
        }
    }

    /// Para a tela.
    #[must_use]
    pub const fn rotulo(self) -> &'static str {
        match self {
            Self::ErroNaEmissao => "Erro na emissão",
            Self::ServicoNaoPrestado => "Serviço não prestado",
            Self::Duplicidade => "Nota em duplicidade",
            Self::ErroDeProcessamento => "Erro de processamento",
        }
    }
}

/// O cancelamento de uma NFS-e.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelamentoNfse {
    /// A chave de acesso.
    pub chave: String,
    /// O número da NFS-e.
    pub numero: u64,
    /// O motivo (código).
    pub motivo: MotivoCancelamentoNfse,
    /// A justificativa.
    pub justificativa: String,
}

fn reais(d: Dinheiro) -> f64 {
    // centavos → reais, sem perder casa (o JSON leva número).
    #[allow(clippy::cast_precision_loss)]
    let v = d.em_centavos() as f64 / 100.0;
    v
}

fn percentual(p: Percentual) -> f64 {
    p.formatar(4).replace(',', ".").parse().unwrap_or(0.0)
}

fn endereco_json(e: &EnderecoEmitente) -> Value {
    let mut v = json!({
        "logradouro": e.logradouro,
        "numero": e.numero,
        "bairro": e.bairro,
        "codigoMunicipio": e.codigo_municipio,
        "uf": e.uf.sigla(),
        "cep": e.cep,
    });
    if !e.complemento.is_empty() {
        v["complemento"] = Value::from(e.complemento.as_str());
    }
    v
}

/// O prestador no formato do `sefaz-api` (o necessário para o DPS e para rotear).
pub(crate) fn prestador_json(p: &Emitente) -> Value {
    let simples = p.crt.e_simples();
    let mut v = json!({
        "cnpj": p.cnpj.sem_mascara(),
        "inscricaoMunicipal": p.inscricao_municipal.clone().unwrap_or_default(),
        "nome": p.razao_social,
        "nomeFantasia": p.nome_fantasia,
        "endereco": endereco_json(&p.endereco),
        "optanteSimplesNacional": simples,
    });
    if simples {
        // Tributos federais e o ISS apurados pelo Simples (no DAS).
        v["regApTribSN"] = Value::from(1);
    }
    v
}

/// O corpo do `POST /nfse/emitir` (sem certificado e ambiente, que o adaptador põe).
pub(crate) fn nfse_json(n: &Nfse) -> serde_json::Map<String, Value> {
    let municipio = n.prestador.endereco.codigo_municipio.clone();
    let mut tomador = json!({ "nome": n.tomador.nome });
    match &n.tomador.documento {
        Some(DocumentoConsumidor::Cpf(c)) => tomador["cpf"] = Value::from(c.sem_mascara()),
        Some(DocumentoConsumidor::Cnpj(c)) => tomador["cnpj"] = Value::from(c.sem_mascara()),
        None => {}
    }
    if let Some(e) = &n.tomador.endereco {
        tomador["endereco"] = endereco_json(e);
    }
    if let Some(email) = &n.tomador.email {
        tomador["contato"] = json!({ "email": email });
    }
    let mut servico = json!({
        "codigoTributacaoNacional": n.codigo_tributacao_nacional,
        "codigoServico": n.item_lc116,
        "discriminacao": n.discriminacao,
        "localPrestacao": { "codigoMunicipio": municipio },
    });
    if let Some(c) = &n.codigo_tributacao_municipal {
        servico["codigoTributacaoMunicipal"] = Value::from(c.as_str());
    }
    if let Some(nbs) = &n.nbs {
        servico["nbs"] = Value::from(nbs.as_str());
    }
    let mut valores = json!({ "valorServicos": reais(n.valor_servicos) });
    if !n.desconto.e_zero() {
        valores["valorDescontoIncondicionado"] = Value::from(reais(n.desconto));
    }
    let mut corpo = serde_json::Map::new();
    corpo.insert("prestador".into(), prestador_json(&n.prestador));
    corpo.insert("tomador".into(), tomador);
    corpo.insert("servico".into(), servico);
    corpo.insert("valores".into(), valores);
    corpo.insert(
        "issqn".into(),
        json!({
            "aliquota": percentual(n.aliquota),
            "tipoRetencaoISSQN": if n.iss_retido { "2" } else { "1" },
        }),
    );
    corpo.insert("numeroDPS".into(), Value::from(n.numero));
    corpo.insert("serie".into(), Value::from(n.serie.to_string()));
    corpo.insert("codigoMunicipioEmissao".into(), Value::from(municipio));
    corpo.insert(
        "rps".into(),
        json!({ "dataEmissao": n.competencia.formatar_iso() }),
    );
    if let Some(i) = &n.informacoes {
        corpo.insert("informacoesComplementares".into(), Value::from(i.as_str()));
    }
    corpo
}

/// A resposta do `sefaz-api` para a NFS-e (`NfseResponse`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct RespostaNfse {
    pub status: String,
    pub numero_nfse: Option<u64>,
    pub chave_acesso: Option<String>,
    pub codigo_verificacao: Option<String>,
    pub xml: Option<String>,
    pub mensagens: Vec<MensagemNfse>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub(crate) struct MensagemNfse {
    pub codigo: String,
    pub mensagem: String,
    pub correcao: Option<String>,
}

impl RespostaNfse {
    pub(crate) fn em_retorno(self) -> RetornoNfse {
        let situacao = match self.status.as_str() {
            "autorizado" => SituacaoNfse::Autorizada,
            "rejeitado" => SituacaoNfse::Rejeitada,
            "processando" => SituacaoNfse::Processando,
            "cancelado" => SituacaoNfse::Cancelada,
            _ => SituacaoNfse::Erro,
        };
        RetornoNfse {
            situacao,
            numero: self.numero_nfse,
            chave: self.chave_acesso.filter(|c| !c.is_empty()),
            codigo_verificacao: self.codigo_verificacao.filter(|c| !c.is_empty()),
            xml: self.xml.filter(|x| !x.is_empty()),
            mensagens: self
                .mensagens
                .into_iter()
                .map(|m| {
                    let mut t = if m.codigo.is_empty() || m.codigo == "0" {
                        m.mensagem
                    } else {
                        format!("{} — {}", m.codigo, m.mensagem)
                    };
                    if let Some(c) = m.correcao.filter(|c| !c.is_empty()) {
                        t = format!("{t} ({c})");
                    }
                    t
                })
                .collect(),
        }
    }
}

#[cfg(test)]
pub(crate) mod testes {
    use cardeal_kernel::{Cnpj, Cpf, Uf};

    use super::*;
    use crate::emissao::Crt;

    pub(crate) fn exemplo() -> Nfse {
        let endereco = EnderecoEmitente {
            logradouro: "Rua Sinimbu".into(),
            numero: "100".into(),
            complemento: String::new(),
            bairro: "Centro".into(),
            codigo_municipio: "4305108".into(),
            municipio: "Caxias do Sul".into(),
            uf: Uf::Rs,
            cep: "95020000".into(),
            telefone: None,
        };
        Nfse {
            prestador: Emitente {
                cnpj: Cnpj::novo("11222333000181").unwrap(),
                razao_social: "ASSISTENCIA TECNICA LTDA".into(),
                nome_fantasia: "Assistência".into(),
                inscricao_estadual: "0960000000".into(),
                inscricao_municipal: Some("12345".into()),
                crt: Crt::SimplesNacional,
                endereco: endereco.clone(),
            },
            tomador: TomadorNfse {
                documento: Some(DocumentoConsumidor::Cpf(Cpf::novo("52998224725").unwrap())),
                nome: "João Silva".into(),
                endereco: Some(endereco),
                email: Some("joao@exemplo.com".into()),
            },
            codigo_tributacao_nacional: "140101".into(),
            item_lc116: "14.01".into(),
            codigo_tributacao_municipal: None,
            nbs: None,
            discriminacao: "OS 42 — Notebook Dell: troca de bateria".into(),
            valor_servicos: Dinheiro::reais(150),
            desconto: Dinheiro::ZERO,
            aliquota: Percentual::de_str("6").unwrap(),
            iss_retido: false,
            serie: 900,
            numero: 7,
            competencia: Data::de_ymd(2026, 10, 28).unwrap(),
            informacoes: None,
        }
    }

    #[test]
    fn o_corpo_do_dps() {
        let c = Value::Object(nfse_json(&exemplo()));
        assert_eq!(c["prestador"]["cnpj"], "11222333000181");
        assert_eq!(c["prestador"]["inscricaoMunicipal"], "12345");
        assert_eq!(c["prestador"]["optanteSimplesNacional"], true);
        assert_eq!(c["prestador"]["regApTribSN"], 1);
        assert_eq!(c["tomador"]["cpf"], "52998224725");
        assert_eq!(c["tomador"]["endereco"]["codigoMunicipio"], "4305108");
        assert_eq!(c["tomador"]["contato"]["email"], "joao@exemplo.com");
        assert_eq!(c["servico"]["codigoTributacaoNacional"], "140101");
        assert_eq!(c["servico"]["codigoServico"], "14.01");
        assert_eq!(c["servico"]["localPrestacao"]["codigoMunicipio"], "4305108");
        assert_eq!(c["valores"]["valorServicos"], 150.0);
        assert!(c["valores"].get("valorDescontoIncondicionado").is_none());
        assert_eq!(c["issqn"]["aliquota"], 6.0);
        assert_eq!(c["issqn"]["tipoRetencaoISSQN"], "1");
        assert_eq!(c["numeroDPS"], 7);
        assert_eq!(c["serie"], "900");
        assert_eq!(c["codigoMunicipioEmissao"], "4305108");
        assert_eq!(c["rps"]["dataEmissao"], "2026-10-28");
    }

    #[test]
    fn a_resposta() {
        let r: RespostaNfse = serde_json::from_str(
            r#"{"success":true,"status":"autorizado","numeroNfse":42,"chaveAcesso":"43051082112233000181000000000004226100000000000017","codigoVerificacao":"ABC","xml":"<NFSe/>"}"#,
        )
        .unwrap();
        let r = r.em_retorno();
        assert_eq!(r.situacao, SituacaoNfse::Autorizada);
        assert_eq!(r.numero, Some(42));
        assert_eq!(r.chave.as_deref().map(str::len), Some(50));
        let nao: RespostaNfse = serde_json::from_str(
            r#"{"success":false,"status":"rejeitado","mensagens":[{"codigo":"404","mensagem":"NFS-e não encontrada"}]}"#,
        )
        .unwrap();
        let nao = nao.em_retorno();
        assert!(nao.nao_encontrada());
        assert_eq!(nao.motivo(), "404 — NFS-e não encontrada");
        let erro: RespostaNfse = serde_json::from_str(
            r#"{"success":false,"status":"erro","mensagens":[{"codigo":"0","mensagem":"Inscrição Municipal do prestador é obrigatória","correcao":"Cadastre"}]}"#,
        )
        .unwrap();
        assert_eq!(
            erro.em_retorno().motivo(),
            "Inscrição Municipal do prestador é obrigatória (Cadastre)"
        );
    }
}
