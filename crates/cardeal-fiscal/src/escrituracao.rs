//! A leitura de uma NF-e/NFC-e **para escriturar** (a EFD ICMS/IPI): o documento inteiro como
//! o XML o descreve — os dois participantes com endereço e IE, os totais e, item a item, o CFOP,
//! a unidade e os tributos (ICMS, ST, IPI, PIS, COFINS). Serve tanto à nota que a empresa emitiu
//! (o `nfeProc` devolvido pela SEFAZ) quanto à do fornecedor (o XML que veio com a compra).
//!
//! Diferente de [`crate::nfe::interpretar`] (o que a entrada de mercadoria precisa), aqui nada é
//! obrigatório além da chave, do número e da data: o que faltar sai zerado ou vazio, e quem
//! escritura decide.

use std::collections::HashMap;

use cardeal_kernel::{Data, Dinheiro, Percentual, Quantidade};
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use serde::{Deserialize, Serialize};

use crate::erros::ErroFiscal;
use crate::nfe::FinalidadeNfe;

/// Um participante (emitente ou destinatário), como está no XML.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParticipanteXml {
    /// CNPJ ou CPF, só dígitos (vazio para o estrangeiro).
    pub documento: String,
    /// O nome (razão social).
    pub nome: String,
    /// A inscrição estadual, quando há.
    pub inscricao_estadual: Option<String>,
    /// Logradouro.
    pub logradouro: String,
    /// Número.
    pub numero: String,
    /// Complemento.
    pub complemento: String,
    /// Bairro.
    pub bairro: String,
    /// Código IBGE do município (7 dígitos).
    pub codigo_municipio: String,
    /// UF.
    pub uf: String,
}

impl ParticipanteXml {
    /// Se o documento é um CNPJ (14 dígitos).
    #[must_use]
    pub fn e_cnpj(&self) -> bool {
        self.documento.len() == 14
    }
}

/// Os totais da nota (`total/ICMSTot`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TotaisXml {
    /// `vProd`.
    pub produtos: Dinheiro,
    /// `vDesc`.
    pub desconto: Dinheiro,
    /// `vFrete`.
    pub frete: Dinheiro,
    /// `vSeg`.
    pub seguro: Dinheiro,
    /// `vOutro`.
    pub outras: Dinheiro,
    /// `vBC`.
    pub base_icms: Dinheiro,
    /// `vICMS`.
    pub icms: Dinheiro,
    /// `vBCST`.
    pub base_st: Dinheiro,
    /// `vST` + `vFCPST`.
    pub st: Dinheiro,
    /// `vIPI` + `vIPIDevol`.
    pub ipi: Dinheiro,
    /// `vPIS`.
    pub pis: Dinheiro,
    /// `vCOFINS`.
    pub cofins: Dinheiro,
    /// `vNF`.
    pub total: Dinheiro,
}

/// Um item (`det`) com os tributos.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemXml {
    /// `nItem`.
    pub numero: u32,
    /// `cProd` (o código de quem emitiu).
    pub codigo: String,
    /// `xProd`.
    pub descricao: String,
    /// `NCM`.
    pub ncm: String,
    /// `cEAN`, sem o `SEM GTIN`.
    pub gtin: Option<String>,
    /// `CEST`.
    pub cest: Option<String>,
    /// `CFOP` (o de quem emitiu).
    pub cfop: String,
    /// `uCom`.
    pub unidade: String,
    /// `qCom`.
    pub quantidade: Quantidade,
    /// `vProd`.
    pub valor: Dinheiro,
    /// `vDesc`.
    pub desconto: Dinheiro,
    /// `vFrete` + `vSeg` + `vOutro` do item.
    pub despesas: Dinheiro,
    /// `orig`.
    pub origem: u8,
    /// O CST do ICMS (regime normal).
    pub cst_icms: Option<String>,
    /// O CSOSN (emitente do Simples).
    pub csosn: Option<String>,
    /// `vBC` do ICMS.
    pub base_icms: Dinheiro,
    /// `pICMS`.
    pub aliquota_icms: Option<Percentual>,
    /// `vICMS`.
    pub icms: Dinheiro,
    /// `vBCST`.
    pub base_st: Dinheiro,
    /// `pICMSST`.
    pub aliquota_st: Option<Percentual>,
    /// `vICMSST` + `vFCPST`.
    pub st: Dinheiro,
    /// `pCredSN`: o crédito que o emitente do Simples permite (CSOSN 101/201/900).
    pub aliquota_credito_sn: Option<Percentual>,
    /// `vCredICMSSN`.
    pub credito_sn: Dinheiro,
    /// O CST do IPI.
    pub cst_ipi: Option<String>,
    /// `cEnq`.
    pub enquadramento_ipi: Option<String>,
    /// `vBC` do IPI.
    pub base_ipi: Dinheiro,
    /// `pIPI`.
    pub aliquota_ipi: Option<Percentual>,
    /// `vIPI`.
    pub ipi: Dinheiro,
    /// O CST do PIS.
    pub cst_pis: Option<String>,
    /// `vBC` do PIS.
    pub base_pis: Dinheiro,
    /// `pPIS`.
    pub aliquota_pis: Option<Percentual>,
    /// `vPIS`.
    pub pis: Dinheiro,
    /// O CST da COFINS.
    pub cst_cofins: Option<String>,
    /// `vBC` da COFINS.
    pub base_cofins: Dinheiro,
    /// `pCOFINS`.
    pub aliquota_cofins: Option<Percentual>,
    /// `vCOFINS`.
    pub cofins: Dinheiro,
}

impl ItemXml {
    /// O valor da operação do item: produto − desconto + despesas + ST + IPI (o que o C190 da
    /// EFD soma).
    #[must_use]
    pub fn valor_da_operacao(&self) -> Dinheiro {
        self.valor - self.desconto + self.despesas + self.st + self.ipi
    }
}

/// A nota inteira, para escriturar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentoXml {
    /// A chave (44 dígitos).
    pub chave: String,
    /// `55` ou `65`.
    pub modelo: String,
    /// A série.
    pub serie: String,
    /// O número.
    pub numero: String,
    /// `dhEmi`.
    pub emissao: Data,
    /// `dhSaiEnt`, quando há.
    pub saida_ou_entrada: Option<Data>,
    /// `tpNF = 1`.
    pub saida: bool,
    /// `finNFe`.
    pub finalidade: FinalidadeNfe,
    /// O emitente.
    pub emitente: ParticipanteXml,
    /// O destinatário, quando há.
    pub destinatario: Option<ParticipanteXml>,
    /// Pagamento a prazo: a nota tem duplicatas (`cobr/dup`) ou `indPag = 1`.
    pub a_prazo: bool,
    /// `transp/modFrete` (9 = sem frete).
    pub modalidade_frete: String,
    /// Os totais.
    pub totais: TotaisXml,
    /// Os itens, na ordem do XML.
    pub itens: Vec<ItemXml>,
}

/// Um número decimal do XML (sempre com ponto) em unidades de `casas` decimais. Casas a mais
/// são cortadas (o XML não as traz além do leiaute).
fn decimal(texto: &str, casas: u32) -> Option<i64> {
    let t = texto.trim();
    let (negativo, t) = t.strip_prefix('-').map_or((false, t), |r| (true, r));
    let (inteiro, fracao) = t.split_once('.').unwrap_or((t, ""));
    if inteiro.is_empty() && fracao.is_empty() {
        return None;
    }
    let inteiro: i64 = if inteiro.is_empty() {
        0
    } else {
        inteiro.parse().ok()?
    };
    let mut f: String = fracao.chars().take(casas as usize).collect();
    if !f.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    while f.len() < casas as usize {
        f.push('0');
    }
    let fracao: i64 = if f.is_empty() { 0 } else { f.parse().ok()? };
    let v = inteiro
        .checked_mul(10_i64.pow(casas))?
        .checked_add(fracao)?;
    Some(if negativo { -v } else { v })
}

fn data(texto: &str) -> Option<Data> {
    let ano = texto.get(0..4)?.parse().ok()?;
    let mes = texto.get(5..7)?.parse().ok()?;
    let dia = texto.get(8..10)?.parse().ok()?;
    Data::de_ymd(ano, mes, dia).ok()
}

/// Os campos lidos de um nível (a nota ou um item): caminho → texto (a primeira ocorrência).
struct Campos(HashMap<String, String>);

impl Campos {
    fn texto(&self, caminho: &str) -> Option<&str> {
        self.0
            .get(caminho)
            .map(String::as_str)
            .filter(|s| !s.is_empty())
    }

    fn opcao(&self, caminho: &str) -> Option<String> {
        self.texto(caminho).map(str::to_owned)
    }

    fn string(&self, caminho: &str) -> String {
        self.texto(caminho).unwrap_or_default().to_owned()
    }

    fn dinheiro(&self, caminho: &str) -> Dinheiro {
        self.texto(caminho)
            .and_then(|t| decimal(t, 2))
            .map_or(Dinheiro::ZERO, Dinheiro::centavos)
    }

    fn percentual(&self, caminho: &str) -> Option<Percentual> {
        self.texto(caminho)
            .and_then(|t| decimal(t, 6))
            .map(Percentual::unidades)
    }

    fn participante(&self, raiz: &str, endereco: &str) -> ParticipanteXml {
        let e = |c: &str| self.string(&format!("{raiz}/{endereco}/{c}"));
        ParticipanteXml {
            documento: self
                .texto(&format!("{raiz}/CNPJ"))
                .or_else(|| self.texto(&format!("{raiz}/CPF")))
                .unwrap_or_default()
                .to_owned(),
            nome: self.string(&format!("{raiz}/xNome")),
            inscricao_estadual: self.opcao(&format!("{raiz}/IE")),
            logradouro: e("xLgr"),
            numero: e("nro"),
            complemento: e("xCpl"),
            bairro: e("xBairro"),
            codigo_municipio: e("cMun"),
            uf: e("UF"),
        }
    }

    fn item(&self, numero: u32) -> ItemXml {
        let icms = |c: &str| format!("imposto/ICMS/*/{c}");
        let ipi = |c: &str| format!("imposto/IPI/*/{c}");
        let pis = |c: &str| format!("imposto/PIS/*/{c}");
        let cofins = |c: &str| format!("imposto/COFINS/*/{c}");
        ItemXml {
            numero,
            codigo: self.string("prod/cProd"),
            descricao: self.string("prod/xProd"),
            ncm: self.string("prod/NCM"),
            gtin: self
                .opcao("prod/cEAN")
                .filter(|g| !g.eq_ignore_ascii_case("SEM GTIN")),
            cest: self.opcao("prod/CEST"),
            cfop: self.string("prod/CFOP"),
            unidade: self.string("prod/uCom"),
            quantidade: self
                .texto("prod/qCom")
                .and_then(|t| decimal(t, 4))
                .map_or(Quantidade::ZERO, Quantidade::interna),
            valor: self.dinheiro("prod/vProd"),
            desconto: self.dinheiro("prod/vDesc"),
            despesas: self.dinheiro("prod/vFrete")
                + self.dinheiro("prod/vSeg")
                + self.dinheiro("prod/vOutro"),
            origem: self
                .texto(&icms("orig"))
                .and_then(|o| o.parse().ok())
                .unwrap_or(0),
            cst_icms: self.opcao(&icms("CST")),
            csosn: self.opcao(&icms("CSOSN")),
            base_icms: self.dinheiro(&icms("vBC")),
            aliquota_icms: self.percentual(&icms("pICMS")),
            icms: self.dinheiro(&icms("vICMS")),
            base_st: self.dinheiro(&icms("vBCST")),
            aliquota_st: self.percentual(&icms("pICMSST")),
            st: self.dinheiro(&icms("vICMSST")) + self.dinheiro(&icms("vFCPST")),
            aliquota_credito_sn: self.percentual(&icms("pCredSN")),
            credito_sn: self.dinheiro(&icms("vCredICMSSN")),
            cst_ipi: self.opcao(&ipi("CST")),
            enquadramento_ipi: self.opcao("imposto/IPI/cEnq"),
            base_ipi: self.dinheiro(&ipi("vBC")),
            aliquota_ipi: self.percentual(&ipi("pIPI")),
            ipi: self.dinheiro(&ipi("vIPI")) + self.dinheiro("impostoDevol/IPI/vIPIDevol"),
            cst_pis: self.opcao(&pis("CST")),
            base_pis: self.dinheiro(&pis("vBC")),
            aliquota_pis: self.percentual(&pis("pPIS")),
            pis: self.dinheiro(&pis("vPIS")),
            cst_cofins: self.opcao(&cofins("CST")),
            base_cofins: self.dinheiro(&cofins("vBC")),
            aliquota_cofins: self.percentual(&cofins("pCOFINS")),
            cofins: self.dinheiro(&cofins("vCOFINS")),
        }
    }
}

/// O caminho de um texto: relativo ao `det` (dentro de um item) ou ao `infNFe`; o grupo de cada
/// tributo (`ICMS00`, `ICMSSN102`, `IPITrib`, `PISAliq`…) vira `*`.
fn caminho(pilha: &[String], dentro_do_item: bool) -> String {
    let raiz = if dentro_do_item { "det" } else { "infNFe" };
    let inicio = pilha.iter().rposition(|p| p == raiz).map_or(0, |i| i + 1);
    let partes = &pilha[inicio..];
    let mut v: Vec<&str> = Vec::with_capacity(partes.len());
    for (i, p) in partes.iter().enumerate() {
        let grupo = i >= 1
            && i + 1 < partes.len()
            && matches!(partes[i - 1].as_str(), "ICMS" | "IPI" | "PIS" | "COFINS")
            && p.starts_with(partes[i - 1].as_str());
        v.push(if grupo { "*" } else { p.as_str() });
    }
    v.join("/")
}

/// O que a varredura do XML recolhe: a chave, os campos da nota, os itens e se há duplicatas.
struct Varredura {
    chave: Option<String>,
    cabecalho: Campos,
    itens: Vec<ItemXml>,
    com_duplicatas: bool,
}

/// Percorre o XML guardando cada texto pelo caminho (o do item, dentro de um `det`).
fn varrer(xml: &str) -> Result<Varredura, ErroFiscal> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut pilha: Vec<String> = Vec::new();
    let mut cabecalho = HashMap::new();
    let mut itens: Vec<ItemXml> = Vec::new();
    let mut item: Option<(u32, HashMap<String, String>)> = None;
    let mut chave = None;
    let mut com_duplicatas = false;
    loop {
        match reader
            .read_event()
            .map_err(|e| ErroFiscal::XmlInvalido(e.to_string()))?
        {
            Event::Eof => break,
            Event::Start(e) => {
                let nome = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                let atributo = |a: &[u8]| {
                    e.attributes()
                        .flatten()
                        .find(|x| x.key.local_name().as_ref() == a)
                        .and_then(|x| x.decode_and_unescape_value(reader.decoder()).ok())
                        .map(std::borrow::Cow::into_owned)
                };
                match nome.as_str() {
                    "infNFe" => {
                        chave = atributo(b"Id").map(|id| id.trim_start_matches("NFe").to_owned());
                    }
                    "det" => {
                        let n = atributo(b"nItem")
                            .and_then(|n| n.parse().ok())
                            .unwrap_or(u32::try_from(itens.len() + 1).unwrap_or(0));
                        item = Some((n, HashMap::new()));
                    }
                    "dup" => com_duplicatas = true,
                    _ => {}
                }
                pilha.push(nome);
            }
            Event::End(e) => {
                if e.local_name().as_ref() == b"det" {
                    if let Some((n, campos)) = item.take() {
                        itens.push(Campos(campos).item(n));
                    }
                }
                pilha.pop();
            }
            Event::Text(t) => {
                let texto = t.unescape().unwrap_or_default().trim().to_owned();
                if texto.is_empty() || !pilha.iter().any(|p| p == "infNFe") {
                    continue;
                }
                match &mut item {
                    Some((_, campos)) => {
                        campos.entry(caminho(&pilha, true)).or_insert(texto);
                    }
                    None => {
                        cabecalho.entry(caminho(&pilha, false)).or_insert(texto);
                    }
                }
            }
            _ => {}
        }
    }
    Ok(Varredura {
        chave,
        cabecalho: Campos(cabecalho),
        itens,
        com_duplicatas,
    })
}

/// Lê a nota para escriturar.
///
/// # Errors
/// [`ErroFiscal::XmlInvalido`] se o XML não puder ser lido ou não tiver a chave, o número ou a
/// data de emissão.
pub fn ler_para_escriturar(xml: &str) -> Result<DocumentoXml, ErroFiscal> {
    let Varredura {
        chave,
        cabecalho: c,
        itens,
        com_duplicatas,
    } = varrer(xml)?;
    let ausente =
        |campo: &str| ErroFiscal::XmlInvalido(format!("campo obrigatório ausente: {campo}"));
    let chave = chave
        .filter(|k| k.len() == 44)
        .ok_or_else(|| ausente("infNFe/Id"))?;
    let emissao = c
        .texto("ide/dhEmi")
        .or_else(|| c.texto("ide/dEmi"))
        .and_then(data)
        .ok_or_else(|| ausente("ide/dhEmi"))?;
    let numero = c.opcao("ide/nNF").ok_or_else(|| ausente("ide/nNF"))?;
    let destinatario = c
        .texto("dest/xNome")
        .or_else(|| c.texto("dest/CNPJ"))
        .or_else(|| c.texto("dest/CPF"))
        .map(|_| c.participante("dest", "enderDest"));
    let soma = |a: &str, b: &str| c.dinheiro(a) + c.dinheiro(b);
    Ok(DocumentoXml {
        modelo: c
            .opcao("ide/mod")
            .unwrap_or_else(|| chave.get(20..22).unwrap_or("55").to_owned()),
        serie: c.string("ide/serie"),
        numero,
        emissao,
        saida_ou_entrada: c.texto("ide/dhSaiEnt").and_then(data),
        saida: c.texto("ide/tpNF") != Some("0"),
        finalidade: match c.texto("ide/finNFe") {
            Some("2") => FinalidadeNfe::Complementar,
            Some("3") => FinalidadeNfe::Ajuste,
            Some("4") => FinalidadeNfe::Devolucao,
            _ => FinalidadeNfe::Normal,
        },
        emitente: c.participante("emit", "enderEmit"),
        destinatario,
        a_prazo: com_duplicatas || c.texto("pag/detPag/indPag") == Some("1"),
        modalidade_frete: c.opcao("transp/modFrete").unwrap_or_else(|| "9".into()),
        totais: TotaisXml {
            produtos: c.dinheiro("total/ICMSTot/vProd"),
            desconto: c.dinheiro("total/ICMSTot/vDesc"),
            frete: c.dinheiro("total/ICMSTot/vFrete"),
            seguro: c.dinheiro("total/ICMSTot/vSeg"),
            outras: c.dinheiro("total/ICMSTot/vOutro"),
            base_icms: c.dinheiro("total/ICMSTot/vBC"),
            icms: c.dinheiro("total/ICMSTot/vICMS"),
            base_st: c.dinheiro("total/ICMSTot/vBCST"),
            st: soma("total/ICMSTot/vST", "total/ICMSTot/vFCPST"),
            ipi: soma("total/ICMSTot/vIPI", "total/ICMSTot/vIPIDevol"),
            pis: c.dinheiro("total/ICMSTot/vPIS"),
            cofins: c.dinheiro("total/ICMSTot/vCOFINS"),
            total: c.dinheiro("total/ICMSTot/vNF"),
        },
        chave,
        itens,
    })
}

#[cfg(test)]
mod testes {
    use super::*;

    const COMPRA: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<nfeProc xmlns="http://www.portalfiscal.inf.br/nfe" versao="4.00"><NFe><infNFe Id="NFe43261011222333000181550010000004561000004567" versao="4.00">
<ide><cUF>43</cUF><mod>55</mod><serie>1</serie><nNF>456</nNF><dhEmi>2026-10-05T09:30:00-03:00</dhEmi><dhSaiEnt>2026-10-06T08:00:00-03:00</dhSaiEnt><tpNF>1</tpNF><finNFe>1</finNFe></ide>
<emit><CNPJ>11222333000181</CNPJ><xNome>Distribuidora Alfa LTDA</xNome><enderEmit><xLgr>Rua A</xLgr><nro>10</nro><xBairro>Centro</xBairro><cMun>4314902</cMun><xMun>Porto Alegre</xMun><UF>RS</UF></enderEmit><IE>0960000000</IE><CRT>3</CRT></emit>
<dest><CNPJ>99888777000166</CNPJ><xNome>Loja Beta</xNome><enderDest><xLgr>Av B</xLgr><nro>5</nro><xBairro>Sul</xBairro><cMun>4304606</cMun><UF>RS</UF></enderDest><indIEDest>1</indIEDest><IE>1234567890</IE></dest>
<det nItem="1"><prod><cProd>A1</cProd><cEAN>7894900011517</cEAN><xProd>REFRI 2L</xProd><NCM>22021000</NCM><CEST>0300700</CEST><CFOP>5405</CFOP><uCom>CX</uCom><qCom>10.0000</qCom><vUnCom>50.0000000000</vUnCom><vProd>500.00</vProd><vDesc>20.00</vDesc><vFrete>10.00</vFrete></prod>
<imposto><ICMS><ICMS10><orig>0</orig><CST>10</CST><modBC>3</modBC><vBC>490.00</vBC><pICMS>17.0000</pICMS><vICMS>83.30</vICMS><modBCST>4</modBCST><pMVAST>40.0000</pMVAST><vBCST>686.00</vBCST><pICMSST>17.0000</pICMSST><vICMSST>33.32</vICMSST></ICMS10></ICMS>
<IPI><cEnq>999</cEnq><IPITrib><CST>50</CST><vBC>490.00</vBC><pIPI>5.0000</pIPI><vIPI>24.50</vIPI></IPITrib></IPI>
<PIS><PISAliq><CST>01</CST><vBC>490.00</vBC><pPIS>1.6500</pPIS><vPIS>8.09</vPIS></PISAliq></PIS><COFINS><COFINSAliq><CST>01</CST><vBC>490.00</vBC><pCOFINS>7.6000</pCOFINS><vCOFINS>37.24</vCOFINS></COFINSAliq></COFINS></imposto></det>
<det nItem="2"><prod><cProd>B2</cProd><cEAN>SEM GTIN</cEAN><xProd>COPO</xProd><NCM>39241000</NCM><CFOP>5102</CFOP><uCom>UN</uCom><qCom>3</qCom><vProd>30.00</vProd></prod>
<imposto><ICMS><ICMSSN101><orig>0</orig><CSOSN>101</CSOSN><pCredSN>2.5600</pCredSN><vCredICMSSN>0.77</vCredICMSSN></ICMSSN101></ICMS></imposto></det>
<total><ICMSTot><vBC>490.00</vBC><vICMS>83.30</vICMS><vBCST>686.00</vBCST><vST>33.32</vST><vFCPST>0.00</vFCPST><vProd>530.00</vProd><vFrete>10.00</vFrete><vSeg>0.00</vSeg><vDesc>20.00</vDesc><vIPI>24.50</vIPI><vIPIDevol>0.00</vIPIDevol><vPIS>8.09</vPIS><vCOFINS>37.24</vCOFINS><vOutro>0.00</vOutro><vNF>577.82</vNF></ICMSTot></total>
<transp><modFrete>0</modFrete></transp><cobr><dup><nDup>001</nDup><dVenc>2026-11-05</dVenc><vDup>577.82</vDup></dup></cobr>
</infNFe></NFe><protNFe versao="4.00"><infProt><chNFe>43261011222333000181550010000004561000004567</chNFe><cStat>100</cStat></infProt></protNFe></nfeProc>"#;

    #[test]
    fn le_participantes_totais_e_os_tributos_de_cada_item() {
        let d = ler_para_escriturar(COMPRA).unwrap();
        assert_eq!(d.chave.len(), 44);
        assert_eq!((d.modelo.as_str(), d.numero.as_str()), ("55", "456"));
        assert_eq!(d.saida_ou_entrada, Some(Data::de_ymd(2026, 10, 6).unwrap()));
        assert!(d.saida && d.a_prazo);
        assert_eq!(d.emitente.documento, "11222333000181");
        assert_eq!(d.emitente.codigo_municipio, "4314902");
        let dest = d.destinatario.as_ref().unwrap();
        assert_eq!(dest.inscricao_estadual.as_deref(), Some("1234567890"));
        assert_eq!(d.totais.total, Dinheiro::centavos(57_782));
        assert_eq!(d.totais.st, Dinheiro::centavos(3_332));
        assert_eq!(d.modalidade_frete, "0");

        let a = &d.itens[0];
        assert_eq!((a.cfop.as_str(), a.unidade.as_str()), ("5405", "CX"));
        assert_eq!(a.cst_icms.as_deref(), Some("10"));
        assert_eq!(a.aliquota_icms, Some(Percentual::pontos(17)));
        assert_eq!(a.icms, Dinheiro::centavos(8_330));
        assert_eq!(a.st, Dinheiro::centavos(3_332));
        assert_eq!(a.ipi, Dinheiro::centavos(2_450));
        assert_eq!(a.enquadramento_ipi.as_deref(), Some("999"));
        assert_eq!(a.cst_pis.as_deref(), Some("01"));
        assert_eq!(a.quantidade, Quantidade::unidades(10));
        assert_eq!(a.despesas, Dinheiro::reais(10));
        // 500 − 20 + 10 + 33,32 + 24,50
        assert_eq!(a.valor_da_operacao(), Dinheiro::centavos(54_782));

        let b = &d.itens[1];
        assert_eq!(b.gtin, None);
        assert_eq!(b.csosn.as_deref(), Some("101"));
        assert_eq!(b.credito_sn, Dinheiro::centavos(77));
        assert_eq!(
            b.aliquota_credito_sn.map(Percentual::unidades_internas),
            Some(2_560_000)
        );
        // As duas operações somam o total da nota.
        let soma: Dinheiro = d.itens.iter().map(ItemXml::valor_da_operacao).sum();
        assert_eq!(soma, d.totais.total);
    }

    #[test]
    fn sem_chave_ou_sem_data_nao_e_documento() {
        assert!(
            ler_para_escriturar("<NFe><infNFe><ide><nNF>1</nNF></ide></infNFe></NFe>").is_err()
        );
        let sem_data = COMPRA.replace("<dhEmi>2026-10-05T09:30:00-03:00</dhEmi>", "");
        assert!(ler_para_escriturar(&sem_data).is_err());
    }

    #[test]
    fn decimais_do_xml_sempre_com_ponto() {
        assert_eq!(decimal("1.234", 4), Some(12_340));
        assert_eq!(decimal("3", 2), Some(300));
        assert_eq!(decimal("0.5", 2), Some(50));
        assert_eq!(decimal("17.0000", 6), Some(17_000_000));
        assert_eq!(decimal("abc", 2), None);
    }
}
