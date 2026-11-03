//! O XML que o simulador devolve: um `nfeProc` com o que o `sefaz-api` monta — `ide`, emitente,
//! destinatário, itens com ICMS/PIS/COFINS e totais. Não é assinado nem validado pelo esquema; é
//! o bastante para quem lê a nota autorizada (o DANFE, a escrituração) ser testado sem a SEFAZ.

use std::fmt::Write as _;

use cardeal_kernel::{Dinheiro, Instante, Percentual, Preco, Quantidade};

use crate::emissao::{
    Consumidor, Destinatario, DocumentoConsumidor, EnderecoEmitente, ItemDocumento, Nfce, Nfe,
};

/// O que a NF-e tem a mais que a NFC-e, no XML.
pub(crate) struct Complemento {
    /// `55` ou `65`.
    pub modelo: &'static str,
    /// `tpNF`: 1 saída, 0 entrada.
    pub tp_nf: &'static str,
    /// `finNFe`.
    pub fin_nfe: &'static str,
    /// O grupo `dest`, pronto (vazio sem destinatário).
    pub dest: String,
    /// O `vNF` (a NF-e de devolução soma o ST e o IPI devolvidos).
    pub total: Dinheiro,
}

impl Complemento {
    /// A NFC-e: saída normal, o consumidor (se identificado) como destinatário.
    pub(crate) fn da_nfce(nota: &Nfce) -> Self {
        Self {
            modelo: "65",
            tp_nf: "1",
            fin_nfe: "1",
            dest: nota
                .consumidor
                .as_ref()
                .map(dest_consumidor)
                .unwrap_or_default(),
            total: nota.total(),
        }
    }

    /// A NF-e: o destinatário com o endereço e a finalidade da nota.
    pub(crate) fn da_nfe(nota: &Nfe) -> Self {
        Self {
            modelo: "55",
            tp_nf: if nota.saida { "1" } else { "0" },
            fin_nfe: nota.finalidade.codigo(),
            dest: dest_nfe(&nota.destinatario),
            total: nota.total(),
        }
    }
}

fn escapar(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn decimal(unidades: i64, escala: i64, casas: u32) -> String {
    let divisor = escala / 10_i64.pow(casas);
    let v = unidades / divisor.max(1);
    let base = 10_i64.pow(casas);
    format!(
        "{}{}.{:0width$}",
        if v < 0 { "-" } else { "" },
        (v / base).abs(),
        (v % base).abs(),
        width = casas as usize
    )
}

fn dinheiro(d: Dinheiro) -> String {
    decimal(d.em_centavos(), 100, 2)
}

fn quantidade(q: Quantidade) -> String {
    decimal(q.unidades_internas(), 10_000, 4)
}

fn preco(p: Preco) -> String {
    decimal(p.unidades_internas(), 1_000_000, 6)
}

fn percentual(p: Percentual) -> String {
    decimal(p.unidades_internas(), 1_000_000, 4)
}

fn documento(d: &DocumentoConsumidor) -> String {
    match d {
        DocumentoConsumidor::Cpf(c) => format!("<CPF>{}</CPF>", c.sem_mascara()),
        DocumentoConsumidor::Cnpj(c) => format!("<CNPJ>{}</CNPJ>", c.sem_mascara()),
    }
}

fn endereco(tag: &str, e: &EnderecoEmitente) -> String {
    format!(
        "<{tag}><xLgr>{}</xLgr><nro>{}</nro>{}<xBairro>{}</xBairro><cMun>{}</cMun>\
         <xMun>{}</xMun><UF>{}</UF><CEP>{}</CEP><cPais>1058</cPais><xPais>BRASIL</xPais></{tag}>",
        escapar(&e.logradouro),
        escapar(&e.numero),
        if e.complemento.is_empty() {
            String::new()
        } else {
            format!("<xCpl>{}</xCpl>", escapar(&e.complemento))
        },
        escapar(&e.bairro),
        e.codigo_municipio,
        escapar(&e.municipio),
        e.uf.sigla(),
        e.cep
    )
}

fn dest_consumidor(c: &Consumidor) -> String {
    format!(
        "<dest>{}{}<indIEDest>9</indIEDest></dest>",
        documento(&c.documento),
        c.nome
            .as_ref()
            .map(|n| format!("<xNome>{}</xNome>", escapar(n)))
            .unwrap_or_default()
    )
}

fn dest_nfe(d: &Destinatario) -> String {
    format!(
        "<dest>{}<xNome>{}</xNome>{}<indIEDest>{}</indIEDest>{}</dest>",
        documento(&d.documento),
        escapar(&d.nome),
        endereco("enderDest", &d.endereco),
        d.indicador_ie.codigo(),
        d.inscricao_estadual
            .as_ref()
            .map(|ie| format!("<IE>{ie}</IE>"))
            .unwrap_or_default()
    )
}

/// O grupo do ICMS do item, pelo CST (regime normal) ou pelo CSOSN (Simples).
fn icms(item: &ItemDocumento) -> String {
    let i = &item.impostos.icms;
    let valores = format!(
        "{}{}{}",
        i.base
            .map(|b| format!("<modBC>3</modBC><vBC>{}</vBC>", dinheiro(b)))
            .unwrap_or_default(),
        i.aliquota
            .map(|a| format!("<pICMS>{}</pICMS>", percentual(a)))
            .unwrap_or_default(),
        i.valor
            .map(|v| format!("<vICMS>{}</vICMS>", dinheiro(v)))
            .unwrap_or_default()
    );
    let reducao = i
        .reducao_base
        .map(|r| format!("<pRedBC>{}</pRedBC>", percentual(r)))
        .unwrap_or_default();
    match (&i.cst, &i.csosn) {
        (Some(cst), _) => {
            let grupo = match cst.as_str() {
                "41" | "50" => "40",
                c => c,
            };
            format!(
                "<ICMS{grupo}><orig>{}</orig><CST>{cst}</CST>{reducao}{valores}</ICMS{grupo}>",
                i.origem
            )
        }
        (None, Some(csosn)) => {
            let grupo = match csosn.as_str() {
                "103" | "300" | "400" => "102",
                "202" | "203" => "201",
                c => c,
            };
            format!(
                "<ICMSSN{grupo}><orig>{}</orig><CSOSN>{csosn}</CSOSN>{valores}</ICMSSN{grupo}>",
                i.origem
            )
        }
        (None, None) => String::new(),
    }
}

fn contribuicao(tag: &str, c: &crate::emissao::ContribuicaoItem) -> String {
    let grupo = match c.cst.as_str() {
        "01" | "02" => "Aliq",
        "04" | "05" | "06" | "07" | "08" | "09" => "NT",
        _ => "Outr",
    };
    format!(
        "<{tag}><{tag}{grupo}><CST>{}</CST>{}{}{}</{tag}{grupo}></{tag}>",
        c.cst,
        c.base
            .map(|b| format!("<vBC>{}</vBC>", dinheiro(b)))
            .unwrap_or_default(),
        c.aliquota
            .map(|a| format!("<p{tag}>{}</p{tag}>", percentual(a)))
            .unwrap_or_default(),
        c.valor
            .map(|v| format!("<v{tag}>{}</v{tag}>", dinheiro(v)))
            .unwrap_or_default()
    )
}

fn det(n: usize, item: &ItemDocumento) -> String {
    format!(
        "<det nItem=\"{n}\"><prod><cProd>{}</cProd><cEAN>{}</cEAN><xProd>{}</xProd><NCM>{}</NCM>\
         {}<CFOP>{}</CFOP><uCom>{}</uCom><qCom>{}</qCom><vUnCom>{}</vUnCom><vProd>{}</vProd>\
         {}<indTot>1</indTot></prod><imposto><ICMS>{}</ICMS>{}{}</imposto></det>",
        escapar(&item.codigo),
        item.gtin.as_deref().unwrap_or("SEM GTIN"),
        escapar(&item.descricao),
        item.ncm,
        item.cest
            .as_ref()
            .map(|c| format!("<CEST>{c}</CEST>"))
            .unwrap_or_default(),
        item.cfop,
        escapar(&item.unidade),
        quantidade(item.quantidade),
        preco(item.valor_unitario),
        dinheiro(item.valor_bruto),
        if item.desconto.e_positivo() {
            format!("<vDesc>{}</vDesc>", dinheiro(item.desconto))
        } else {
            String::new()
        },
        icms(item),
        contribuicao("PIS", &item.impostos.pis),
        contribuicao("COFINS", &item.impostos.cofins),
    )
}

/// A `NFe` "assinada" (sem assinatura), com a emissão (agora, em Brasília) e o grupo do QR
/// Code — o bastante para guardar, imprimir, escriturar e mandar ao contador em teste.
pub(crate) fn nfe_assinada(chave: &str, nota: &Nfce, c: &Complemento) -> String {
    let agora = Instante::agora();
    let fuso = cardeal_kernel::Fuso::BRASILIA;
    let e = &nota.emitente;
    let soma = |f: &dyn Fn(&ItemDocumento) -> Option<Dinheiro>| -> Dinheiro {
        nota.itens.iter().filter_map(f).sum()
    };
    let v_bc = soma(&|i| i.impostos.icms.base);
    let v_icms = soma(&|i| i.impostos.icms.valor);
    let v_pis = soma(&|i| i.impostos.pis.valor);
    let v_cofins = soma(&|i| i.impostos.cofins.valor);
    let v_prod: Dinheiro = nota.itens.iter().map(|i| i.valor_bruto).sum();
    let v_desc: Dinheiro = nota.itens.iter().map(|i| i.desconto).sum();
    let mut dets = String::new();
    for (n, item) in nota.itens.iter().enumerate() {
        dets.push_str(&det(n + 1, item));
    }
    let mut x = String::new();
    let _ = write!(
        x,
        "<NFe><infNFe Id=\"NFe{chave}\" versao=\"4.00\"><ide><cUF>{}</cUF><natOp>VENDA</natOp>\
         <mod>{}</mod><serie>{}</serie><nNF>{}</nNF><dhEmi>{}T{}-03:00</dhEmi><tpNF>{}</tpNF>\
         <finNFe>{}</finNFe></ide><emit><CNPJ>{}</CNPJ><xNome>{}</xNome><xFant>{}</xFant>{}\
         <IE>{}</IE><CRT>{}</CRT></emit>{}{dets}<total><ICMSTot><vBC>{}</vBC><vICMS>{}</vICMS>\
         <vICMSDeson>0.00</vICMSDeson><vFCP>0.00</vFCP><vBCST>0.00</vBCST><vST>0.00</vST>\
         <vFCPST>0.00</vFCPST><vFCPSTRet>0.00</vFCPSTRet><vProd>{}</vProd><vFrete>0.00</vFrete>\
         <vSeg>0.00</vSeg><vDesc>{}</vDesc><vII>0.00</vII><vIPI>0.00</vIPI>\
         <vIPIDevol>0.00</vIPIDevol><vPIS>{}</vPIS><vCOFINS>{}</vCOFINS><vOutro>0.00</vOutro>\
         <vNF>{}</vNF></ICMSTot></total><transp><modFrete>9</modFrete></transp></infNFe>",
        e.endereco.uf.codigo_ibge(),
        c.modelo,
        nota.serie,
        nota.numero,
        agora.data(fuso).formatar_iso(),
        agora.hora(fuso).formatar_com_segundos(),
        c.tp_nf,
        c.fin_nfe,
        e.cnpj.sem_mascara(),
        escapar(&e.razao_social),
        escapar(&e.nome_fantasia),
        endereco("enderEmit", &e.endereco),
        e.inscricao_estadual,
        e.crt.codigo(),
        c.dest,
        dinheiro(v_bc),
        dinheiro(v_icms),
        dinheiro(v_prod),
        dinheiro(v_desc),
        dinheiro(v_pis),
        dinheiro(v_cofins),
        dinheiro(c.total),
    );
    let _ = write!(
        x,
        "<infNFeSupl><qrCode>https://homologacao.simulado/nfce?p={chave}|3|2</qrCode>\
         <urlChave>https://homologacao.simulado/nfce</urlChave></infNFeSupl></NFe>"
    );
    x
}

/// Um `nfeProc`: a nota "assinada" com o protocolo.
pub(crate) fn xml_simulado(chave: &str, nota: &Nfce, c: &Complemento) -> String {
    format!(
        "<nfeProc versao=\"4.00\">{}<protNFe><infProt><chNFe>{chave}</chNFe><cStat>100</cStat>\
         </infProt></protNFe></nfeProc>",
        nfe_assinada(chave, nota, c)
    )
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn decimais_no_formato_do_xml() {
        assert_eq!(dinheiro(Dinheiro::centavos(12_345)), "123.45");
        assert_eq!(dinheiro(Dinheiro::centavos(5)), "0.05");
        assert_eq!(quantidade(Quantidade::milesimos(1_500)), "1.5000");
        assert_eq!(percentual(Percentual::pontos(18)), "18.0000");
        assert_eq!(preco(Preco::reais(4)), "4.000000");
    }
}
