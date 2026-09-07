//! Leitura dos campos essenciais de um XML de NF-e 4.00 — **não** o esquema completo (sem
//! impostos discriminados, sem transporte, sem duplicatas): só o que `mod-compras` precisa
//! para montar a `NotaEntrada` em conferência (`docs/modulos/compras.md` §3).
//!
//! Feito com `quick-xml` em modo evento — sem validar contra o XSD oficial. Um XML fora do
//! padrão (tag essencial ausente, chave malformada) vira [`ErroFiscal::XmlInvalido`], nunca
//! panic.

use cardeal_kernel::{Cnpj, Data, Dinheiro, Preco, Quantidade};
use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::chave::ChaveAcesso;
use crate::erros::ErroFiscal;

/// Um item (`det/prod`) de uma NF-e.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemNfe {
    /// O código do produto no cadastro do fornecedor (`cProd`).
    pub codigo_fornecedor: String,
    /// A descrição do fornecedor (`xProd`).
    pub descricao: String,
    /// O NCM (8 dígitos).
    pub ncm: String,
    /// A quantidade comercial (`qCom`).
    pub quantidade: Quantidade,
    /// O valor unitário comercial (`vUnCom`).
    pub valor_unitario: Preco,
    /// O valor total do item (`vProd`).
    pub valor_total: Dinheiro,
}

/// Os campos essenciais de uma NF-e, extraídos do XML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotaFiscalXml {
    /// A chave de acesso (atributo `Id` de `infNFe`, sem o prefixo `"NFe"`).
    pub chave_acesso: ChaveAcesso,
    /// Número da nota (`ide/nNF`).
    pub numero: String,
    /// Série (`ide/serie`).
    pub serie: String,
    /// Data de emissão (`ide/dhEmi`, só a parte de data).
    pub emissao: Data,
    /// CNPJ do emitente (`emit/CNPJ`).
    pub fornecedor_cnpj: Cnpj,
    /// Razão social do emitente (`emit/xNome`).
    pub fornecedor_nome: String,
    /// Os itens (`det/prod`), na ordem do XML.
    pub itens: Vec<ItemNfe>,
    /// Valor do frete (`total/ICMSTot/vFrete`).
    pub valor_frete: Dinheiro,
    /// Valor do seguro (`total/ICMSTot/vSeg`).
    pub valor_seguro: Dinheiro,
    /// Outras despesas acessórias (`total/ICMSTot/vOutro`).
    pub valor_outras_despesas: Dinheiro,
    /// Valor total da nota (`total/ICMSTot/vNF`).
    pub valor_total: Dinheiro,
}

fn campo_ausente(nome: &str) -> ErroFiscal {
    ErroFiscal::XmlInvalido(format!("campo obrigatório ausente: {nome}"))
}

fn dinheiro_de(texto: &str, campo: &str) -> Result<Dinheiro, ErroFiscal> {
    Dinheiro::de_str(texto)
        .map_err(|_| ErroFiscal::XmlInvalido(format!("{campo} inválido: {texto}")))
}

fn data_de_dh_emi(dh_emi: &str) -> Result<Data, ErroFiscal> {
    let erro = || ErroFiscal::XmlInvalido(format!("dhEmi inválido: {dh_emi}"));
    let data = dh_emi.get(0..10).ok_or_else(erro)?;
    let ano: i32 = data
        .get(0..4)
        .and_then(|s| s.parse().ok())
        .ok_or_else(erro)?;
    let mes: u32 = data
        .get(5..7)
        .and_then(|s| s.parse().ok())
        .ok_or_else(erro)?;
    let dia: u32 = data
        .get(8..10)
        .and_then(|s| s.parse().ok())
        .ok_or_else(erro)?;
    Data::de_ymd(ano, mes, dia).map_err(|_| erro())
}

/// Interpreta o XML de uma NF-e, extraindo só os campos essenciais.
///
/// # Errors
/// [`ErroFiscal::XmlInvalido`] se o XML não puder ser lido ou faltar um campo obrigatório.
pub fn interpretar(xml: &str) -> Result<NotaFiscalXml, ErroFiscal> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut chave_acesso = None;
    let mut numero = None;
    let mut serie = None;
    let mut emissao = None;
    let mut fornecedor_cnpj = None;
    let mut fornecedor_nome = None;
    let mut valor_frete = Dinheiro::ZERO;
    let mut valor_seguro = Dinheiro::ZERO;
    let mut valor_outras_despesas = Dinheiro::ZERO;
    let mut valor_total = None;
    let mut itens = Vec::new();

    // Pilha de nomes de tag abertos — dá contexto para tags de nome repetido em seções
    // diferentes (nenhuma colide aqui, mas a pilha deixa o parser honesto sobre onde está).
    let mut pilha: Vec<String> = Vec::new();
    let mut item_atual: Option<ItemParcial> = None;

    loop {
        match reader
            .read_event()
            .map_err(|e| ErroFiscal::XmlInvalido(e.to_string()))?
        {
            Event::Eof => break,
            Event::Start(e) => {
                let nome = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if nome == "infNFe" {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"Id" {
                            let valor = attr
                                .decode_and_unescape_value(reader.decoder())
                                .unwrap_or_default();
                            let sem_prefixo = valor.strip_prefix("NFe").unwrap_or(&valor);
                            chave_acesso = Some(ChaveAcesso::nova(sem_prefixo)?);
                        }
                    }
                }
                if nome == "det" {
                    item_atual = Some(ItemParcial::default());
                }
                pilha.push(nome);
            }
            Event::End(e) => {
                let nome = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if nome == "det" {
                    if let Some(parcial) = item_atual.take() {
                        itens.push(parcial.finalizar()?);
                    }
                }
                pilha.pop();
            }
            Event::Text(t) => {
                let texto = t.unescape().unwrap_or_default().into_owned();
                if texto.trim().is_empty() {
                    continue;
                }
                let caminho: Vec<&str> = pilha.iter().map(String::as_str).collect();
                processar_texto(
                    &caminho,
                    &texto,
                    &mut numero,
                    &mut serie,
                    &mut emissao,
                    &mut fornecedor_cnpj,
                    &mut fornecedor_nome,
                    &mut valor_frete,
                    &mut valor_seguro,
                    &mut valor_outras_despesas,
                    &mut valor_total,
                    &mut item_atual,
                )?;
            }
            // Inclui `Event::Empty` (`<tag/>` autofechada): nenhuma tag deste XML que nos
            // importa é vazia (todas carregam texto), então só ignoramos — e nunca empilha,
            // porque uma tag autofechada não tem `Event::End` para desempilhar depois.
            _ => {}
        }
    }

    Ok(NotaFiscalXml {
        chave_acesso: chave_acesso.ok_or_else(|| campo_ausente("infNFe/Id"))?,
        numero: numero.ok_or_else(|| campo_ausente("ide/nNF"))?,
        serie: serie.ok_or_else(|| campo_ausente("ide/serie"))?,
        emissao: emissao.ok_or_else(|| campo_ausente("ide/dhEmi"))?,
        fornecedor_cnpj: fornecedor_cnpj.ok_or_else(|| campo_ausente("emit/CNPJ"))?,
        fornecedor_nome: fornecedor_nome.ok_or_else(|| campo_ausente("emit/xNome"))?,
        itens,
        valor_frete,
        valor_seguro,
        valor_outras_despesas,
        valor_total: valor_total.ok_or_else(|| campo_ausente("total/ICMSTot/vNF"))?,
    })
}

#[derive(Default)]
struct ItemParcial {
    codigo_fornecedor: Option<String>,
    descricao: Option<String>,
    ncm: Option<String>,
    quantidade: Option<Quantidade>,
    valor_unitario: Option<Preco>,
    valor_total: Option<Dinheiro>,
}

impl ItemParcial {
    fn finalizar(self) -> Result<ItemNfe, ErroFiscal> {
        Ok(ItemNfe {
            codigo_fornecedor: self
                .codigo_fornecedor
                .ok_or_else(|| campo_ausente("det/prod/cProd"))?,
            descricao: self
                .descricao
                .ok_or_else(|| campo_ausente("det/prod/xProd"))?,
            ncm: self.ncm.ok_or_else(|| campo_ausente("det/prod/NCM"))?,
            quantidade: self
                .quantidade
                .ok_or_else(|| campo_ausente("det/prod/qCom"))?,
            valor_unitario: self
                .valor_unitario
                .ok_or_else(|| campo_ausente("det/prod/vUnCom"))?,
            valor_total: self
                .valor_total
                .ok_or_else(|| campo_ausente("det/prod/vProd"))?,
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn processar_texto(
    caminho: &[&str],
    texto: &str,
    numero: &mut Option<String>,
    serie: &mut Option<String>,
    emissao: &mut Option<Data>,
    fornecedor_cnpj: &mut Option<Cnpj>,
    fornecedor_nome: &mut Option<String>,
    valor_frete: &mut Dinheiro,
    valor_seguro: &mut Dinheiro,
    valor_outras_despesas: &mut Dinheiro,
    valor_total: &mut Option<Dinheiro>,
    item_atual: &mut Option<ItemParcial>,
) -> Result<(), ErroFiscal> {
    match caminho {
        [.., "ide", "nNF"] => *numero = Some(texto.to_string()),
        [.., "ide", "serie"] => *serie = Some(texto.to_string()),
        [.., "ide", "dhEmi"] => *emissao = Some(data_de_dh_emi(texto)?),
        [.., "emit", "CNPJ"] => {
            *fornecedor_cnpj = Some(
                Cnpj::novo(texto)
                    .map_err(|_| ErroFiscal::XmlInvalido(format!("CNPJ inválido: {texto}")))?,
            );
        }
        [.., "emit", "xNome"] => *fornecedor_nome = Some(texto.to_string()),
        [.., "ICMSTot", "vFrete"] => *valor_frete = dinheiro_de(texto, "vFrete")?,
        [.., "ICMSTot", "vSeg"] => *valor_seguro = dinheiro_de(texto, "vSeg")?,
        [.., "ICMSTot", "vOutro"] => *valor_outras_despesas = dinheiro_de(texto, "vOutro")?,
        [.., "ICMSTot", "vNF"] => *valor_total = Some(dinheiro_de(texto, "vNF")?),
        [.., "prod", "cProd"] => {
            set_item(item_atual, |i| {
                i.codigo_fornecedor = Some(texto.to_string());
            });
        }
        [.., "prod", "xProd"] => set_item(item_atual, |i| i.descricao = Some(texto.to_string())),
        [.., "prod", "NCM"] => set_item(item_atual, |i| i.ncm = Some(texto.to_string())),
        [.., "prod", "qCom"] => {
            let q = Quantidade::de_str(texto)
                .map_err(|_| ErroFiscal::XmlInvalido(format!("qCom inválido: {texto}")))?;
            set_item(item_atual, |i| i.quantidade = Some(q));
        }
        [.., "prod", "vUnCom"] => {
            let p = Preco::de_str(texto)
                .map_err(|_| ErroFiscal::XmlInvalido(format!("vUnCom inválido: {texto}")))?;
            set_item(item_atual, |i| i.valor_unitario = Some(p));
        }
        [.., "prod", "vProd"] => {
            let v = dinheiro_de(texto, "vProd")?;
            set_item(item_atual, |i| i.valor_total = Some(v));
        }
        _ => {}
    }
    Ok(())
}

fn set_item(item_atual: &mut Option<ItemParcial>, aplicar: impl FnOnce(&mut ItemParcial)) {
    if let Some(item) = item_atual {
        aplicar(item);
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    const XML_EXEMPLO: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<nfeProc xmlns="http://www.portalfiscal.inf.br/nfe">
  <NFe>
    <infNFe Id="NFe35240912345678000181550010000012341123456789" versao="4.00">
      <ide>
        <nNF>1234</nNF>
        <serie>1</serie>
        <dhEmi>2024-09-12T10:00:00-03:00</dhEmi>
      </ide>
      <emit>
        <CNPJ>11222333000181</CNPJ>
        <xNome>Distribuidora Alfa LTDA</xNome>
      </emit>
      <det nItem="1">
        <prod>
          <cProd>REF-8821</cProd>
          <xProd>REFRIG COLA PET 2L</xProd>
          <NCM>22021000</NCM>
          <qCom>120.0000</qCom>
          <vUnCom>4.320000</vUnCom>
          <vProd>518.40</vProd>
        </prod>
      </det>
      <det nItem="2">
        <prod>
          <cProd>REF-8822</cProd>
          <xProd>REFRIG GUARANA PET 2L</xProd>
          <NCM>22021000</NCM>
          <qCom>60.0000</qCom>
          <vUnCom>4.320000</vUnCom>
          <vProd>259.20</vProd>
        </prod>
      </det>
      <total>
        <ICMSTot>
          <vProd>777.60</vProd>
          <vFrete>180.00</vFrete>
          <vSeg>0.00</vSeg>
          <vOutro>0.00</vOutro>
          <vNF>957.60</vNF>
        </ICMSTot>
      </total>
    </infNFe>
  </NFe>
</nfeProc>"#;

    #[test]
    fn interpreta_os_campos_essenciais() {
        let nota = interpretar(XML_EXEMPLO).unwrap();
        assert_eq!(nota.numero, "1234");
        assert_eq!(nota.serie, "1");
        assert_eq!(nota.fornecedor_nome, "Distribuidora Alfa LTDA");
        assert_eq!(nota.itens.len(), 2);
        assert_eq!(nota.itens[0].codigo_fornecedor, "REF-8821");
        assert_eq!(nota.itens[0].valor_total, Dinheiro::centavos(51_840));
        assert_eq!(nota.valor_frete, Dinheiro::reais(180));
        assert_eq!(
            nota.valor_total,
            Dinheiro::reais(957) + Dinheiro::centavos(60)
        );
    }

    #[test]
    fn xml_sem_campo_obrigatorio_e_recusado() {
        let erro = interpretar("<nfeProc><NFe><infNFe Id=\"NFe12345678901234567890123456789012345678901234\"></infNFe></NFe></nfeProc>")
            .unwrap_err();
        assert!(matches!(erro, ErroFiscal::XmlInvalido(_)));
    }
}
