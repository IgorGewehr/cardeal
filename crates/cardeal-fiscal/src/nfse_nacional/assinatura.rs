//! A assinatura **`XMLDSig` enveloped** do Sistema Nacional NFS-e (manual de integração, §6.1.4):
//! referência ao `Id` do grupo assinado, transformações `enveloped-signature` + C14N 1.0,
//! `rsa-sha1` com resumo `sha1`, e só o certificado do titular no `KeyInfo` (`EndCertOnly`; sem
//! `X509SubjectName`, `X509IssuerSerial` nem `KeyValue`).
//!
//! O grupo assinado (`infDPS`, `infPedReg`) chega já canônico (ver [`super::xml`]); a forma
//! canônica dele como subconjunto do documento é a mesma com o `xmlns` herdado do pai — que o
//! C14N inclusivo escreve no elemento do topo.

use std::fmt::Write as _;

use base64::Engine as _;
use sha1::{Digest, Sha1};

use crate::certificado::{AlgoritmoAssinatura, ChaveDeAssinatura};
use crate::erros::ErroFiscal;

/// O namespace do `XMLDSig`.
pub(crate) const NS_DSIG: &str = "http://www.w3.org/2000/09/xmldsig#";
const C14N: &str = "http://www.w3.org/TR/2001/REC-xml-c14n-20010315";
const ENVELOPED: &str = "http://www.w3.org/2000/09/xmldsig#enveloped-signature";
const RSA_SHA1: &str = "http://www.w3.org/2000/09/xmldsig#rsa-sha1";
const SHA1: &str = "http://www.w3.org/2000/09/xmldsig#sha1";

/// O documento assinado: `<raiz xmlns=ns atributos>grupo<Signature>…</Signature></raiz>`, com a
/// declaração XML (UTF-8, como a Sefin exige).
///
/// `grupo` é o elemento assinado, canônico e sem `xmlns` (herdado da raiz); `id` é o valor do
/// atributo `Id` dele.
///
/// # Errors
/// [`ErroFiscal::CertificadoInvalido`] se a operação RSA falhar.
pub(crate) fn assinar_enveloped(
    raiz: &str,
    ns: &str,
    atributos_da_raiz: &[(&str, &str)],
    grupo: &str,
    id: &str,
    chave: &ChaveDeAssinatura,
) -> Result<String, ErroFiscal> {
    assinar_enveloped_com(raiz, ns, atributos_da_raiz, grupo, "", id, chave)
}

/// Como [`assinar_enveloped`], com `depois_do_grupo` entre o grupo assinado e a `Signature`
/// — fora da assinatura (o `infNFeSupl` da NFC-e, com o QR Code).
///
/// # Errors
/// [`ErroFiscal::CertificadoInvalido`] se a operação RSA falhar.
pub(crate) fn assinar_enveloped_com(
    raiz: &str,
    ns: &str,
    atributos_da_raiz: &[(&str, &str)],
    grupo: &str,
    depois_do_grupo: &str,
    id: &str,
    chave: &ChaveDeAssinatura,
) -> Result<String, ErroFiscal> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let canonico = com_namespace(grupo, ns);
    let resumo = b64.encode(Sha1::digest(canonico.as_bytes()));
    let signed_info = |com_ns: bool| {
        let ns_attr = if com_ns {
            format!(" xmlns=\"{NS_DSIG}\"")
        } else {
            String::new()
        };
        format!(
            "<SignedInfo{ns_attr}>\
             <CanonicalizationMethod Algorithm=\"{C14N}\"></CanonicalizationMethod>\
             <SignatureMethod Algorithm=\"{RSA_SHA1}\"></SignatureMethod>\
             <Reference URI=\"#{id}\"><Transforms>\
             <Transform Algorithm=\"{ENVELOPED}\"></Transform>\
             <Transform Algorithm=\"{C14N}\"></Transform>\
             </Transforms><DigestMethod Algorithm=\"{SHA1}\"></DigestMethod>\
             <DigestValue>{resumo}</DigestValue></Reference></SignedInfo>"
        )
    };
    let valor = b64.encode(chave.assinar(AlgoritmoAssinatura::Sha1, signed_info(true).as_bytes())?);
    let certificado = b64.encode(&chave.certificado);
    let mut abertura = format!("<{raiz} xmlns=\"{ns}\"");
    for (k, v) in atributos_da_raiz {
        let _ = write!(abertura, " {k}=\"{v}\"");
    }
    Ok(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>{abertura}>{grupo}{depois_do_grupo}\
         <Signature xmlns=\"{NS_DSIG}\">{}<SignatureValue>{valor}</SignatureValue>\
         <KeyInfo><X509Data><X509Certificate>{certificado}</X509Certificate></X509Data></KeyInfo>\
         </Signature></{raiz}>",
        signed_info(false)
    ))
}

/// O elemento com o `xmlns` do pai declarado nele (como o C14N do subconjunto o escreve: antes
/// dos atributos).
fn com_namespace(grupo: &str, ns: &str) -> String {
    let fim_do_nome = grupo.find([' ', '>']).unwrap_or(grupo.len());
    format!(
        "{} xmlns=\"{ns}\"{}",
        &grupo[..fim_do_nome],
        &grupo[fim_do_nome..]
    )
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_xmlns_entra_antes_dos_atributos() {
        assert_eq!(
            com_namespace("<infDPS Id=\"DPS1\"><a>1</a></infDPS>", "urn:x"),
            "<infDPS xmlns=\"urn:x\" Id=\"DPS1\"><a>1</a></infDPS>"
        );
    }
}
