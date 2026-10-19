//! O certificado digital A1 (PKCS#12, `.pfx`/`.p12`) — lido para conferir, **antes** de
//! guardar, que a senha está certa, de quem ele é (o CNPJ do e-CNPJ) e até quando vale.
//!
//! O Cardeal não assina nada: quem assina é o `sefaz-api` (ADR-0006), que recebe o arquivo e a
//! senha a cada chamada. Por isso aqui só se decifra a parte pública (o certificado); a chave
//! privada nem é aberta.
//!
//! Formatos aceitos (os que as ACs e o Windows/OpenSSL exportam): PBES2 (PBKDF2 com
//! HMAC-SHA1/256/384/512, AES-128/192/256-CBC ou 3DES-CBC) e PBES1 do PKCS#12 (3DES, RC2-40,
//! RC2-128), com MAC HMAC-SHA1/256/384/512.

use std::borrow::Cow;
use std::fmt::Write as _;

use aes::cipher::block_padding::Pkcs7;
use aes::cipher::{BlockDecryptMut, BlockEncryptMut, InnerIvInit, KeyIvInit};
use cardeal_kernel::{Cnpj, Data, Fuso, Hora, Instante};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::digest::core_api::BlockSizeUser;
use sha2::{Digest, Sha256, Sha384, Sha512};

use zeroize::Zeroizing;

use crate::der::{
    elemento, inteiro_der, invalido, octetos_der, oid_der, sem_zeros_a_esquerda, sequencia, tag,
    FormatoInvalido, Res, Tlv,
};
use crate::erros::ErroFiscal;

mod oid {
    pub const DADOS: &str = "1.2.840.113549.1.7.1";
    pub const DADOS_CIFRADOS: &str = "1.2.840.113549.1.7.6";
    pub const BOLSA_CHAVE: &str = "1.2.840.113549.1.12.10.1.1";
    pub const BOLSA_CHAVE_CIFRADA: &str = "1.2.840.113549.1.12.10.1.2";
    pub const BOLSA_CERTIFICADO: &str = "1.2.840.113549.1.12.10.1.3";
    pub const BOLSA_ANINHADA: &str = "1.2.840.113549.1.12.10.1.6";
    pub const CERTIFICADO_X509: &str = "1.2.840.113549.1.9.22.1";
    pub const ID_LOCAL_DA_CHAVE: &str = "1.2.840.113549.1.9.21";

    pub const PBE_SHA1_3DES: &str = "1.2.840.113549.1.12.1.3";
    pub const PBE_SHA1_RC2_128: &str = "1.2.840.113549.1.12.1.5";
    pub const PBE_SHA1_RC2_40: &str = "1.2.840.113549.1.12.1.6";
    pub const PBES2: &str = "1.2.840.113549.1.5.13";
    pub const PBKDF2: &str = "1.2.840.113549.1.5.12";
    pub const HMAC_SHA1: &str = "1.2.840.113549.2.7";
    pub const HMAC_SHA256: &str = "1.2.840.113549.2.9";
    pub const HMAC_SHA384: &str = "1.2.840.113549.2.10";
    pub const HMAC_SHA512: &str = "1.2.840.113549.2.11";
    pub const AES128_CBC: &str = "2.16.840.1.101.3.4.1.2";
    pub const AES192_CBC: &str = "2.16.840.1.101.3.4.1.22";
    pub const AES256_CBC: &str = "2.16.840.1.101.3.4.1.42";
    pub const DES_EDE3_CBC: &str = "1.2.840.113549.3.7";

    pub const SHA1: &str = "1.3.14.3.2.26";
    pub const SHA256: &str = "2.16.840.1.101.3.4.2.1";
    pub const SHA384: &str = "2.16.840.1.101.3.4.2.2";
    pub const SHA512: &str = "2.16.840.1.101.3.4.2.3";

    pub const NOME_COMUM: &str = "2.5.4.3";
    pub const NOMES_ALTERNATIVOS: &str = "2.5.29.17";
    pub const RESTRICOES_BASICAS: &str = "2.5.29.19";
    /// ICP-Brasil: o CNPJ da pessoa jurídica titular do e-CNPJ.
    pub const ICP_BRASIL_CNPJ: &str = "2.16.76.1.3.3";
}

/// Teto de iterações das derivações de chave — um arquivo hostil não trava a máquina. Os
/// exportadores reais usam de 2.000 a algumas centenas de milhares.
const ITERACOES_MAXIMAS: u64 = 5_000_000;

/// O que se sabe de um certificado A1 depois de aberto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfoCertificado {
    /// O nome do titular (o `CN`, sem o `:CNPJ` que as ACs brasileiras penduram no fim).
    pub titular: String,
    /// O CNPJ do titular, quando é um e-CNPJ (a extensão ICP-Brasil `2.16.76.1.3.3`, ou o
    /// `:CNPJ` do fim do `CN`).
    pub cnpj: Option<Cnpj>,
    /// O nome da autoridade certificadora que o emitiu.
    pub emissor: String,
    /// O número de série, em hexadecimal.
    pub numero_serie: String,
    /// Início da validade.
    pub valido_de: Instante,
    /// Fim da validade.
    pub valido_ate: Instante,
}

impl InfoCertificado {
    /// Verdadeiro se já venceu em `agora`.
    #[must_use]
    pub fn vencido(&self, agora: Instante) -> bool {
        agora > self.valido_ate
    }

    /// Quantos dias faltam para vencer (negativo: venceu há tantos dias).
    #[must_use]
    pub fn dias_para_vencer(&self, hoje: Data, fuso: Fuso) -> i32 {
        hoje.dias_ate(self.valido_ate.data(fuso))
    }
}

/// Abre um certificado A1 e lê o que interessa dele.
///
/// # Errors
/// [`ErroFiscal::SenhaDoCertificadoIncorreta`] se a senha não abre o arquivo;
/// [`ErroFiscal::CertificadoInvalido`] se o arquivo não é um PKCS#12 legível (ou usa um
/// algoritmo que nenhum exportador atual produz).
pub fn ler_certificado_a1(pfx: &[u8], senha: &str) -> Result<InfoCertificado, ErroFiscal> {
    abrir(pfx, senha)
        .and_then(|c| Ok(ler_x509(&escolher_folha(&c)?)?))
        .map_err(Falha::em_erro_fiscal)
}

/// O certificado pronto para guardar e mandar ao `sefaz-api`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertificadoPreparado {
    /// O que se leu dele.
    pub info: InfoCertificado,
    /// Os bytes a guardar: o arquivo original, ou o regravado (ver [`Self::convertido`]).
    pub pfx: Vec<u8>,
    /// Verdadeiro se o arquivo usava cifras antigas (RC2) e foi regravado.
    pub convertido: bool,
}

/// Abre o certificado como [`ler_certificado_a1`] e, se ele usa RC2 — o padrão de exportação
/// antigo, que o OpenSSL 3 (e portanto o TLS do `sefaz-api`) recusa com "Unsupported PKCS12
/// PFX data" —, regrava com a mesma senha: certificados em claro (são públicos), chave em
/// 3DES, MAC HMAC-SHA256. É o que o `saas-erp` faz no envio, e o que funciona em produção.
/// Um arquivo que já está num formato aceito volta intocado.
///
/// # Errors
/// Os mesmos de [`ler_certificado_a1`].
pub fn preparar_certificado_a1(
    pfx: &[u8],
    senha: &str,
) -> Result<CertificadoPreparado, ErroFiscal> {
    let preparar = || -> Result<CertificadoPreparado, Falha> {
        let conteudo = abrir(pfx, senha)?;
        let info = ler_x509(&escolher_folha(&conteudo)?)?;
        if !conteudo.legado {
            return Ok(CertificadoPreparado {
                info,
                pfx: pfx.to_vec(),
                convertido: false,
            });
        }
        Ok(CertificadoPreparado {
            info,
            pfx: regravar(&conteudo, senha)?,
            convertido: true,
        })
    };
    preparar().map_err(Falha::em_erro_fiscal)
}

enum Falha {
    Formato(FormatoInvalido),
    Senha,
}

impl Falha {
    fn em_erro_fiscal(self) -> ErroFiscal {
        match self {
            Self::Senha => ErroFiscal::SenhaDoCertificadoIncorreta,
            Self::Formato(FormatoInvalido(motivo)) => ErroFiscal::CertificadoInvalido(motivo),
        }
    }
}

impl From<FormatoInvalido> for Falha {
    fn from(f: FormatoInvalido) -> Self {
        Self::Formato(f)
    }
}

fn formato<T>(motivo: impl Into<String>) -> Result<T, Falha> {
    Err(Falha::Formato(FormatoInvalido(motivo.into())))
}

/// Um certificado encontrado no arquivo, com o `localKeyId` que o liga à chave.
struct CertificadoNaBolsa {
    der: Vec<u8>,
    id_local: Option<Vec<u8>>,
    /// A `SafeBag` inteira, para regravar.
    bolsa: Vec<u8>,
}

/// Uma chave privada (ainda cifrada) encontrada no arquivo.
struct ChaveNaBolsa {
    id_local: Option<Vec<u8>>,
    bolsa: Vec<u8>,
    /// Cifrada com RC2 — o OpenSSL 3 não abre.
    legada: bool,
}

#[derive(Default)]
struct Conteudo {
    certificados: Vec<CertificadoNaBolsa>,
    chaves: Vec<ChaveNaBolsa>,
    /// Alguma parte usa uma cifra que o OpenSSL 3 recusa.
    legado: bool,
}

/// As cifras do PKCS#12 que o OpenSSL 3 só abre com o provedor "legacy".
fn cifra_legada(oid: &str) -> bool {
    matches!(oid, oid::PBE_SHA1_RC2_40 | oid::PBE_SHA1_RC2_128)
}

fn abrir(pfx: &[u8], senha: &str) -> Result<Conteudo, Falha> {
    let raiz = Tlv::unico(pfx)
        .and_then(|t| t.exigir(tag::SEQUENCIA, "PFX"))
        .map_err(|_| {
            Falha::Formato(FormatoInvalido(
                "o arquivo não é um certificado A1 (.pfx/.p12)".into(),
            ))
        })?;
    let partes = raiz.filhos()?;
    let [versao, cofre, resto @ ..] = partes.as_slice() else {
        return formato("PFX incompleto");
    };
    if versao.inteiro()? != 3 {
        return formato("versão de PKCS#12 não suportada");
    }
    let cofre = conteudo_de_dados(*cofre)?;
    let senha_bmp = senha_bmp(senha);
    let mac_conferido = match resto.first() {
        Some(mac) => {
            if !mac_confere(*mac, &cofre, &senha_bmp)? {
                return Err(Falha::Senha);
            }
            true
        }
        None => false,
    };

    let mut conteudo = Conteudo::default();
    for info in Tlv::unico(&cofre)?
        .exigir(tag::SEQUENCIA, "AuthenticatedSafe")?
        .filhos()?
    {
        let [tipo, explicito] = info.filhos()?[..] else {
            return formato("ContentInfo malformado");
        };
        match tipo.oid()?.as_str() {
            oid::DADOS => {
                let bolsas = Tlv::unico(explicito.valor)?.octetos()?;
                ler_bolsas(&bolsas, &mut conteudo)?;
            }
            oid::DADOS_CIFRADOS => {
                let (bolsas, legado) = match decifrar_dados(explicito, senha, &senha_bmp) {
                    // Sem MAC para conferir a senha, um enchimento inválido é a senha errada.
                    Err(Falha::Formato(_)) if !mac_conferido => return Err(Falha::Senha),
                    outro => outro?,
                };
                conteudo.legado |= legado;
                ler_bolsas(&bolsas, &mut conteudo)?;
            }
            // Envelope com chave pública (modo de privacidade por chave pública): não existe
            // em certificado A1 de AC brasileira — ignorado.
            _ => {}
        }
    }
    conteudo.legado |= conteudo.chaves.iter().any(|c| c.legada);
    Ok(conteudo)
}

/// Regrava o PKCS#12 sem RC2 (ver [`preparar_certificado_a1`]).
fn regravar(conteudo: &Conteudo, senha: &str) -> Result<Vec<u8>, Falha> {
    let senha_bmp = senha_bmp(senha);
    let mut chaves = Vec::with_capacity(conteudo.chaves.len());
    for chave in &conteudo.chaves {
        chaves.push(if chave.legada {
            recifrar_chave(&chave.bolsa, senha, &senha_bmp)?
        } else {
            chave.bolsa.clone()
        });
    }
    let info_de_dados = |bolsas: &[&[u8]]| {
        sequencia(&[
            &oid_der(oid::DADOS),
            &elemento(tag::CONTEXTO_0, &octetos_der(&sequencia(bolsas))),
        ])
    };
    let certificados: Vec<&[u8]> = conteudo
        .certificados
        .iter()
        .map(|c| c.bolsa.as_slice())
        .collect();
    let chaves: Vec<&[u8]> = chaves.iter().map(Vec::as_slice).collect();
    let cofre = sequencia(&[&info_de_dados(&certificados), &info_de_dados(&chaves)]);

    let sal: [u8; 16] = rand::random();
    let iteracoes = 2048;
    let chave_mac = kdf_pkcs12::<Sha256>(&senha_bmp, &sal, 3, iteracoes, 32);
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&chave_mac)
        .map_err(|_| Falha::Formato(FormatoInvalido("chave de MAC inválida".into())))?;
    mac.update(&cofre);
    let resumo = mac.finalize().into_bytes();
    let dados_mac = sequencia(&[
        &sequencia(&[
            &sequencia(&[&oid_der(oid::SHA256), &[0x05, 0x00]]),
            &octetos_der(&resumo),
        ]),
        &octetos_der(&sal),
        &inteiro_der(u64::from(iteracoes)),
    ]);
    let autenticado = sequencia(&[
        &oid_der(oid::DADOS),
        &elemento(tag::CONTEXTO_0, &octetos_der(&cofre)),
    ]);
    Ok(sequencia(&[&inteiro_der(3), &autenticado, &dados_mac]))
}

/// Decifra uma `pkcs8ShroudedKeyBag` em RC2 e a cifra de novo em 3DES (PBES1 do PKCS#12),
/// mantendo os atributos (o `localKeyId` que a liga ao certificado).
fn recifrar_chave(bolsa: &[u8], senha: &str, senha_bmp: &[u8]) -> Result<Vec<u8>, Falha> {
    let campos = Tlv::unico(bolsa)?.filhos()?;
    let [_, valor, atributos @ ..] = campos.as_slice() else {
        return formato("SafeBag malformada");
    };
    let info = Tlv::unico(valor.valor)?.filhos()?;
    let [algoritmo, cifrado] = info[..] else {
        return formato("EncryptedPrivateKeyInfo malformado");
    };
    let claro = Zeroizing::new(decifrar(algoritmo, &cifrado.octetos()?, senha, senha_bmp)?);
    let sal: [u8; 8] = rand::random();
    let iteracoes = 2048;
    let chave = Zeroizing::new(kdf_pkcs12::<Sha1>(senha_bmp, &sal, 1, iteracoes, 24));
    let iv = kdf_pkcs12::<Sha1>(senha_bmp, &sal, 2, iteracoes, 8);
    let cifrado = cbc::Encryptor::<des::TdesEde3>::new_from_slices(&chave, &iv)
        .map_err(|_| Falha::Formato(FormatoInvalido("chave 3DES inválida".into())))?
        .encrypt_padded_vec_mut::<Pkcs7>(&claro);
    let algoritmo = sequencia(&[
        &oid_der(oid::PBE_SHA1_3DES),
        &sequencia(&[&octetos_der(&sal), &inteiro_der(u64::from(iteracoes))]),
    ]);
    let info = sequencia(&[&algoritmo, &octetos_der(&cifrado)]);
    let mut partes = vec![
        oid_der(oid::BOLSA_CHAVE_CIFRADA),
        elemento(tag::CONTEXTO_0, &info),
    ];
    partes.extend(atributos.iter().map(|a| a.bruto.to_vec()));
    let partes: Vec<&[u8]> = partes.iter().map(Vec::as_slice).collect();
    Ok(sequencia(&partes))
}

/// O conteúdo de um `ContentInfo` do tipo `data`.
fn conteudo_de_dados(info: Tlv<'_>) -> Res<Vec<u8>> {
    let filhos = info.exigir(tag::SEQUENCIA, "ContentInfo")?.filhos()?;
    let [tipo, explicito] = filhos[..] else {
        return invalido("ContentInfo malformado");
    };
    if tipo.oid()? != oid::DADOS {
        return invalido("PFX assinado por chave pública não é suportado");
    }
    Ok(
        Tlv::unico(explicito.exigir(tag::CONTEXTO_0, "ContentInfo")?.valor)?
            .octetos()?
            .into_owned(),
    )
}

/// A senha como o PKCS#12 a usa no PBES1 e no MAC: UTF-16 big-endian com dois zeros no fim.
fn senha_bmp(senha: &str) -> Vec<u8> {
    let mut bmp: Vec<u8> = senha.encode_utf16().flat_map(u16::to_be_bytes).collect();
    bmp.extend_from_slice(&[0, 0]);
    bmp
}

fn mac_confere(mac_data: Tlv<'_>, conteudo: &[u8], senha_bmp: &[u8]) -> Result<bool, Falha> {
    let campos = mac_data.exigir(tag::SEQUENCIA, "MacData")?.filhos()?;
    let [resumo, sal, resto @ ..] = campos.as_slice() else {
        return formato("MacData malformado");
    };
    let iteracoes = match resto.first() {
        Some(i) => iteracoes(*i)?,
        None => 1,
    };
    let [algoritmo, esperado] = resumo.filhos()?[..] else {
        return formato("DigestInfo malformado");
    };
    let algoritmo = algoritmo.filhos()?;
    let Some(alg) = algoritmo.first() else {
        return formato("DigestInfo malformado");
    };
    let sal = sal.octetos()?;
    let esperado = esperado.octetos()?;
    macro_rules! conferir {
        ($h:ty) => {{
            let chave =
                kdf_pkcs12::<$h>(senha_bmp, &sal, 3, iteracoes, <$h as Digest>::output_size());
            let mut m = <Hmac<$h> as Mac>::new_from_slice(&chave)
                .map_err(|_| Falha::Formato(FormatoInvalido("chave de MAC inválida".into())))?;
            m.update(conteudo);
            m.verify_slice(&esperado).is_ok()
        }};
    }
    Ok(match alg.oid()?.as_str() {
        oid::SHA1 => conferir!(Sha1),
        oid::SHA256 => conferir!(Sha256),
        oid::SHA384 => conferir!(Sha384),
        oid::SHA512 => conferir!(Sha512),
        outro => return formato(format!("algoritmo de MAC não suportado: {outro}")),
    })
}

fn iteracoes(t: Tlv<'_>) -> Result<u32, Falha> {
    let n = t.inteiro()?;
    if n == 0 || n > ITERACOES_MAXIMAS {
        return formato("contagem de iterações fora da faixa");
    }
    Ok(u32::try_from(n).unwrap_or(u32::MAX))
}

/// A derivação de chave do PKCS#12 (RFC 7292, apêndice B.2) — os nomes curtos são os da RFC.
#[allow(clippy::many_single_char_names)]
fn kdf_pkcs12<D: Digest + BlockSizeUser>(
    senha_bmp: &[u8],
    sal: &[u8],
    id: u8,
    iteracoes: u32,
    tamanho: usize,
) -> Vec<u8> {
    let v = <D as BlockSizeUser>::block_size();
    let estender = |x: &[u8]| -> Vec<u8> {
        x.iter()
            .copied()
            .cycle()
            .take(v * x.len().div_ceil(v))
            .collect()
    };
    let d = vec![id; v];
    let mut i = estender(sal);
    i.extend(estender(senha_bmp));
    let mut saida = Vec::with_capacity(tamanho);
    loop {
        let mut a = D::new()
            .chain_update(&d)
            .chain_update(&i)
            .finalize()
            .to_vec();
        for _ in 1..iteracoes {
            a = D::digest(&a).to_vec();
        }
        let falta = tamanho - saida.len();
        saida.extend_from_slice(&a[..falta.min(a.len())]);
        if saida.len() >= tamanho {
            return saida;
        }
        let b: Vec<u8> = a.iter().copied().cycle().take(v).collect();
        for bloco in i.chunks_mut(v) {
            let mut vai_um = 1u16;
            for k in (0..v).rev() {
                let soma = u16::from(bloco[k]) + u16::from(b[k]) + vai_um;
                bloco[k] = soma.to_be_bytes()[1];
                vai_um = soma >> 8;
            }
        }
    }
}

/// Decifra um `EncryptedData` (PKCS#7) — devolve o `SafeContents` e se a cifra era legada.
fn decifrar_dados(
    explicito: Tlv<'_>,
    senha: &str,
    senha_bmp: &[u8],
) -> Result<(Vec<u8>, bool), Falha> {
    let dados = Tlv::unico(explicito.valor)?.exigir(tag::SEQUENCIA, "EncryptedData")?;
    let campos = dados.filhos()?;
    let Some(info) = campos.get(1) else {
        return formato("EncryptedData malformado");
    };
    let campos = info
        .exigir(tag::SEQUENCIA, "EncryptedContentInfo")?
        .filhos()?;
    let [_, algoritmo, cifrado] = campos[..] else {
        return formato("EncryptedContentInfo sem conteúdo");
    };
    let legado = algoritmo
        .filhos()?
        .first()
        .map(|t| t.oid())
        .transpose()?
        .is_some_and(|o| cifra_legada(&o));
    Ok((
        decifrar(algoritmo, &cifrado.octetos()?, senha, senha_bmp)?,
        legado,
    ))
}

fn decifrar(
    algoritmo: Tlv<'_>,
    cifrado: &[u8],
    senha: &str,
    senha_bmp: &[u8],
) -> Result<Vec<u8>, Falha> {
    let campos = algoritmo
        .exigir(tag::SEQUENCIA, "AlgorithmIdentifier")?
        .filhos()?;
    let [alg, parametros] = campos[..] else {
        return formato("AlgorithmIdentifier sem parâmetros");
    };
    let alg = alg.oid()?;
    if alg == oid::PBES2 {
        return decifrar_pbes2(parametros, cifrado, senha);
    }
    let [sal, iter] = parametros.filhos()?[..] else {
        return formato("parâmetros de PBE malformados");
    };
    let (sal, iter) = (sal.octetos()?, iteracoes(iter)?);
    let chave = |n| kdf_pkcs12::<Sha1>(senha_bmp, &sal, 1, iter, n);
    let iv = kdf_pkcs12::<Sha1>(senha_bmp, &sal, 2, iter, 8);
    let claro = match alg.as_str() {
        oid::PBE_SHA1_3DES => cbc::Decryptor::<des::TdesEde3>::new_from_slices(&chave(24), &iv)
            .ok()
            .and_then(|d| d.decrypt_padded_vec_mut::<Pkcs7>(cifrado).ok()),
        oid::PBE_SHA1_RC2_40 | oid::PBE_SHA1_RC2_128 => {
            let bits = if alg == oid::PBE_SHA1_RC2_40 { 40 } else { 128 };
            let rc2 = rc2::Rc2::new_with_eff_key_len(&chave(bits / 8), bits);
            Some(cbc::Decryptor::<rc2::Rc2>::inner_iv_slice_init(rc2, &iv))
                .and_then(Result::ok)
                .and_then(|d| d.decrypt_padded_vec_mut::<Pkcs7>(cifrado).ok())
        }
        outro => return formato(format!("cifra não suportada: {outro}")),
    };
    claro.map_or_else(|| formato("não foi possível decifrar"), Ok)
}

fn decifrar_pbes2(parametros: Tlv<'_>, cifrado: &[u8], senha: &str) -> Result<Vec<u8>, Falha> {
    let [kdf, esquema] = parametros.filhos()?[..] else {
        return formato("parâmetros de PBES2 malformados");
    };
    let [kdf_oid, kdf_par] = kdf.filhos()?[..] else {
        return formato("PBES2 sem derivação de chave");
    };
    if kdf_oid.oid()? != oid::PBKDF2 {
        return formato("PBES2 com derivação de chave não suportada");
    }
    let kdf_par = kdf_par.filhos()?;
    let [sal, iter, opcionais @ ..] = kdf_par.as_slice() else {
        return formato("parâmetros de PBKDF2 malformados");
    };
    let (sal, iter) = (sal.octetos()?, iteracoes(*iter)?);
    let mut prf = oid::HMAC_SHA1.to_string();
    for o in opcionais {
        if o.tag == tag::SEQUENCIA {
            prf = o.filhos()?.first().map_or(Ok(prf.clone()), |t| t.oid())?;
        }
    }
    let [cifra, iv] = esquema.filhos()?[..] else {
        return formato("PBES2 sem esquema de cifra");
    };
    let cifra = cifra.oid()?;
    let iv = iv.octetos()?;
    let tamanho = match cifra.as_str() {
        oid::AES128_CBC => 16,
        oid::AES192_CBC | oid::DES_EDE3_CBC => 24,
        oid::AES256_CBC => 32,
        outra => return formato(format!("cifra não suportada: {outra}")),
    };
    let mut chave = vec![0u8; tamanho];
    let senha = senha.as_bytes();
    match prf.as_str() {
        oid::HMAC_SHA1 => pbkdf2::pbkdf2_hmac::<Sha1>(senha, &sal, iter, &mut chave),
        oid::HMAC_SHA256 => pbkdf2::pbkdf2_hmac::<Sha256>(senha, &sal, iter, &mut chave),
        oid::HMAC_SHA384 => pbkdf2::pbkdf2_hmac::<Sha384>(senha, &sal, iter, &mut chave),
        oid::HMAC_SHA512 => pbkdf2::pbkdf2_hmac::<Sha512>(senha, &sal, iter, &mut chave),
        outro => return formato(format!("PRF não suportada: {outro}")),
    }
    macro_rules! cbc {
        ($c:ty) => {
            cbc::Decryptor::<$c>::new_from_slices(&chave, &iv)
                .ok()
                .and_then(|d| d.decrypt_padded_vec_mut::<Pkcs7>(cifrado).ok())
        };
    }
    let claro = match cifra.as_str() {
        oid::AES128_CBC => cbc!(aes::Aes128),
        oid::AES192_CBC => cbc!(aes::Aes192),
        oid::AES256_CBC => cbc!(aes::Aes256),
        _ => cbc!(des::TdesEde3),
    };
    claro.map_or_else(|| formato("não foi possível decifrar"), Ok)
}

/// Percorre um `SafeContents`, juntando certificados e os `localKeyId` das chaves.
fn ler_bolsas(bolsas: &[u8], conteudo: &mut Conteudo) -> Result<(), Falha> {
    for bolsa in Tlv::unico(bolsas)?
        .exigir(tag::SEQUENCIA, "SafeContents")?
        .filhos()?
    {
        let campos = bolsa.filhos()?;
        let [tipo, valor, resto @ ..] = campos.as_slice() else {
            return formato("SafeBag malformada");
        };
        let id_local = match resto.first() {
            Some(atributos) => id_local(*atributos)?,
            None => None,
        };
        match tipo.oid()?.as_str() {
            oid::BOLSA_CERTIFICADO => {
                let cert_bag = Tlv::unico(valor.valor)?.filhos()?;
                let [tipo_cert, explicito] = cert_bag[..] else {
                    return formato("CertBag malformada");
                };
                if tipo_cert.oid()? == oid::CERTIFICADO_X509 {
                    let der = Tlv::unico(explicito.valor)?.octetos()?.into_owned();
                    conteudo.certificados.push(CertificadoNaBolsa {
                        der,
                        id_local,
                        bolsa: bolsa.bruto.to_vec(),
                    });
                }
            }
            oid::BOLSA_CHAVE => conteudo.chaves.push(ChaveNaBolsa {
                id_local,
                bolsa: bolsa.bruto.to_vec(),
                legada: false,
            }),
            oid::BOLSA_CHAVE_CIFRADA => {
                let legada = Tlv::unico(valor.valor)?
                    .filhos()?
                    .first()
                    .and_then(|alg| alg.filhos().ok())
                    .and_then(|f| f.first().map(|t| t.oid()))
                    .transpose()?
                    .is_some_and(|o| cifra_legada(&o));
                conteudo.chaves.push(ChaveNaBolsa {
                    id_local,
                    bolsa: bolsa.bruto.to_vec(),
                    legada,
                });
            }
            oid::BOLSA_ANINHADA => ler_bolsas(valor.valor, conteudo)?,
            _ => {}
        }
    }
    Ok(())
}

fn id_local(atributos: Tlv<'_>) -> Res<Option<Vec<u8>>> {
    for atributo in atributos.filhos()? {
        let campos = atributo.filhos()?;
        if let [tipo, valores] = campos[..] {
            if tipo.oid()? == oid::ID_LOCAL_DA_CHAVE {
                return valores
                    .filhos()?
                    .first()
                    .map(|v| v.octetos().map(Cow::into_owned))
                    .transpose();
            }
        }
    }
    Ok(None)
}

/// O certificado do titular: o que tem o mesmo `localKeyId` da chave; sem isso, o que tem
/// CNPJ; sem isso, o primeiro que não é de AC.
fn escolher_folha(conteudo: &Conteudo) -> Result<Vec<u8>, Falha> {
    let certs = &conteudo.certificados;
    if let Some(c) = certs.iter().find(|c| {
        c.id_local.as_ref().is_some_and(|id| {
            conteudo
                .chaves
                .iter()
                .any(|k| k.id_local.as_ref() == Some(id))
        })
    }) {
        return Ok(c.der.clone());
    }
    let lidos: Vec<(&CertificadoNaBolsa, Option<X509>)> = certs
        .iter()
        .map(|c| (c, ler_x509_bruto(&c.der).ok()))
        .collect();
    lidos
        .iter()
        .find(|(_, x)| x.as_ref().is_some_and(|x| x.info.cnpj.is_some()))
        .or_else(|| {
            lidos
                .iter()
                .find(|(_, x)| x.as_ref().is_some_and(|x| !x.ac))
        })
        .or_else(|| lidos.first())
        .map(|(c, _)| c.der.clone())
        .map_or_else(|| formato("o arquivo não contém nenhum certificado"), Ok)
}

struct X509 {
    info: InfoCertificado,
    ac: bool,
}

fn ler_x509(der: &[u8]) -> Res<InfoCertificado> {
    ler_x509_bruto(der).map(|x| x.info)
}

fn ler_x509_bruto(der: &[u8]) -> Res<X509> {
    let cert = Tlv::unico(der)?
        .exigir(tag::SEQUENCIA, "Certificate")?
        .filhos()?;
    let Some(tbs) = cert.first() else {
        return invalido("certificado vazio");
    };
    let mut campos = tbs
        .exigir(tag::SEQUENCIA, "TBSCertificate")?
        .filhos()?
        .into_iter();
    let mut proximo = campos.next();
    if proximo.is_some_and(|t| t.tag == tag::CONTEXTO_0) {
        proximo = campos.next(); // a versão
    }
    let serie = proximo.map_or_else(
        || invalido("certificado sem número de série"),
        |t| t.exigir(tag::INTEIRO, "série"),
    )?;
    let _assinatura = campos.next();
    let (Some(emissor), Some(validade), Some(titular)) =
        (campos.next(), campos.next(), campos.next())
    else {
        return invalido("certificado incompleto");
    };
    let [de, ate] = validade.filhos()?[..] else {
        return invalido("validade malformada");
    };
    let mut cnpj_extensao = None;
    let mut ac = false;
    for extra in campos.filter(|t| t.tag == tag::CONTEXTO_3) {
        for extensao in Tlv::unico(extra.valor)?.filhos()? {
            let partes = extensao.filhos()?;
            let (Some(tipo), Some(valor)) = (partes.first(), partes.last()) else {
                continue;
            };
            let valor = valor.octetos()?;
            match tipo.oid()?.as_str() {
                oid::NOMES_ALTERNATIVOS => cnpj_extensao = cnpj_dos_nomes_alternativos(&valor)?,
                oid::RESTRICOES_BASICAS => {
                    ac = Tlv::unico(&valor)?.filhos()?.first().is_some_and(|b| {
                        b.tag == tag::BOOLEANO && b.valor.first().is_some_and(|&v| v != 0)
                    });
                }
                _ => {}
            }
        }
    }
    let cn = nome_comum(titular)?.unwrap_or_default();
    let (nome, cnpj_do_cn) = separar_documento(&cn);
    let numero_serie =
        sem_zeros_a_esquerda(serie.valor)
            .iter()
            .fold(String::new(), |mut hex, b| {
                let _ = write!(hex, "{b:02X}");
                hex
            });
    Ok(X509 {
        info: InfoCertificado {
            titular: nome,
            cnpj: cnpj_extensao.or(cnpj_do_cn),
            emissor: nome_comum(emissor)?.unwrap_or_default(),
            numero_serie,
            valido_de: instante(de)?,
            valido_ate: instante(ate)?,
        },
        ac,
    })
}

/// O `CN` de um `Name`.
fn nome_comum(nome: Tlv<'_>) -> Res<Option<String>> {
    for rdn in nome.filhos()? {
        for atributo in rdn.filhos()? {
            if let [tipo, valor] = atributo.filhos()?[..] {
                if tipo.oid()? == oid::NOME_COMUM {
                    return valor.texto().map(Some);
                }
            }
        }
    }
    Ok(None)
}

/// "RAZAO SOCIAL:11222333000181" → ("RAZAO SOCIAL", CNPJ). Um `:CPF` (e-CPF) é tirado do
/// nome mas não vira CNPJ.
fn separar_documento(cn: &str) -> (String, Option<Cnpj>) {
    match cn.rsplit_once(':') {
        Some((nome, doc))
            if (doc.len() == 11 || doc.len() == 14) && doc.bytes().all(|b| b.is_ascii_digit()) =>
        {
            (nome.trim().to_string(), Cnpj::novo(doc).ok())
        }
        _ => (cn.trim().to_string(), None),
    }
}

/// O CNPJ da extensão ICP-Brasil `otherName 2.16.76.1.3.3` do `subjectAltName`.
fn cnpj_dos_nomes_alternativos(valor: &[u8]) -> Res<Option<Cnpj>> {
    for nome in Tlv::unico(valor)?.filhos()? {
        if nome.tag != tag::CONTEXTO_0 {
            continue; // e-mail, DNS...
        }
        let partes = nome.filhos()?;
        let [tipo, explicito] = partes[..] else {
            continue;
        };
        if tipo.oid()? != oid::ICP_BRASIL_CNPJ {
            continue;
        }
        let texto = Tlv::unico(explicito.valor)?.texto()?;
        let digitos: String = texto.chars().filter(char::is_ascii_digit).collect();
        return Ok(Cnpj::novo(&digitos).ok());
    }
    Ok(None)
}

/// `UTCTime` (`AAMMDDhhmmssZ`) ou `GeneralizedTime` (`AAAAMMDDhhmmssZ`).
fn instante(t: Tlv<'_>) -> Res<Instante> {
    let texto =
        std::str::from_utf8(t.valor).map_err(|_| FormatoInvalido("data inválida".into()))?;
    let numeros = texto.strip_suffix('Z').unwrap_or(texto);
    let (ano, resto) = match t.tag {
        tag::UTC if numeros.len() >= 10 => {
            let aa: i32 = numeros[..2]
                .parse()
                .map_err(|_| FormatoInvalido("data inválida".into()))?;
            (if aa >= 50 { 1900 + aa } else { 2000 + aa }, &numeros[2..])
        }
        tag::GENERALIZADO if numeros.len() >= 12 => (
            numeros[..4]
                .parse()
                .map_err(|_| FormatoInvalido("data inválida".into()))?,
            &numeros[4..],
        ),
        _ => return invalido("data de validade em formato desconhecido"),
    };
    let campo = |i: usize| -> Res<u32> {
        resto
            .get(i..i + 2)
            .map_or(Ok(0), str::parse)
            .map_err(|_| FormatoInvalido("data inválida".into()))
    };
    let data = Data::de_ymd(ano, campo(0)?, campo(2)?)
        .map_err(|_| FormatoInvalido("data inválida".into()))?;
    let hora = Hora::de_hms(campo(4)?, campo(6)?, campo(8)?)
        .map_err(|_| FormatoInvalido("hora inválida".into()))?;
    Ok(Instante::de_data_hora(data, hora, Fuso::UTC))
}

#[cfg(test)]
mod testes {
    use super::*;

    const SENHA: &str = "cardeal-teste";

    fn dado(nome: &str) -> Vec<u8> {
        let caminho = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/dados")
            .join(nome);
        std::fs::read(&caminho).unwrap_or_else(|e| panic!("{}: {e}", caminho.display()))
    }

    fn instante_utc(a: i32, m: u32, d: u32, h: u32) -> Instante {
        Instante::de_data_hora(
            Data::de_ymd(a, m, d).unwrap(),
            Hora::de_hms(h, 0, 0).unwrap(),
            Fuso::UTC,
        )
    }

    #[test]
    fn le_os_formatos_que_os_exportadores_produzem() {
        for arquivo in ["aes256.pfx", "aes128.pfx", "3des.pfx", "legado.pfx"] {
            let info = ler_certificado_a1(&dado(arquivo), SENHA)
                .unwrap_or_else(|e| panic!("{arquivo}: {e}"));
            assert_eq!(info.titular, "EMPRESA TESTE LTDA", "{arquivo}");
            assert_eq!(
                info.cnpj,
                Some(Cnpj::novo("11222333000181").unwrap()),
                "{arquivo}"
            );
            assert_eq!(info.emissor, "AC TESTE CARDEAL", "{arquivo}");
            assert_eq!(info.valido_de, instante_utc(2025, 3, 1, 12), "{arquivo}");
            assert_eq!(info.valido_ate, instante_utc(2035, 3, 1, 12), "{arquivo}");
            assert_eq!(
                info.numero_serie.len(),
                16,
                "{arquivo}: {}",
                info.numero_serie
            );
        }
    }

    #[test]
    fn a_ac_na_frente_nao_engana_a_escolha_do_titular() {
        // aes256.pfx traz a AC junto; o titular é o da chave.
        let info = ler_certificado_a1(&dado("aes256.pfx"), SENHA).unwrap();
        assert_ne!(info.titular, "AC TESTE CARDEAL");
    }

    #[test]
    fn senha_errada_e_arquivo_que_nao_e_certificado() {
        for arquivo in ["aes256.pfx", "legado.pfx", "3des.pfx"] {
            assert_eq!(
                ler_certificado_a1(&dado(arquivo), "outra-senha"),
                Err(ErroFiscal::SenhaDoCertificadoIncorreta),
                "{arquivo}"
            );
        }
        assert!(matches!(
            ler_certificado_a1(b"isto nao e um pfx", SENHA),
            Err(ErroFiscal::CertificadoInvalido(_))
        ));
        let mut cortado = dado("aes256.pfx");
        cortado.truncate(cortado.len() / 2);
        assert!(matches!(
            ler_certificado_a1(&cortado, SENHA),
            Err(ErroFiscal::CertificadoInvalido(_))
        ));
    }

    #[test]
    fn vencido_cnpj_pelo_cn_e_certificado_de_pessoa() {
        let vencido = ler_certificado_a1(&dado("vencido.pfx"), SENHA).unwrap();
        assert_eq!(vencido.titular, "EMPRESA VENCIDA LTDA");
        assert_eq!(vencido.cnpj, Some(Cnpj::novo("11444777000161").unwrap()));
        assert!(vencido.vencido(instante_utc(2026, 1, 1, 0)));
        let hoje = Data::de_ymd(2020, 12, 22).unwrap();
        assert_eq!(vencido.dias_para_vencer(hoje, Fuso::UTC), 10);

        let pessoa = ler_certificado_a1(&dado("pessoa.pfx"), SENHA).unwrap();
        assert_eq!(pessoa.titular, "FULANO DE TAL");
        assert_eq!(pessoa.cnpj, None);
    }

    #[test]
    fn preparar_regrava_so_o_que_usa_rc2() {
        // aes256, aes128 e 3des já servem ao OpenSSL 3: voltam intocados.
        for arquivo in ["aes256.pfx", "3des.pfx", "aes128.pfx"] {
            let original = dado(arquivo);
            let p = preparar_certificado_a1(&original, SENHA).unwrap();
            assert!(!p.convertido, "{arquivo}");
            assert_eq!(p.pfx, original, "{arquivo}");
        }
        // legado.pfx: certificado em RC2-40 — regravado; a regravação abre com a mesma senha,
        // com o mesmo titular, e já não tem nada legado.
        let original = dado("legado.pfx");
        let p = preparar_certificado_a1(&original, SENHA).unwrap();
        assert!(p.convertido);
        assert_ne!(p.pfx, original);
        assert_eq!(p.info, ler_certificado_a1(&original, SENHA).unwrap());
        assert_eq!(ler_certificado_a1(&p.pfx, SENHA).unwrap(), p.info);
        let reaberto = abrir(&p.pfx, SENHA).ok().unwrap();
        assert!(!reaberto.legado);
        assert_eq!(reaberto.chaves.len(), 1);
        assert_eq!(
            ler_certificado_a1(&p.pfx, "outra"),
            Err(ErroFiscal::SenhaDoCertificadoIncorreta)
        );
        // Preparar de novo não muda nada.
        assert!(!preparar_certificado_a1(&p.pfx, SENHA).unwrap().convertido);
    }

    #[test]
    fn a_chave_recifrada_e_a_mesma() {
        let original = abrir(&dado("legado.pfx"), SENHA).ok().unwrap();
        let p = preparar_certificado_a1(&dado("legado.pfx"), SENHA).unwrap();
        let regravado = abrir(&p.pfx, SENHA).ok().unwrap();
        let claro = |bolsa: &[u8]| {
            let campos = Tlv::unico(bolsa).unwrap().filhos().unwrap();
            let info = Tlv::unico(campos[1].valor).unwrap().filhos().unwrap();
            let bmp = senha_bmp(SENHA);
            decifrar(info[0], &info[1].octetos().unwrap(), SENHA, &bmp)
                .ok()
                .unwrap()
        };
        assert_eq!(
            claro(&original.chaves[0].bolsa),
            claro(&regravado.chaves[0].bolsa)
        );
        assert_eq!(original.chaves[0].id_local, regravado.chaves[0].id_local);
    }

    #[test]
    fn kdf_do_pkcs12_bate_com_o_vetor_conhecido() {
        // Vetor do conjunto de testes do PKCS#12 (senha "smeg", sal 0A58CF64530D823F, ID 1,
        // 1 iteração, 24 bytes, SHA-1).
        let chave = kdf_pkcs12::<Sha1>(
            &senha_bmp("smeg"),
            &[0x0A, 0x58, 0xCF, 0x64, 0x53, 0x0D, 0x82, 0x3F],
            1,
            1,
            24,
        );
        let hex = chave.iter().fold(String::new(), |mut hex, b| {
            let _ = write!(hex, "{b:02X}");
            hex
        });
        assert_eq!(hex, "8AAAE6297B6CB04642AB5B077851284EB7128F1A2A7FBCA3");
    }
}
