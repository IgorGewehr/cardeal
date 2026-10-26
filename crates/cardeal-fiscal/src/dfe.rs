//! A distribuição `DFe` para o destinatário — as notas de compra que a SEFAZ entrega ao CNPJ — e
//! a Manifestação do Destinatário: os tipos que atravessam a [`crate::PortaFiscal`] e a leitura
//! dos resumos (`resNFe`, `resEvento`).
//!
//! Como a SEFAZ entrega: a distribuição pagina por NSU (um número sequencial por CNPJ). A nota
//! de um fornecedor chega primeiro como **resumo** (`resNFe`: emitente, valor, situação); a
//! **Ciência da Operação** (evento 210210) libera a nota completa, que chega por um NSU novo ou
//! pela consulta por chave. Quem pede de novo antes da hora toma 656 ("consumo indevido") e
//! espera uma hora.

use cardeal_kernel::{Data, Dinheiro};
use serde::{Deserialize, Serialize};

use crate::chave::ChaveAcesso;
use crate::erros::ErroFiscal;
use crate::porta::Nsu;

/// O que a distribuição trouxe num documento.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TipoDfe {
    /// O resumo da NF-e (`resNFe`) — a nota completa vem depois da Ciência da Operação.
    ResumoDaNota,
    /// A NF-e completa (`nfeProc`).
    Nota,
    /// O resumo de um evento (`resEvento`) — o cancelamento pelo emitente, por exemplo.
    ResumoDeEvento,
    /// Um evento completo (`procEventoNFe`).
    Evento,
    /// Outro documento (CT-e, MDF-e...).
    Outro,
}

/// Um documento da distribuição.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentoDfe {
    /// O NSU dele.
    pub nsu: Nsu,
    /// O que é.
    pub tipo: TipoDfe,
    /// A chave da NF-e a que se refere.
    pub chave: Option<ChaveAcesso>,
    /// O XML, descompactado.
    pub xml: String,
}

/// Uma página da distribuição (até 50 documentos).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PaginaDfe {
    /// A SEFAZ barrou por consulta repetida (656): esperar uma hora. Os NSUs não valem.
    pub consumo_indevido: bool,
    /// O último NSU pesquisado — o próximo pedido parte dele.
    pub ultimo_nsu: Nsu,
    /// O maior NSU que existe para o CNPJ: alcançado, pedir de novo só depois de uma hora.
    pub maior_nsu: Nsu,
    /// Os documentos.
    pub documentos: Vec<DocumentoDfe>,
}

/// A Manifestação do Destinatário.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Manifestacao {
    /// 210210 — Ciência da Operação: a empresa sabe da nota (libera o XML completo).
    Ciencia,
    /// 210200 — Confirmação da Operação: a mercadoria chegou.
    Confirmacao,
    /// 210220 — Desconhecimento da Operação: a empresa não fez essa compra.
    Desconhecimento,
    /// 210240 — Operação não Realizada (com justificativa).
    NaoRealizada,
}

impl Manifestacao {
    /// O nome no `sefaz-api`.
    #[must_use]
    pub const fn na_api(self) -> &'static str {
        match self {
            Self::Ciencia => "ciencia",
            Self::Confirmacao => "confirmacao",
            Self::Desconhecimento => "desconhecimento",
            Self::NaoRealizada => "nao_realizada",
        }
    }

    /// O código do evento.
    #[must_use]
    pub const fn evento(self) -> &'static str {
        match self {
            Self::Ciencia => "210210",
            Self::Confirmacao => "210200",
            Self::Desconhecimento => "210220",
            Self::NaoRealizada => "210240",
        }
    }
}

/// A situação da nota no resumo (`cSitNFe`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SituacaoDoResumo {
    /// 1 — autorizada.
    Autorizada,
    /// 2 — denegada.
    Denegada,
    /// 3 — cancelada.
    Cancelada,
}

impl SituacaoDoResumo {
    /// O código.
    #[must_use]
    pub const fn codigo(self) -> u8 {
        match self {
            Self::Autorizada => 1,
            Self::Denegada => 2,
            Self::Cancelada => 3,
        }
    }

    /// O contrário de [`Self::codigo`].
    #[must_use]
    pub const fn de_codigo(c: u8) -> Option<Self> {
        match c {
            1 => Some(Self::Autorizada),
            2 => Some(Self::Denegada),
            3 => Some(Self::Cancelada),
            _ => None,
        }
    }
}

/// O resumo de uma NF-e (`resNFe`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumoNfe {
    /// A chave.
    pub chave: ChaveAcesso,
    /// CNPJ (ou CPF) do emitente, só dígitos.
    pub emitente_documento: String,
    /// Nome do emitente.
    pub emitente_nome: String,
    /// O dia da emissão.
    pub emissao: Option<Data>,
    /// O valor da nota.
    pub valor: Dinheiro,
    /// A situação.
    pub situacao: SituacaoDoResumo,
}

/// O resumo de um evento (`resEvento`) ou o evento completo (`procEventoNFe`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventoDfe {
    /// A chave da nota.
    pub chave: ChaveAcesso,
    /// O código do evento (`110111` = cancelamento).
    pub tipo: String,
    /// A descrição.
    pub descricao: String,
}

impl EventoDfe {
    /// O emitente cancelou a nota (110111, ou 110112 por substituição).
    #[must_use]
    pub fn e_cancelamento(&self) -> bool {
        matches!(self.tipo.as_str(), "110111" | "110112")
    }
}

/// O texto do primeiro `<tag>` (ou `<tag ...>`), com as entidades do XML resolvidas.
fn texto(xml: &str, tag: &str) -> Option<String> {
    let mut resto = xml;
    loop {
        let i = resto.find(&format!("<{tag}"))?;
        let depois = &resto[i + tag.len() + 1..];
        match depois.chars().next() {
            Some('>' | ' ') => {
                let ini = depois.find('>')? + 1;
                let fim = depois[ini..].find('<')?;
                return Some(desescapar(depois[ini..ini + fim].trim()));
            }
            _ => resto = depois,
        }
    }
}

fn desescapar(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn data_iso(s: &str) -> Option<Data> {
    let n = |a: usize, b: usize| s.get(a..b)?.parse::<u32>().ok();
    Data::de_ymd(i32::try_from(n(0, 4)?).ok()?, n(5, 7)?, n(8, 10)?).ok()
}

fn chave_de(xml: &str) -> Result<ChaveAcesso, ErroFiscal> {
    let ch = texto(xml, "chNFe").ok_or_else(|| ErroFiscal::XmlInvalido("sem chNFe".into()))?;
    ChaveAcesso::nova(&ch).map_err(|e| ErroFiscal::XmlInvalido(e.to_string()))
}

/// Lê um `resNFe`.
///
/// # Errors
/// [`ErroFiscal::XmlInvalido`] sem chave, valor ou situação.
pub fn ler_resumo_nfe(xml: &str) -> Result<ResumoNfe, ErroFiscal> {
    let chave = chave_de(xml)?;
    let valor = texto(xml, "vNF")
        .and_then(|v| Dinheiro::de_str(&v).ok())
        .ok_or_else(|| ErroFiscal::XmlInvalido("resNFe sem vNF".into()))?;
    let situacao = texto(xml, "cSitNFe")
        .and_then(|c| c.parse::<u8>().ok())
        .and_then(SituacaoDoResumo::de_codigo)
        .ok_or_else(|| ErroFiscal::XmlInvalido("resNFe sem cSitNFe".into()))?;
    Ok(ResumoNfe {
        emitente_documento: texto(xml, "CNPJ")
            .or_else(|| texto(xml, "CPF"))
            .unwrap_or_default(),
        emitente_nome: texto(xml, "xNome").unwrap_or_default(),
        emissao: texto(xml, "dhEmi").as_deref().and_then(data_iso),
        chave,
        valor,
        situacao,
    })
}

/// Lê um `resEvento` ou `procEventoNFe`.
///
/// # Errors
/// [`ErroFiscal::XmlInvalido`] sem chave ou sem tipo de evento.
pub fn ler_evento(xml: &str) -> Result<EventoDfe, ErroFiscal> {
    Ok(EventoDfe {
        chave: chave_de(xml)?,
        tipo: texto(xml, "tpEvento")
            .ok_or_else(|| ErroFiscal::XmlInvalido("evento sem tpEvento".into()))?,
        descricao: texto(xml, "xEvento")
            .or_else(|| texto(xml, "descEvento"))
            .unwrap_or_default(),
    })
}

#[cfg(test)]
mod testes {
    use super::*;

    const CHAVE: &str = "43261011222333000181550010000001231000000010";

    #[test]
    fn le_o_resumo_da_nota() {
        let xml = format!(
            "<resNFe xmlns=\"http://www.portalfiscal.inf.br/nfe\" versao=\"1.01\"><chNFe>{CHAVE}</chNFe>\
             <CNPJ>11222333000181</CNPJ><xNome>FORNECEDOR &amp; CIA LTDA</xNome><IE>0960000000</IE>\
             <dhEmi>2026-10-25T09:00:00-03:00</dhEmi><tpNF>1</tpNF><vNF>150.40</vNF>\
             <digVal>abc=</digVal><dhRecbto>2026-10-25T09:00:05-03:00</dhRecbto>\
             <nProt>143260000000001</nProt><cSitNFe>1</cSitNFe></resNFe>"
        );
        let r = ler_resumo_nfe(&xml).unwrap();
        assert_eq!(r.chave.como_str(), CHAVE);
        assert_eq!(r.emitente_documento, "11222333000181");
        assert_eq!(r.emitente_nome, "FORNECEDOR & CIA LTDA");
        assert_eq!(r.emissao, Some(Data::de_ymd(2026, 10, 25).unwrap()));
        assert_eq!(r.valor, Dinheiro::centavos(15_040));
        assert_eq!(r.situacao, SituacaoDoResumo::Autorizada);
        assert!(ler_resumo_nfe("<resNFe/>").is_err());
    }

    #[test]
    fn le_o_evento_e_reconhece_o_cancelamento() {
        let xml = format!(
            "<resEvento versao=\"1.01\"><cOrgao>43</cOrgao><CNPJ>11222333000181</CNPJ>\
             <chNFe>{CHAVE}</chNFe><dhEvento>2026-10-25T10:00:00-03:00</dhEvento>\
             <tpEvento>110111</tpEvento><nSeqEvento>1</nSeqEvento><xEvento>Cancelamento</xEvento>\
             </resEvento>"
        );
        let e = ler_evento(&xml).unwrap();
        assert!(e.e_cancelamento());
        assert_eq!(e.descricao, "Cancelamento");
        // A tag mais curta não confunde a mais longa (tpEvento x tpEventoX).
        assert_eq!(
            texto("<tpEventoX>9</tpEventoX><tpEvento>1</tpEvento>", "tpEvento").as_deref(),
            Some("1")
        );
    }
}
