//! A **NFC-e em contingência off-line montada e assinada pelo próprio Cardeal** (ADR-0021):
//! quando nem o `sefaz-api` é alcançado (o caixa sem internet), a venda não fica sem cupom
//! fiscal. A nota sai com `tpEmis = 9`, `dhCont`/`xJust`, o QR Code v3 off-line (os parâmetros
//! assinados com a chave do A1) e a assinatura `XMLDSig` — o mesmo XML que o
//! `POST /nfe/nfce/contingencia/prepare` do `sefaz-api` monta para o mesmo pedido (conferido em
//! teste contra o montador dele e contra o XSD oficial 4.00). A transmissão, quando a rede
//! volta, é a de sempre: o `sefaz-api` envia **o mesmo XML** (`/contingencia/transmit`).
//!
//! Só o que o Cardeal emite cabe aqui: Simples (CSOSN 102, 103, 300, 400, 500) e regime normal
//! (CST 00, 20, 40, 41, 50, 60); outro código é recusado — e a nota espera a rede, como antes.

use base64::Engine as _;

use cardeal_kernel::{Instante, Uf};

use crate::certificado::{AlgoritmoAssinatura, ChaveDeAssinatura};
use crate::chave::ChaveAcesso;
use crate::emissao::{
    ContribuicaoItem, DocumentoConsumidor, EntradaEmContingencia, IcmsItem, ItemDocumento,
    MeioPagamento, Nfce, NotaEmContingencia,
};
use crate::erros::ErroFiscal;
use crate::nfse_nacional::assinatura::assinar_enveloped_com;
use crate::nfse_nacional::xml::{digitos, texto_xsd, Xml};
use crate::porta::AmbienteFiscal;

/// O namespace da NF-e.
const NS_NFE: &str = "http://www.portalfiscal.inf.br/nfe";

/// O nome que a SEFAZ exige no destinatário em homologação.
const NOME_EM_HOMOLOGACAO: &str = "NF-E EMITIDA EM AMBIENTE DE HOMOLOGACAO - SEM VALOR FISCAL";

/// As variáveis do responsável técnico, quando ainda não há nota autorizada de onde tirá-lo.
pub const VARS_RESPONSAVEL_TECNICO: [&str; 4] = [
    "CARDEAL_RESP_TEC_CNPJ",
    "CARDEAL_RESP_TEC_CONTATO",
    "CARDEAL_RESP_TEC_EMAIL",
    "CARDEAL_RESP_TEC_FONE",
];

/// O responsável técnico pelo sistema emissor (`infRespTec`, NT 2018.005) — obrigatório na
/// NFC-e. É o do `sefaz-api` (a mesma operação): o Cardeal o tira da última nota autorizada
/// da empresa, que o `sefaz-api` montou, ou das variáveis [`VARS_RESPONSAVEL_TECNICO`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ResponsavelTecnico {
    /// O CNPJ (14 dígitos).
    pub cnpj: String,
    /// O nome da pessoa de contato.
    pub contato: String,
    /// O e-mail.
    pub email: String,
    /// O telefone (só dígitos).
    pub fone: String,
}

impl ResponsavelTecnico {
    /// O responsável técnico de um XML de NF-e/NFC-e (o `infRespTec`), se houver e for completo.
    #[must_use]
    pub fn do_xml(xml: &str) -> Option<Self> {
        let i = xml.find("<infRespTec>")?;
        let f = i + xml[i..].find("</infRespTec>")?;
        let bloco = &xml[i..f];
        let campo = |nome: &str| {
            let abre = format!("<{nome}>");
            let a = bloco.find(&abre)? + abre.len();
            let b = a + bloco[a..].find('<')?;
            Some(desescapar(&bloco[a..b]))
        };
        Self::novo(
            &campo("CNPJ")?,
            &campo("xContato")?,
            &campo("email")?,
            &campo("fone")?,
        )
    }

    /// O das variáveis [`VARS_RESPONSAVEL_TECNICO`], se todas estiverem definidas.
    #[must_use]
    pub fn do_ambiente() -> Option<Self> {
        let v = |nome: &str| std::env::var(nome).ok();
        let [cnpj, contato, email, fone] = VARS_RESPONSAVEL_TECNICO;
        Self::novo(&v(cnpj)?, &v(contato)?, &v(email)?, &v(fone)?)
    }

    /// Conferido: CNPJ de 14 dígitos, telefone de 6 a 14, contato e e-mail preenchidos.
    #[must_use]
    pub fn novo(cnpj: &str, contato: &str, email: &str, fone: &str) -> Option<Self> {
        let cnpj = digitos(cnpj);
        let fone = digitos(fone);
        let contato = texto_xsd(contato, 60, false)?;
        let email = texto_xsd(email, 60, false)?;
        (cnpj.len() == 14 && (6..=14).contains(&fone.len()) && email.contains('@')).then_some(
            Self {
                cnpj,
                contato,
                email,
                fone,
            },
        )
    }
}

fn desescapar(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Monta e assina a NFC-e em contingência off-line: a chave com `tpEmis = 9`, o `infNFe`, o
/// `infNFeSupl` (QR Code v3 off-line e `urlChave`) e a `Signature`.
///
/// `emitida_em` é o `dhEmi` (e o `AAMM` da chave); `codigo_numerico` é o `cNF` (8 dígitos,
/// aleatório e diferente do número).
///
/// # Errors
/// [`ErroFiscal::ApiFiscalRecusou`] com o motivo se a nota tem o que a contingência local não
/// monta (sem itens, CST/CSOSN fora da lista, justificativa curta); a do certificado se a
/// assinatura falhar.
#[allow(clippy::too_many_lines)] // o infNFe na ordem do XSD, grupo a grupo
pub fn montar_nfce_em_contingencia(
    nota: &Nfce,
    entrada: &EntradaEmContingencia,
    ambiente: AmbienteFiscal,
    responsavel: &ResponsavelTecnico,
    chave_de_assinatura: &ChaveDeAssinatura,
    emitida_em: Instante,
    codigo_numerico: u32,
) -> Result<NotaEmContingencia, ErroFiscal> {
    let recusa = |m: String| ErroFiscal::ApiFiscalRecusou(format!("contingência local: {m}"));
    let e = &nota.emitente;
    let uf = e.endereco.uf;
    let justificativa = texto_xsd(&entrada.justificativa, 256, false)
        .filter(|j| j.chars().count() >= 15)
        .ok_or_else(|| recusa("a justificativa precisa de 15 a 256 caracteres".into()))?;
    if nota.itens.is_empty() {
        return Err(recusa("a NFC-e não tem itens".into()));
    }
    let codigo_numerico = if u64::from(codigo_numerico) == nota.numero % 100_000_000 {
        (codigo_numerico + 1) % 100_000_000
    } else {
        codigo_numerico % 100_000_000
    };
    let data = emitida_em.data(entrada.fuso);
    let aamm = format!("{:02}{:02}", data.ano() % 100, data.mes());
    let chave = ChaveAcesso::montar(
        uf.codigo_ibge(),
        &aamm,
        &e.cnpj.sem_mascara(),
        "65",
        nota.serie,
        nota.numero,
        9,
        codigo_numerico,
    );
    let chave_str = chave.como_str().to_owned();
    let tp_amb = match ambiente {
        AmbienteFiscal::Producao => "1",
        AmbienteFiscal::Homologacao => "2",
    };
    let dh_emi = data_hora(emitida_em, entrada.fuso);
    let dh_cont = data_hora(entrada.inicio, entrada.fuso);

    let mut x = Xml::novo();
    let id = format!("NFe{chave_str}");
    x.abrir("infNFe", &[("Id", &id), ("versao", "4.00")]);

    // ide
    x.abrir("ide", &[])
        .folha("cUF", &format!("{:02}", uf.codigo_ibge()))
        .folha("cNF", &format!("{codigo_numerico:08}"))
        .folha("natOp", "VENDA AO CONSUMIDOR")
        .folha("mod", "65")
        .folha("serie", &nota.serie.to_string())
        .folha("nNF", &nota.numero.to_string())
        .folha("dhEmi", &dh_emi)
        .folha("tpNF", "1")
        .folha("idDest", "1")
        .folha("cMunFG", &digitos(&e.endereco.codigo_municipio))
        .folha("tpImp", "4")
        .folha("tpEmis", "9")
        .folha("cDV", &chave_str[43..])
        .folha("tpAmb", tp_amb)
        .folha("finNFe", "1")
        .folha("indFinal", "1")
        .folha("indPres", "1")
        .folha("procEmi", "0")
        .folha("verProc", &versao_do_emissor())
        .folha("dhCont", &dh_cont)
        .folha("xJust", &justificativa)
        .fechar("ide");

    // emit
    x.abrir("emit", &[]).folha("CNPJ", &e.cnpj.sem_mascara());
    x.folha("xNome", &texto(&e.razao_social, 60, "a razão social")?);
    if let Some(f) = texto_xsd(&e.nome_fantasia, 60, false) {
        x.folha("xFant", &f);
    }
    endereco(&mut x, "enderEmit", &e.endereco)?;
    let ie = if e.inscricao_estadual.trim().eq_ignore_ascii_case("ISENTO") {
        "ISENTO".to_owned()
    } else {
        digitos(&e.inscricao_estadual)
    };
    x.folha("IE", &ie);
    if let Some(im) = e
        .inscricao_municipal
        .as_deref()
        .and_then(|m| texto_xsd(m, 15, false))
    {
        x.folha("IM", &im);
    }
    x.folha("CRT", e.crt.codigo()).fechar("emit");

    // dest (o consumidor identificado)
    let (tp_id_dest, c_dest) = match nota.consumidor.as_ref().map(|c| c.documento) {
        Some(DocumentoConsumidor::Cnpj(c)) => ("1", c.sem_mascara()),
        Some(DocumentoConsumidor::Cpf(c)) => ("2", c.sem_mascara()),
        None => ("", String::new()),
    };
    if let Some(c) = &nota.consumidor {
        x.abrir("dest", &[]);
        x.folha(if tp_id_dest == "1" { "CNPJ" } else { "CPF" }, &c_dest);
        if let Some(nome) = c.nome.as_deref().and_then(|n| texto_xsd(n, 60, false)) {
            let nome = if tp_amb == "2" {
                NOME_EM_HOMOLOGACAO.to_owned()
            } else {
                nome
            };
            x.folha("xNome", &nome);
        }
        x.folha("indIEDest", "9").fechar("dest");
    }

    // det
    let simples = e.crt.e_simples();
    let mut t = Totais::default();
    for (i, item) in nota.itens.iter().enumerate() {
        det(&mut x, i + 1, item, simples, &mut t).map_err(&recusa)?;
    }

    // total
    let v_nf = t.prod - t.desc;
    x.abrir("total", &[]).abrir("ICMSTot", &[]);
    for (nome, v) in [
        ("vBC", t.bc),
        ("vICMS", t.icms),
        ("vICMSDeson", 0),
        ("vFCPUFDest", 0),
        ("vICMSUFDest", 0),
        ("vICMSUFRemet", 0),
        ("vFCP", 0),
        ("vBCST", 0),
        ("vST", 0),
        ("vFCPST", 0),
        ("vFCPSTRet", 0),
        ("vProd", t.prod),
        ("vFrete", 0),
        ("vSeg", 0),
        ("vDesc", t.desc),
        ("vII", 0),
        ("vIPI", 0),
        ("vIPIDevol", 0),
        ("vPIS", t.pis),
        ("vCOFINS", t.cofins),
        ("vOutro", 0),
        ("vNF", v_nf),
    ] {
        x.folha(nome, &centavos(v));
    }
    x.fechar("ICMSTot").fechar("total");

    // transp, pag
    x.abrir("transp", &[])
        .folha("modFrete", "9")
        .fechar("transp");
    x.abrir("pag", &[]);
    for p in &nota.pagamentos {
        x.abrir("detPag", &[]).folha("tPag", p.meio.codigo());
        if p.meio == MeioPagamento::Outros {
            x.folha("xPag", "Outros");
        }
        x.folha("vPag", &centavos(p.valor.em_centavos()));
        if p.meio.exige_grupo_cartao() {
            x.abrir("card", &[]).folha("tpIntegra", "2").fechar("card");
        }
        x.fechar("detPag");
    }
    if !nota.troco.e_zero() && nota.troco.em_centavos() > 0 {
        x.folha("vTroco", &centavos(nota.troco.em_centavos()));
    }
    x.fechar("pag");

    if let Some(info) = nota
        .informacoes
        .as_deref()
        .and_then(|i| texto_xsd(i, 5000, false))
    {
        x.abrir("infAdic", &[])
            .folha("infCpl", &info)
            .fechar("infAdic");
    }
    x.abrir("infRespTec", &[])
        .folha("CNPJ", &responsavel.cnpj)
        .folha("xContato", &responsavel.contato)
        .folha("email", &responsavel.email)
        .folha("fone", &responsavel.fone)
        .fechar("infRespTec");
    x.fechar("infNFe");
    let inf_nfe = x.texto();

    // infNFeSupl: o QR Code v3 off-line — chNFe|3|tpAmb|dia|vNF|tpIdDest|cDest, assinado (RSA-SHA1,
    // base64) com a chave do certificado.
    let (url_qr, url_consulta) = urls(uf, tp_amb == "1");
    let sequencia = format!(
        "{chave_str}|3|{tp_amb}|{:02}|{}|{tp_id_dest}|{c_dest}",
        data.dia(),
        centavos(v_nf)
    );
    let assinatura_qr = base64::engine::general_purpose::STANDARD
        .encode(chave_de_assinatura.assinar(AlgoritmoAssinatura::Sha1, sequencia.as_bytes())?);
    let mut supl = Xml::novo();
    supl.abrir("infNFeSupl", &[])
        .folha("qrCode", &format!("{url_qr}?p={sequencia}|{assinatura_qr}"))
        .folha("urlChave", url_consulta)
        .fechar("infNFeSupl");

    let xml = assinar_enveloped_com(
        "NFe",
        NS_NFE,
        &[],
        &inf_nfe,
        &supl.texto(),
        &id,
        chave_de_assinatura,
    )?;
    Ok(NotaEmContingencia {
        chave: chave_str,
        xml,
    })
}

/// `Cardeal <versão>` (o `verProc`, até 20 caracteres).
fn versao_do_emissor() -> String {
    format!("Cardeal {}", env!("CARGO_PKG_VERSION"))
        .chars()
        .take(20)
        .collect()
}

/// `AAAA-MM-DDThh:mm:ss-03:00`, no fuso da empresa.
fn data_hora(instante: Instante, fuso: cardeal_kernel::Fuso) -> String {
    let minutos = fuso.em_minutos();
    let sinal = if minutos < 0 { '-' } else { '+' };
    let minutos = minutos.unsigned_abs();
    format!(
        "{}T{}{sinal}{:02}:{:02}",
        instante.data(fuso).formatar_iso(),
        instante.hora(fuso).formatar_com_segundos(),
        minutos / 60,
        minutos % 60
    )
}

fn texto(s: &str, maximo: usize, o_que: &str) -> Result<String, ErroFiscal> {
    texto_xsd(s, maximo, false)
        .ok_or_else(|| ErroFiscal::ApiFiscalRecusou(format!("contingência local: falta {o_que}")))
}

fn endereco(
    x: &mut Xml,
    tag: &str,
    en: &crate::emissao::EnderecoEmitente,
) -> Result<(), ErroFiscal> {
    x.abrir(tag, &[])
        .folha(
            "xLgr",
            &texto(&en.logradouro, 60, "o logradouro do emitente")?,
        )
        .folha(
            "nro",
            &texto(&en.numero, 60, "o número do endereço do emitente")?,
        );
    if let Some(c) = texto_xsd(&en.complemento, 60, false) {
        x.folha("xCpl", &c);
    }
    x.folha("xBairro", &texto(&en.bairro, 60, "o bairro do emitente")?)
        .folha("cMun", &digitos(&en.codigo_municipio))
        .folha(
            "xMun",
            &texto(&en.municipio, 60, "o município do emitente")?,
        )
        .folha("UF", en.uf.sigla())
        .folha("CEP", &digitos(&en.cep))
        .folha("cPais", "1058")
        .folha("xPais", "Brasil");
    if let Some(f) = en
        .telefone
        .as_deref()
        .map(digitos)
        .filter(|f| !f.is_empty())
    {
        x.folha("fone", &f);
    }
    x.fechar(tag);
    Ok(())
}

#[derive(Default)]
struct Totais {
    bc: i64,
    icms: i64,
    prod: i64,
    desc: i64,
    pis: i64,
    cofins: i64,
}

fn det(
    x: &mut Xml,
    n: usize,
    item: &ItemDocumento,
    simples: bool,
    t: &mut Totais,
) -> Result<(), String> {
    let ncm = digitos(&item.ncm);
    let cfop = digitos(&item.cfop);
    if ncm.len() != 8 {
        return Err(format!("item {n}: NCM inválido"));
    }
    if cfop.len() != 4 {
        return Err(format!("item {n}: CFOP inválido"));
    }
    let gtin = item
        .gtin
        .as_deref()
        .map(str::trim)
        .filter(|g| !g.is_empty())
        .unwrap_or("SEM GTIN")
        .to_owned();
    let descricao =
        texto_xsd(&item.descricao, 120, false).ok_or_else(|| format!("item {n}: sem descrição"))?;
    let codigo =
        texto_xsd(&item.codigo, 60, false).ok_or_else(|| format!("item {n}: sem código"))?;
    let unidade =
        texto_xsd(&item.unidade, 6, false).ok_or_else(|| format!("item {n}: sem unidade"))?;
    let quantidade = fixo(item.quantidade.unidades_internas(), 4, 4);
    let unitario = fixo(item.valor_unitario.unidades_internas(), 6, 10);
    x.abrir("det", &[("nItem", &n.to_string())])
        .abrir("prod", &[]);
    x.folha("cProd", &codigo)
        .folha("cEAN", &gtin)
        .folha("xProd", &descricao)
        .folha("NCM", &ncm);
    if let Some(cest) = item.cest.as_deref().map(digitos).filter(|c| !c.is_empty()) {
        x.folha("CEST", &cest);
    }
    x.folha("CFOP", &cfop)
        .folha("uCom", &unidade)
        .folha("qCom", &quantidade)
        .folha("vUnCom", &unitario)
        .folha("vProd", &centavos(item.valor_bruto.em_centavos()))
        .folha("cEANTrib", &gtin)
        .folha("uTrib", &unidade)
        .folha("qTrib", &quantidade)
        .folha("vUnTrib", &unitario);
    if !item.desconto.e_zero() {
        x.folha("vDesc", &centavos(item.desconto.em_centavos()));
    }
    x.folha("indTot", "1").fechar("prod");

    x.abrir("imposto", &[]).abrir("ICMS", &[]);
    icms(x, n, &item.impostos.icms, simples)?;
    x.fechar("ICMS");
    contribuicao(x, "PIS", "pPIS", "vPIS", &item.impostos.pis);
    contribuicao(x, "COFINS", "pCOFINS", "vCOFINS", &item.impostos.cofins);
    x.fechar("imposto").fechar("det");

    let i = &item.impostos.icms;
    t.bc += i.base.map_or(0, cardeal_kernel::Dinheiro::em_centavos);
    t.icms += i.valor.map_or(0, cardeal_kernel::Dinheiro::em_centavos);
    t.prod += item.valor_bruto.em_centavos();
    t.desc += item.desconto.em_centavos();
    t.pis += item
        .impostos
        .pis
        .valor
        .map_or(0, cardeal_kernel::Dinheiro::em_centavos);
    t.cofins += item
        .impostos
        .cofins
        .valor
        .map_or(0, cardeal_kernel::Dinheiro::em_centavos);
    Ok(())
}

fn icms(x: &mut Xml, n: usize, i: &IcmsItem, simples: bool) -> Result<(), String> {
    let orig = i.origem.to_string();
    if simples {
        let csosn = i.csosn.as_deref().unwrap_or("102");
        let grupo = match csosn {
            "102" | "103" | "300" | "400" => "ICMSSN102",
            "500" => "ICMSSN500",
            outro => {
                return Err(format!(
                    "item {n}: CSOSN {outro} fora da contingência local"
                ))
            }
        };
        x.abrir(grupo, &[])
            .folha("orig", &orig)
            .folha("CSOSN", csosn)
            .fechar(grupo);
        return Ok(());
    }
    let cst = i.cst.as_deref().unwrap_or("00");
    let base = || {
        i.base
            .ok_or_else(|| format!("item {n}: CST {cst} sem a base do ICMS"))
    };
    let aliquota = || {
        i.aliquota
            .ok_or_else(|| format!("item {n}: CST {cst} sem a alíquota"))
    };
    let valor = || {
        i.valor
            .ok_or_else(|| format!("item {n}: CST {cst} sem o valor do ICMS"))
    };
    match cst {
        "00" => {
            x.abrir("ICMS00", &[])
                .folha("orig", &orig)
                .folha("CST", cst)
                .folha("modBC", "3")
                .folha("vBC", &centavos(base()?.em_centavos()))
                .folha("pICMS", &fixo(aliquota()?.unidades_internas(), 6, 2))
                .folha("vICMS", &centavos(valor()?.em_centavos()))
                .fechar("ICMS00");
        }
        "20" => {
            let reducao = i
                .reducao_base
                .ok_or_else(|| format!("item {n}: CST 20 sem a redução da base"))?;
            x.abrir("ICMS20", &[])
                .folha("orig", &orig)
                .folha("CST", cst)
                .folha("modBC", "3")
                .folha("pRedBC", &fixo(reducao.unidades_internas(), 6, 2))
                .folha("vBC", &centavos(base()?.em_centavos()))
                .folha("pICMS", &fixo(aliquota()?.unidades_internas(), 6, 2))
                .folha("vICMS", &centavos(valor()?.em_centavos()))
                .fechar("ICMS20");
        }
        "40" | "41" | "50" => {
            x.abrir("ICMS40", &[])
                .folha("orig", &orig)
                .folha("CST", cst)
                .fechar("ICMS40");
        }
        "60" => {
            x.abrir("ICMS60", &[])
                .folha("orig", &orig)
                .folha("CST", cst)
                .fechar("ICMS60");
        }
        outro => return Err(format!("item {n}: CST {outro} fora da contingência local")),
    }
    Ok(())
}

fn contribuicao(x: &mut Xml, tributo: &str, aliq: &str, valor: &str, c: &ContribuicaoItem) {
    let cst = c.cst.as_str();
    x.abrir(tributo, &[]);
    let centavos_de = |d: Option<cardeal_kernel::Dinheiro>| {
        centavos(d.map_or(0, cardeal_kernel::Dinheiro::em_centavos))
    };
    let percentual = fixo(
        c.aliquota
            .map_or(0, cardeal_kernel::Percentual::unidades_internas),
        6,
        4,
    );
    match cst {
        "01" | "02" => {
            let g = format!("{tributo}Aliq");
            x.abrir(&g, &[])
                .folha("CST", cst)
                .folha("vBC", &centavos_de(c.base))
                .folha(aliq, &percentual)
                .folha(valor, &centavos_de(c.valor))
                .fechar(&g);
        }
        "04" | "05" | "06" | "07" | "08" | "09" => {
            let g = format!("{tributo}NT");
            x.abrir(&g, &[]).folha("CST", cst).fechar(&g);
        }
        _ => {
            // 49 a 99 ("outras operações"): vBC, alíquota e valor (zerados sem valores).
            let g = format!("{tributo}Outr");
            x.abrir(&g, &[])
                .folha("CST", cst)
                .folha("vBC", &centavos_de(c.base))
                .folha(aliq, &percentual)
                .folha(valor, &centavos_de(c.valor))
                .fechar(&g);
        }
    }
    x.fechar(tributo);
}

/// Centavos como reais com 2 casas (`1234` → `12.34`, `-5` → `-0.05`).
fn centavos(c: i64) -> String {
    fixo(c, 2, 2)
}

/// O valor `n × 10^-escala` com `casas` decimais, arredondado meio para longe do zero.
fn fixo(n: i64, escala: u32, casas: u32) -> String {
    let n = i128::from(n);
    let ajustado = if casas >= escala {
        n * 10_i128.pow(casas - escala)
    } else {
        let d = 10_i128.pow(escala - casas);
        let meio = d / 2;
        if n >= 0 {
            (n + meio) / d
        } else {
            (n - meio) / d
        }
    };
    let sinal = if ajustado < 0 { "-" } else { "" };
    let a = ajustado.unsigned_abs();
    if casas == 0 {
        return format!("{sinal}{a}");
    }
    let p = 10_u128.pow(casas);
    format!("{sinal}{}.{:0w$}", a / p, a % p, w = casas as usize)
}

fn urls(uf: Uf, producao: bool) -> (&'static str, &'static str) {
    let (qp, qh) = url_qr_code(uf);
    let (cp, ch) = url_consulta(uf);
    if producao {
        (qp, cp)
    } else {
        (qh, ch)
    }
}

/// A URL do QR Code da NFC-e por UF (produção, homologação) — a tabela do `sefaz-api`.
#[allow(clippy::too_many_lines)] // uma linha por UF
const fn url_qr_code(uf: Uf) -> (&'static str, &'static str) {
    match uf {
        Uf::Ac => ("http://www.sefaznet.ac.gov.br/nfce/qrcode", "http://hml.sefaznet.ac.gov.br/nfce/qrcode"),
        Uf::Al => ("https://nfce.sefaz.al.gov.br/QRCode/consultarNFCe.jsp", "https://nfce.sefaz.al.gov.br/QRCode/consultarNFCe.jsp"),
        Uf::Am => ("https://sistemas.sefaz.am.gov.br/nfceweb/consultarNFCe.jsp", "https://homnfce.sefaz.am.gov.br/nfceweb/consultarNFCe.jsp"),
        Uf::Ap => ("https://www.sefaz.ap.gov.br/nfce/nfcep.php", "https://www.sefaz.ap.gov.br/nfcehml/nfce.php"),
        Uf::Ba => ("https://nfe.sefaz.ba.gov.br/servicos/nfce/qrcode.aspx", "https://hnfe.sefaz.ba.gov.br/servicos/nfce/qrcode.aspx"),
        Uf::Ce => ("https://nfce.sefaz.ce.gov.br/pages/ShowNFCe.html", "https://nfceh.sefaz.ce.gov.br/pages/ShowNFCe.html"),
        Uf::Df => ("https://dec.fazenda.df.gov.br/ConsultarNFCe.aspx", "https://dec.fazenda.df.gov.br/ConsultarNFCe.aspx"),
        Uf::Es => ("http://app.sefaz.es.gov.br/ConsultaNFCe/qrcode.aspx", "http://homologacao.sefaz.es.gov.br/ConsultaNFCe/qrcode.aspx"),
        Uf::Go => ("https://nfe.sefaz.go.gov.br/nfeweb/sites/nfce/danfeNFCe", "https://homolog.sefaz.go.gov.br/nfeweb/sites/nfce/danfeNFCe"),
        Uf::Ma => ("https://nfce.sefaz.ma.gov.br/portal/consultarNFCe.jsp", "https://homologacao.sefaz.ma.gov.br/portal/consultarNFCe.jsp"),
        Uf::Mg => ("https://nfce.fazenda.mg.gov.br/portalnfce/sistema/qrcode.xhtml", "https://hnfce.fazenda.mg.gov.br/portalnfce/sistema/qrcode.xhtml"),
        Uf::Ms => ("https://www.dfe.ms.gov.br/nfce/qrcode", "https://www.dfe.ms.gov.br/nfce/qrcode"),
        Uf::Mt => ("https://www.sefaz.mt.gov.br/nfce/consultanfce", "https://homologacao.sefaz.mt.gov.br/nfce/consultanfce"),
        Uf::Pa => ("https://appnfc.sefa.pa.gov.br/portal/view/consultas/nfce/consultanfce.seam", "https://appnfc.sefa.pa.gov.br/portal-homologacao/view/consultas/nfce/consultanfce.seam"),
        Uf::Pb => ("https://www.sefaz.pb.gov.br/nfce", "https://www.sefaz.pb.gov.br/nfcehom"),
        Uf::Pe => ("https://nfce.sefaz.pe.gov.br/nfce/consulta", "https://nfcehomolog.sefaz.pe.gov.br/nfce/consulta"),
        Uf::Pi => ("https://www.sefaz.pi.gov.br/nfce/qrcode", "https://www.sefaz.pi.gov.br/nfce/qrcode"),
        Uf::Pr => ("https://www.fazenda.pr.gov.br/nfce/qrcode", "https://www.fazenda.pr.gov.br/nfce/qrcode"),
        Uf::Rj => ("https://www.nfce.fazenda.rj.gov.br/consulta", "https://www.nfce.fazenda.rj.gov.br/consulta"),
        Uf::Rn => ("https://nfce.set.rn.gov.br/consultarNFCe.aspx", "https://hom.nfce.set.rn.gov.br/consultarNFCe.aspx"),
        Uf::Ro => ("https://www.nfce.sefin.ro.gov.br/consultanfce/consulta.jsp", "https://www.nfce.sefin.ro.gov.br/consultanfce/consulta.jsp"),
        Uf::Rr => ("https://www.sefaz.rr.gov.br/nfce/servlet/qrcode", "https://www.sefaz.rr.gov.br/nfce/servlet/qrcode"),
        Uf::Rs => ("https://www.sefaz.rs.gov.br/NFCE/NFCE-COM.aspx", "https://www.sefaz.rs.gov.br/NFCE/NFCE-COM.aspx"),
        Uf::Sc => ("https://sat.sef.sc.gov.br/nfce/consulta", "https://hom.sat.sef.sc.gov.br/nfce/consulta"),
        Uf::Se => ("https://nfce.sefaz.se.gov.br/nfce/consultarNFCe.jsp", "https://nfce.sefaz.se.gov.br/nfce/consultarNFCe.jsp"),
        Uf::Sp => ("https://www.nfce.fazenda.sp.gov.br/NFCeConsultaPublica/Paginas/ConsultaQRCode.aspx", "https://www.homologacao.nfce.fazenda.sp.gov.br/NFCeConsultaPublica/Paginas/ConsultaQRCode.aspx"),
        Uf::To => ("https://www.sefaz.to.gov.br/nfce/qrcode", "https://www.sefaz.to.gov.br/nfce/qrcode"),
    }
}

/// A URL de consulta por chave (urlChave) da NFC-e por UF (produção, homologação) — a tabela do `sefaz-api`.
#[allow(clippy::too_many_lines)] // uma linha por UF
const fn url_consulta(uf: Uf) -> (&'static str, &'static str) {
    match uf {
        Uf::Ac => (
            "www.sefaznet.ac.gov.br/nfce/consulta",
            "www.sefaznet.ac.gov.br/nfce/consulta",
        ),
        Uf::Al => (
            "www.sefaz.al.gov.br/nfce/consulta",
            "www.sefaz.al.gov.br/nfce/consulta",
        ),
        Uf::Am => (
            "www.sefaz.am.gov.br/nfce/consulta",
            "www.sefaz.am.gov.br/nfce/consulta",
        ),
        Uf::Ap => (
            "www.sefaz.ap.gov.br/nfce/consulta",
            "www.sefaz.ap.gov.br/nfce/consulta",
        ),
        Uf::Ba => (
            "http://www.sefaz.ba.gov.br/nfce/consulta",
            "http://hinternet.sefaz.ba.gov.br/nfce/consulta",
        ),
        Uf::Ce => (
            "www.sefaz.ce.gov.br/nfce/consulta",
            "www.sefaz.ce.gov.br/nfce/consulta",
        ),
        Uf::Df => (
            "www.fazenda.df.gov.br/nfce/consulta",
            "www.fazenda.df.gov.br/nfce/consulta",
        ),
        Uf::Es => (
            "www.sefaz.es.gov.br/nfce/consulta",
            "www.sefaz.es.gov.br/nfce/consulta",
        ),
        Uf::Go => (
            "www.sefaz.go.gov.br/nfce/consulta",
            "www.nfce.go.gov.br/post/ver/214413/consulta-nfc-e-homologacao",
        ),
        Uf::Ma => (
            "www.sefaz.ma.gov.br/nfce/consulta",
            "www.sefaz.ma.gov.br/nfce/consulta",
        ),
        Uf::Mg => (
            "https://portalsped.fazenda.mg.gov.br/portalnfce",
            "https://hportalsped.fazenda.mg.gov.br/portalnfce",
        ),
        Uf::Ms => (
            "http://www.dfe.ms.gov.br/nfce/consulta",
            "http://www.dfe.ms.gov.br/nfce/consulta",
        ),
        Uf::Mt => (
            "http://www.sefaz.mt.gov.br/nfce/consultanfce",
            "http://homologacao.sefaz.mt.gov.br/nfce/consultanfce",
        ),
        Uf::Pa => (
            "www.sefa.pa.gov.br/nfce/consulta",
            "www.sefa.pa.gov.br/nfce/consulta",
        ),
        Uf::Pb => (
            "www.sefaz.pb.gov.br/nfce/consulta",
            "www.sefaz.pb.gov.br/nfcehom",
        ),
        Uf::Pe => (
            "nfce.sefaz.pe.gov.br/nfce/consulta",
            "nfce.sefaz.pe.gov.br/nfce/consulta",
        ),
        Uf::Pi => (
            "www.sefaz.pi.gov.br/nfce/consulta",
            "www.sefaz.pi.gov.br/nfce/consulta",
        ),
        Uf::Pr => (
            "http://www.fazenda.pr.gov.br/nfce/consulta",
            "http://www.fazenda.pr.gov.br/nfce/consulta",
        ),
        Uf::Rj => (
            "www.fazenda.rj.gov.br/nfce/consulta",
            "www.fazenda.rj.gov.br/nfce/consulta",
        ),
        Uf::Rn => (
            "www.set.rn.gov.br/nfce/consulta",
            "www.set.rn.gov.br/nfce/consulta",
        ),
        Uf::Ro => (
            "www.sefin.ro.gov.br/nfce/consulta",
            "www.sefin.ro.gov.br/nfce/consulta",
        ),
        Uf::Rr => (
            "www.sefaz.rr.gov.br/nfce/consulta",
            "www.sefaz.rr.gov.br/nfce/consulta",
        ),
        Uf::Rs => (
            "www.sefaz.rs.gov.br/nfce/consulta",
            "www.sefaz.rs.gov.br/nfce/consulta",
        ),
        Uf::Sc => (
            "https://sat.sef.sc.gov.br/nfce/consulta",
            "https://hom.sat.sef.sc.gov.br/nfce/consulta",
        ),
        Uf::Se => (
            "http://www.nfce.se.gov.br/nfce/consulta",
            "http://www.hom.nfe.se.gov.br/nfce/consulta",
        ),
        Uf::Sp => (
            "https://www.nfce.fazenda.sp.gov.br/NFCeConsultaPublica",
            "https://www.homologacao.nfce.fazenda.sp.gov.br/NFCeConsultaPublica",
        ),
        Uf::To => (
            "www.sefaz.to.gov.br/nfce/consulta",
            "http://homologacao.sefaz.to.gov.br/nfce/consulta.jsf",
        ),
    }
}

#[cfg(test)]
#[path = "nfce_offline_testes.rs"]
mod testes;
