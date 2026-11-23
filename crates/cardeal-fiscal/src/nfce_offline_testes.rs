//! A NFC-e em contingência local contra o **XSD oficial 4.00** (`xmllint`), com a assinatura
//! conferida por um verificador independente (`xmlsec1`) e o QR Code off-line conferido com a
//! chave pública do certificado (`openssl`). Sem as ferramentas, esses testes avisam e passam.

use std::path::{Path, PathBuf};
use std::process::Command;

use cardeal_kernel::{Data, Dinheiro, Fuso, Hora, Instante, Percentual};

use super::*;
use crate::certificado::abrir_para_assinar;
use crate::emissao::{exemplo, Crt};

const SENHA: &str = "cardeal-teste";

fn dados() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/dados")
}

fn chave() -> ChaveDeAssinatura {
    let pfx = std::fs::read(dados().join("aes256.pfx")).unwrap();
    abrir_para_assinar(&pfx, SENHA).unwrap()
}

fn tem(programa: &str) -> bool {
    Command::new(programa).arg("--version").output().is_ok()
}

fn em(h: u32, m: u32) -> Instante {
    Instante::de_data_hora(
        Data::de_ymd(2026, 11, 23).unwrap(),
        Hora::de_hms(h, m, 0).unwrap(),
        Fuso::BRASILIA,
    )
}

fn entrada() -> EntradaEmContingencia {
    EntradaEmContingencia {
        inicio: em(10, 0),
        justificativa: "Sem conexao com a API fiscal - contingencia off-line".into(),
        fuso: Fuso::BRASILIA,
        responsavel_tecnico: None,
    }
}

fn responsavel() -> ResponsavelTecnico {
    ResponsavelTecnico::novo(
        "11222333000181",
        "Fulano de Tal",
        "suporte@exemplo.com.br",
        "51999990000",
    )
    .unwrap()
}

fn montar(nota: &Nfce, ambiente: AmbienteFiscal) -> NotaEmContingencia {
    montar_nfce_em_contingencia(
        nota,
        &entrada(),
        ambiente,
        &responsavel(),
        &chave(),
        em(10, 5),
        12_345_678,
    )
    .unwrap()
}

fn validar(xml: &str, nome: &str) -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("cardeal-nfce-{}-{nome}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let arquivo = dir.join("nfce.xml");
    std::fs::write(&arquivo, xml).unwrap();
    let saida = Command::new("xmllint")
        .args(["--noout", "--nonet", "--schema"])
        .arg(dados().join("nfe_xsd/nfe_v4.00.xsd"))
        .arg(&arquivo)
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    if saida.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&saida.stderr).into_owned())
    }
}

fn conferir_assinatura(xml: &str, nome: &str) -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("cardeal-nfce-dsig-{}-{nome}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (arquivo, certificado) = (dir.join("nfce.xml"), dir.join("cert.der"));
    std::fs::write(&arquivo, xml).unwrap();
    std::fs::write(&certificado, &chave().certificado).unwrap();
    let saida = Command::new("xmlsec1")
        .env("OPENSSL_ENABLE_SHA1_SIGNATURES", "1")
        .args(["--verify", "--id-attr:Id", "infNFe", "--pubkey-cert-der"])
        .arg(&certificado)
        .arg(&arquivo)
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    if saida.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&saida.stderr).into_owned())
    }
}

/// A assinatura dos parâmetros do QR Code confere com a chave pública do certificado.
fn conferir_qr(xml: &str, nome: &str) -> Result<(), String> {
    let qr = trecho(xml, "qrCode");
    let p = &qr[qr.find("?p=").unwrap() + 3..];
    let (sequencia, assinatura) = p.rsplit_once('|').unwrap();
    let dir = std::env::temp_dir().join(format!("cardeal-nfce-qr-{}-{nome}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("cert.der"), &chave().certificado).unwrap();
    std::fs::write(dir.join("dados.txt"), sequencia).unwrap();
    std::fs::write(
        dir.join("assinatura.bin"),
        base64::engine::general_purpose::STANDARD
            .decode(assinatura)
            .unwrap(),
    )
    .unwrap();
    let publica = Command::new("openssl")
        .args(["x509", "-inform", "DER", "-pubkey", "-noout", "-in"])
        .arg(dir.join("cert.der"))
        .output()
        .unwrap();
    std::fs::write(dir.join("pub.pem"), &publica.stdout).unwrap();
    let saida = Command::new("openssl")
        .env("OPENSSL_ENABLE_SHA1_SIGNATURES", "1")
        .args(["dgst", "-sha1", "-verify"])
        .arg(dir.join("pub.pem"))
        .arg("-signature")
        .arg(dir.join("assinatura.bin"))
        .arg(dir.join("dados.txt"))
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    if saida.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&saida.stdout).into_owned()
            + &String::from_utf8_lossy(&saida.stderr))
    }
}

fn trecho<'a>(xml: &'a str, tag: &str) -> &'a str {
    let abre = format!("<{tag}>");
    let i = xml.find(&abre).unwrap() + abre.len();
    let f = i + xml[i..].find(&format!("</{tag}>")).unwrap();
    &xml[i..f]
}

fn regime_normal() -> Nfce {
    let mut n = exemplo();
    n.emitente.crt = Crt::RegimeNormal;
    n.consumidor = None;
    let i = &mut n.itens[0].impostos;
    i.icms.csosn = None;
    i.icms.cst = Some("00".into());
    i.icms.base = Some(Dinheiro::centavos(5_400));
    i.icms.aliquota = Some(Percentual::pontos(17));
    i.icms.valor = Some(Dinheiro::centavos(918));
    i.pis.cst = "01".into();
    i.pis.base = Some(Dinheiro::centavos(5_400));
    i.pis.aliquota = Some(Percentual::milesimos(1_650));
    i.pis.valor = Some(Dinheiro::centavos(89));
    let j = &mut n.itens[1].impostos;
    j.icms.csosn = None;
    j.icms.cst = Some("60".into());
    j.pis.cst = "07".into();
    j.cofins.cst = "07".into();
    n
}

#[test]
fn a_nfce_local_valida_no_xsd_oficial_e_as_assinaturas_conferem() {
    if !tem("xmllint") || !tem("xmlsec1") || !tem("openssl") {
        eprintln!("AVISO: sem xmllint/xmlsec1/openssl — a NFC-e local não foi validada");
        return;
    }
    let casos = [
        (
            "simples-cpf-homologacao",
            exemplo(),
            AmbienteFiscal::Homologacao,
        ),
        ("simples-cpf-producao", exemplo(), AmbienteFiscal::Producao),
        (
            "normal-sem-consumidor",
            regime_normal(),
            AmbienteFiscal::Homologacao,
        ),
    ];
    for (nome, nota, ambiente) in casos {
        let n = montar(&nota, ambiente);
        validar(&n.xml, nome).unwrap_or_else(|e| panic!("{nome}: XSD\n{e}\n{}", n.xml));
        conferir_assinatura(&n.xml, nome).unwrap_or_else(|e| panic!("{nome}: assinatura\n{e}"));
        conferir_qr(&n.xml, nome).unwrap_or_else(|e| panic!("{nome}: QR\n{e}"));
    }
}

#[test]
fn a_chave_e_o_ide_sao_de_contingencia() {
    let nota = exemplo();
    let n = montar(&nota, AmbienteFiscal::Homologacao);
    assert_eq!(n.chave.len(), 44);
    assert!(crate::emissao::chave_de_contingencia(&n.chave));
    assert!(ChaveAcesso::nova(&n.chave).unwrap().dv_confere());
    assert_eq!(&n.chave[2..6], "2611", "AAMM da emissão");
    assert_eq!(&n.chave[20..22], "65");
    assert_eq!(&n.chave[35..43], "12345678", "o cNF");
    assert!(n
        .xml
        .contains(&format!("<infNFe Id=\"NFe{}\" versao=\"4.00\">", n.chave)));
    assert!(n.xml.contains("<tpEmis>9</tpEmis>"));
    assert!(n.xml.contains("<dhEmi>2026-11-23T10:05:00-03:00</dhEmi>"));
    assert!(n.xml.contains(
        "<dhCont>2026-11-23T10:00:00-03:00</dhCont><xJust>Sem conexao com a API fiscal - contingencia off-line</xJust></ide>"
    ));
    // O infNFeSupl fica entre o infNFe e a Signature.
    let supl = n.xml.find("<infNFeSupl>").unwrap();
    assert!(n.xml.find("</infNFe>").unwrap() < supl);
    assert!(supl < n.xml.find("<Signature").unwrap());
}

#[test]
fn o_qr_code_off_line_tem_os_parametros_da_nota() {
    let n = montar(&exemplo(), AmbienteFiscal::Homologacao);
    let qr = trecho(&n.xml, "qrCode");
    // exemplo(): 59,80 + 15,00 − 5,80 = 69,00; CPF 52998224725 na nota; dia 23.
    let esperado = format!(
        "https://www.sefaz.rs.gov.br/NFCE/NFCE-COM.aspx?p={}|3|2|23|69.00|2|",
        n.chave
    );
    assert!(qr.starts_with(&esperado), "{qr}");
    assert!(n.xml.contains("<vNF>69.00</vNF>"));
    assert_eq!(
        trecho(&n.xml, "urlChave"),
        "www.sefaz.rs.gov.br/nfce/consulta"
    );

    let mut sem = exemplo();
    sem.consumidor = None;
    let n = montar(&sem, AmbienteFiscal::Producao);
    assert!(
        trecho(&n.xml, "qrCode").contains("|3|1|23|69.00|||"),
        "sem consumidor, os campos vazios"
    );
    assert!(!n.xml.contains("<dest>"));
}

#[test]
fn em_homologacao_o_nome_do_consumidor_e_o_da_sefaz() {
    let mut nota = exemplo();
    nota.consumidor.as_mut().unwrap().nome = Some("Maria".into());
    let n = montar(&nota, AmbienteFiscal::Homologacao);
    assert!(n.xml.contains(&format!(
        "<xNome>{NOME_EM_HOMOLOGACAO}</xNome><indIEDest>9</indIEDest>"
    )));
    let n = montar(&nota, AmbienteFiscal::Producao);
    assert!(n
        .xml
        .contains("<xNome>Maria</xNome><indIEDest>9</indIEDest>"));
}

#[test]
fn o_que_a_contingencia_local_nao_monta_e_recusado() {
    let mut nota = exemplo();
    nota.itens[0].impostos.icms.csosn = Some("900".into());
    let e = montar_nfce_em_contingencia(
        &nota,
        &entrada(),
        AmbienteFiscal::Homologacao,
        &responsavel(),
        &chave(),
        em(10, 5),
        1,
    )
    .unwrap_err();
    assert!(e.to_string().contains("CSOSN 900"), "{e}");

    let mut curta = entrada();
    curta.justificativa = "sem rede".into();
    let e = montar_nfce_em_contingencia(
        &exemplo(),
        &curta,
        AmbienteFiscal::Homologacao,
        &responsavel(),
        &chave(),
        em(10, 5),
        1,
    )
    .unwrap_err();
    assert!(e.to_string().contains("justificativa"), "{e}");
}

#[test]
fn o_cnf_nunca_e_o_numero() {
    let nota = exemplo();
    let n = montar_nfce_em_contingencia(
        &nota,
        &entrada(),
        AmbienteFiscal::Homologacao,
        &responsavel(),
        &chave(),
        em(10, 5),
        u32::try_from(nota.numero).unwrap(),
    )
    .unwrap();
    assert_ne!(n.chave[35..43].parse::<u64>().unwrap(), nota.numero);
}

#[test]
fn o_responsavel_tecnico_sai_da_nota_autorizada() {
    let xml =
        "<NFe><infNFe><infRespTec><CNPJ>11222333000181</CNPJ><xContato>Fulano &amp; Cia</xContato>\
               <email>a@b.com</email><fone>51999990000</fone></infRespTec></infNFe></NFe>";
    let r = ResponsavelTecnico::do_xml(xml).unwrap();
    assert_eq!(r.contato, "Fulano & Cia");
    assert_eq!(r.fone, "51999990000");
    assert!(ResponsavelTecnico::do_xml("<NFe></NFe>").is_none());
    assert!(
        ResponsavelTecnico::novo("123", "x", "a@b", "51999990000").is_none(),
        "CNPJ curto"
    );
}

#[test]
fn valores_com_as_casas_do_leiaute() {
    assert_eq!(centavos(6_900), "69.00");
    assert_eq!(centavos(-5), "-0.05");
    assert_eq!(fixo(20_000, 4, 4), "2.0000");
    assert_eq!(fixo(29_900_000, 6, 10), "29.9000000000");
    assert_eq!(fixo(17_000_000, 6, 2), "17.00");
    assert_eq!(fixo(1_650_000, 6, 4), "1.6500");
    assert_eq!(fixo(17_125_000, 6, 2), "17.13", "meio para longe do zero");
}

/// Exporta, para a conferência com o montador do `sefaz-api` (o pedido JSON que o Cardeal
/// manda ao `/contingencia/prepare`, a chave e o XML local): `CARDEAL_EXPORTAR_NFCE=<pasta>`.
#[test]
#[ignore = "manual: gera os arquivos da conferência com o sefaz-api"]
fn exportar_para_conferir_com_o_sefaz_api() {
    let pasta = PathBuf::from(std::env::var("CARDEAL_EXPORTAR_NFCE").unwrap());
    std::fs::create_dir_all(&pasta).unwrap();
    let csc = crate::emissao::Csc {
        id: "1".into(),
        token: cardeal_storage::Segredo::novo("ABCDEF0123456789".to_owned()),
    };
    let mut com_nome = exemplo();
    com_nome.consumidor.as_mut().unwrap().nome = Some("Maria & Filhos".into());
    com_nome.informacoes = Some("Obrigado pela preferencia <volte sempre>".into());
    com_nome.emitente.endereco.telefone = Some("(51) 3333-4444".into());
    let casos = [
        (
            "simples-homologacao",
            exemplo(),
            AmbienteFiscal::Homologacao,
        ),
        ("simples-producao-nome", com_nome, AmbienteFiscal::Producao),
        (
            "normal-sem-consumidor",
            regime_normal(),
            AmbienteFiscal::Homologacao,
        ),
    ];
    for (nome, nota, ambiente) in casos {
        let n = montar(&nota, ambiente);
        let mut pedido = crate::sefaz_api::nfce_json(&nota, &csc);
        pedido.insert(
            "contingencia".into(),
            serde_json::json!({
                "dhCont": "2026-11-23T10:00:00-03:00",
                "xJust": entrada().justificativa,
            }),
        );
        let tudo = serde_json::json!({
            "pedido": pedido,
            "chave": n.chave,
            "tpAmb": if ambiente == AmbienteFiscal::Producao { "1" } else { "2" },
            "xml": n.xml,
        });
        std::fs::write(pasta.join(format!("{nome}.json")), tudo.to_string()).unwrap();
    }
}
