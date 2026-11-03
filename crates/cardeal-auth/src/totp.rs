//! **Verificação em duas etapas** (`docs/08-seguranca-permissoes.md` §2.1): o código de 6
//! dígitos de um aplicativo autenticador (TOTP, RFC 6238 — Google Authenticator, Microsoft
//! Authenticator, Authy…), além da senha.
//!
//! Domínio puro: o segredo, o código de cada janela de 30 segundos, a conferência (com uma
//! janela de tolerância para o relógio do celular e **sem reaproveitar** um código já usado) e
//! os **códigos de recuperação** (para quem perdeu o celular — cada um vale uma vez).
//!
//! Quem guarda é quem chama: o estado inteiro ([`SegundoFator`]) vai **cifrado no cofre**
//! (`cardeal_storage::Cofre`), nunca em texto na base — quem copia o `.db` não leva o segredo nem
//! os resumos dos códigos de recuperação.

use cardeal_kernel::Instante;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha1::Sha1;

/// A janela de cada código, em segundos.
pub const PASSO_SEGUNDOS: i64 = 30;
/// Quantos dígitos tem o código.
pub const DIGITOS: usize = 6;
/// Quantos códigos de recuperação se geram.
pub const QUANTOS_CODIGOS_DE_RECUPERACAO: usize = 8;

/// O alfabeto dos códigos de recuperação: sem 0/o, 1/l/i, que se confundem no papel.
const ALFABETO_RECUPERACAO: &[u8] = b"23456789abcdefghjkmnpqrstuvwxyz";
/// O alfabeto do Base32 (RFC 4648), o que os aplicativos autenticadores leem.
const BASE32: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// O segredo novo, para o aplicativo: o QR Code leva o `uri`; quem não lê digita o `segredo`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegundoFatorProposto {
    /// O segredo em Base32 (volta na confirmação).
    pub segredo: String,
    /// O endereço `otpauth://` do QR Code.
    pub uri: String,
}

/// O segredo compartilhado com o aplicativo autenticador (160 bits, como o RFC recomenda).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegredoTotp([u8; 20]);

impl std::fmt::Debug for SegredoTotp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SegredoTotp([oculto])")
    }
}

/// O passo (a janela de 30 s) de um instante.
#[must_use]
pub fn passo_de(agora: Instante) -> i64 {
    agora.em_micros().div_euclid(PASSO_SEGUNDOS * 1_000_000)
}

/// Só os dígitos (o código chega como "123 456", "123-456"…).
fn digitos(texto: &str) -> String {
    texto.chars().filter(char::is_ascii_digit).collect()
}

/// Comparação sem atalho: o tempo não revela quantos caracteres batem.
fn iguais(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

impl SegredoTotp {
    /// Um segredo novo, aleatório.
    #[must_use]
    pub fn gerar() -> Self {
        use rand::RngCore;
        let mut b = [0u8; 20];
        rand::rngs::OsRng.fill_bytes(&mut b);
        Self(b)
    }

    /// O segredo a partir dos bytes.
    #[must_use]
    pub const fn de_bytes(b: [u8; 20]) -> Self {
        Self(b)
    }

    /// O segredo em Base32 (sem `=`), em grupos de 4 — o que se digita no aplicativo quando
    /// não dá para ler o QR Code.
    #[must_use]
    pub fn em_base32(&self) -> String {
        let mut saida = String::new();
        let mut buffer: u32 = 0;
        let mut bits = 0;
        for byte in self.0 {
            buffer = (buffer << 8) | u32::from(byte);
            bits += 8;
            while bits >= 5 {
                bits -= 5;
                saida.push(char::from(BASE32[((buffer >> bits) & 31) as usize]));
            }
        }
        if bits > 0 {
            saida.push(char::from(BASE32[((buffer << (5 - bits)) & 31) as usize]));
        }
        saida
    }

    /// O segredo a partir do Base32 (maiúsculas ou minúsculas, com ou sem espaços).
    #[must_use]
    pub fn de_base32(texto: &str) -> Option<Self> {
        let mut bytes = Vec::with_capacity(20);
        let mut buffer: u32 = 0;
        let mut bits = 0;
        for c in texto.chars().filter(|c| !c.is_whitespace() && *c != '=') {
            let valor = BASE32
                .iter()
                .position(|b| char::from(*b) == c.to_ascii_uppercase())?;
            buffer = (buffer << 5) | u32::try_from(valor).ok()?;
            bits += 5;
            if bits >= 8 {
                bits -= 8;
                bytes.push(u8::try_from((buffer >> bits) & 0xFF).ok()?);
            }
        }
        <[u8; 20]>::try_from(bytes).ok().map(Self)
    }

    /// O endereço `otpauth://` que o QR Code leva ao aplicativo.
    #[must_use]
    pub fn uri(&self, emissor: &str, conta: &str) -> String {
        let e = codificar(emissor);
        format!(
            "otpauth://totp/{e}:{}?secret={}&issuer={e}&algorithm=SHA1&digits={DIGITOS}&period={PASSO_SEGUNDOS}",
            codificar(conta),
            self.em_base32()
        )
    }

    /// O código de um passo (HOTP, RFC 4226, com o contador = o passo).
    ///
    /// # Panics
    /// Nunca: o HMAC aceita chave de qualquer tamanho.
    #[must_use]
    pub fn codigo_no_passo(&self, passo: i64) -> String {
        let mut mac = Hmac::<Sha1>::new_from_slice(&self.0).expect("HMAC aceita qualquer chave");
        mac.update(&passo.to_be_bytes());
        let h = mac.finalize().into_bytes();
        let deslocamento = usize::from(h[19] & 0x0f);
        let valor = (u32::from(h[deslocamento] & 0x7f) << 24)
            | (u32::from(h[deslocamento + 1]) << 16)
            | (u32::from(h[deslocamento + 2]) << 8)
            | u32::from(h[deslocamento + 3]);
        format!("{:06}", valor % 1_000_000)
    }

    /// O código de agora.
    #[must_use]
    pub fn codigo_em(&self, agora: Instante) -> String {
        self.codigo_no_passo(passo_de(agora))
    }

    /// Confere o código digitado: o do passo de agora, o anterior ou o seguinte (o relógio do
    /// celular pode estar meio minuto fora), nunca um passo já usado (`ultimo_passo`). Devolve
    /// o passo que bateu — quem chama o guarda como o último usado.
    #[must_use]
    pub fn conferir(
        &self,
        digitado: &str,
        agora: Instante,
        ultimo_passo: Option<i64>,
    ) -> Option<i64> {
        let d = digitos(digitado);
        if d.len() != DIGITOS {
            return None;
        }
        let p = passo_de(agora);
        (p - 1..=p + 1)
            .filter(|passo| ultimo_passo.is_none_or(|u| *passo > u))
            .find(|passo| iguais(self.codigo_no_passo(*passo).as_bytes(), d.as_bytes()))
    }
}

/// Codificação para o `otpauth://` (letras, dígitos e `-._~` passam; o resto vira `%XX`).
fn codificar(texto: &str) -> String {
    use std::fmt::Write as _;
    texto.bytes().fold(String::new(), |mut s, b| {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            s.push(char::from(b));
        } else {
            let _ = write!(s, "%{b:02X}");
        }
        s
    })
}

/// O resumo de um código de recuperação (sem traço, minúsculo), com o usuário no contexto —
/// o mesmo código de outra pessoa dá outro resumo.
fn resumo_de_recuperacao(dono: &[u8], codigo: &str) -> [u8; 32] {
    let normal: String = codigo
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    let mut h = blake3::Hasher::new_derive_key("cardeal 2fa recuperacao v1");
    h.update(dono);
    h.update(normal.as_bytes());
    *h.finalize().as_bytes()
}

/// O estado da verificação em duas etapas de uma pessoa: o segredo, os resumos dos códigos de
/// recuperação ainda não usados e o último passo aceito. É isto que vai cifrado no cofre.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegundoFator {
    /// O segredo do aplicativo.
    pub segredo: SegredoTotp,
    /// Os resumos dos códigos de recuperação que ainda valem.
    pub recuperacao: Vec<[u8; 32]>,
    /// O último passo aceito (o código daquele passo e dos anteriores não vale mais).
    pub ultimo_passo: Option<i64>,
}

/// Como o segundo fator foi aceito.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aceito {
    /// Pelo código do aplicativo.
    Aplicativo,
    /// Por um código de recuperação (que deixou de valer); restam tantos.
    Recuperacao {
        /// Quantos códigos de recuperação ainda valem.
        restantes: usize,
    },
}

impl SegundoFator {
    /// O estado novo, com os códigos de recuperação em claro — mostrados **uma vez** a quem
    /// ativou. `dono` é o identificador da pessoa (o resumo depende dele).
    #[must_use]
    pub fn novo(segredo: SegredoTotp, dono: &[u8]) -> (Self, Vec<String>) {
        use rand::Rng;
        let mut rng = rand::rngs::OsRng;
        let codigos: Vec<String> = (0..QUANTOS_CODIGOS_DE_RECUPERACAO)
            .map(|_| {
                let c: String = (0..10)
                    .map(|_| {
                        char::from(
                            ALFABETO_RECUPERACAO[rng.gen_range(0..ALFABETO_RECUPERACAO.len())],
                        )
                    })
                    .collect();
                format!("{}-{}", &c[..5], &c[5..])
            })
            .collect();
        let recuperacao = codigos
            .iter()
            .map(|c| resumo_de_recuperacao(dono, c))
            .collect();
        (
            Self {
                segredo,
                recuperacao,
                ultimo_passo: None,
            },
            codigos,
        )
    }

    /// Confere o código digitado: o do aplicativo (6 dígitos) ou um de recuperação (que deixa
    /// de valer). Em sucesso, o estado muda (o último passo, ou o código gasto) e quem chama
    /// precisa gravá-lo de novo.
    pub fn conferir(&mut self, digitado: &str, agora: Instante, dono: &[u8]) -> Option<Aceito> {
        if let Some(passo) = self.segredo.conferir(digitado, agora, self.ultimo_passo) {
            self.ultimo_passo = Some(passo);
            return Some(Aceito::Aplicativo);
        }
        let alfanum = digitado.chars().filter(char::is_ascii_alphanumeric).count();
        if alfanum != 10 {
            return None;
        }
        let r = resumo_de_recuperacao(dono, digitado);
        let pos = self.recuperacao.iter().position(|x| iguais(x, &r))?;
        self.recuperacao.remove(pos);
        Some(Aceito::Recuperacao {
            restantes: self.recuperacao.len(),
        })
    }
}

/// O segundo fator selado pelo cofre, em hexadecimal — o que vai à base (`mfa_segredo` do
/// usuário local, `segundo_fator` da conta do servidor). Amarrado ao `dono`: o selado copiado
/// para outra pessoa não abre.
///
/// # Panics
/// Nunca: o estado (bytes, vetores e um número) sempre serializa.
#[cfg(feature = "sqlite")]
#[must_use]
pub fn selar(estado: &SegundoFator, cofre: &cardeal_storage::Cofre, dono: &[u8]) -> String {
    use std::fmt::Write as _;
    let claro = postcard::to_stdvec(estado).expect("o estado do segundo fator sempre serializa");
    cofre
        .selar(&claro, &contexto(dono))
        .iter()
        .fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// Abre o que [`selar`] produziu.
///
/// # Errors
/// O erro do cofre (outra instalação, corrompido); `FALHA_INTERNA` se o texto não é um selado.
#[cfg(feature = "sqlite")]
pub fn abrir(
    selado: &str,
    cofre: &cardeal_storage::Cofre,
    dono: &[u8],
) -> cardeal_kernel::Resultado<SegundoFator> {
    use cardeal_kernel::{CodigoErro, Erro};
    let corrompido = || {
        Erro::novo(
            CodigoErro::FALHA_INTERNA,
            "a verificação em duas etapas desta conta está corrompida — peça ao administrador              para desligá-la e ative de novo",
        )
    };
    if selado.len() % 2 != 0 {
        return Err(corrompido());
    }
    let bytes: Vec<u8> = (0..selado.len())
        .step_by(2)
        .map(|i| {
            selado
                .get(i..i + 2)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        })
        .collect::<Option<_>>()
        .ok_or_else(corrompido)?;
    let aberto = cofre
        .abrir(&bytes, &contexto(dono))
        .map_err(|e| Erro::de_dominio(&e))?;
    postcard::from_bytes(aberto.expor()).map_err(|_| corrompido())
}

#[cfg(feature = "sqlite")]
fn contexto(dono: &[u8]) -> Vec<u8> {
    [b"cardeal 2fa v1:".as_slice(), dono].concat()
}

#[cfg(test)]
mod testes {
    use super::*;

    /// O segredo dos vetores de teste do RFC 6238 (SHA-1): "12345678901234567890".
    fn rfc() -> SegredoTotp {
        SegredoTotp::de_bytes(*b"12345678901234567890")
    }

    fn em(segundos: i64) -> Instante {
        Instante::de_micros(segundos * 1_000_000)
    }

    #[test]
    fn os_codigos_batem_com_os_vetores_do_rfc_6238() {
        // RFC 6238, apêndice B (8 dígitos): 94287082, 07081804, 14050471, 89005924, 69279037
        // — os 6 últimos são o código de 6 dígitos.
        assert_eq!(rfc().codigo_em(em(59)), "287082");
        assert_eq!(rfc().codigo_em(em(1_111_111_109)), "081804");
        assert_eq!(rfc().codigo_em(em(1_111_111_111)), "050471");
        assert_eq!(rfc().codigo_em(em(1_234_567_890)), "005924");
        assert_eq!(rfc().codigo_em(em(2_000_000_000)), "279037");
    }

    #[test]
    fn base32_vai_e_volta_e_o_uri_leva_o_segredo() {
        let s = rfc();
        assert_eq!(s.em_base32(), "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ");
        assert_eq!(
            SegredoTotp::de_base32("gezd gnbv gy3t qojq gezd gnbv gy3t qojq"),
            Some(s.clone())
        );
        assert_eq!(SegredoTotp::de_base32("GEZ1"), None);
        let novo = SegredoTotp::gerar();
        assert_eq!(SegredoTotp::de_base32(&novo.em_base32()), Some(novo));
        let uri = s.uri("Cardeal", "maria@loja.com");
        assert!(
            uri.starts_with("otpauth://totp/Cardeal:maria%40loja.com?secret=GEZDG"),
            "{uri}"
        );
        assert!(uri.contains("issuer=Cardeal"));
    }

    #[test]
    fn aceita_a_janela_vizinha_mas_nunca_o_mesmo_codigo_duas_vezes() {
        let s = rfc();
        let agora = em(1_000_000_005);
        let codigo = s.codigo_em(agora);
        let p = s.conferir(&codigo, agora, None).expect("o de agora vale");
        assert_eq!(
            s.conferir(&codigo, agora, Some(p)),
            None,
            "não se reaproveita"
        );
        // O do passo anterior ainda vale (o relógio do celular atrasado meio minuto).
        let anterior = s.codigo_em(em(1_000_000_005 - 30));
        assert!(s.conferir(&anterior, agora, None).is_some());
        // Dois passos atrás, não.
        let velho = s.codigo_em(em(1_000_000_005 - 60));
        assert_eq!(s.conferir(&velho, agora, None), None);
        assert_eq!(s.conferir("12345", agora, None), None);
        assert_eq!(
            s.conferir(&format!("{} {}", &codigo[..3], &codigo[3..]), agora, None),
            Some(p)
        );
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn o_selado_so_abre_no_cofre_e_para_o_dono_certo() {
        let cofre = cardeal_storage::Cofre::aleatorio();
        let (sf, _) = SegundoFator::novo(SegredoTotp::gerar(), b"maria");
        let selado = selar(&sf, &cofre, b"maria");
        assert!(!selado.contains(&sf.segredo.em_base32()), "nada em claro");
        assert_eq!(abrir(&selado, &cofre, b"maria").unwrap(), sf);
        assert!(abrir(&selado, &cofre, b"joao").is_err());
        assert!(abrir(&selado, &cardeal_storage::Cofre::aleatorio(), b"maria").is_err());
        assert!(abrir("xyz", &cofre, b"maria").is_err());
    }

    #[test]
    fn o_codigo_de_recuperacao_vale_uma_vez_e_so_para_o_dono() {
        let (mut sf, codigos) = SegundoFator::novo(SegredoTotp::gerar(), b"maria");
        assert_eq!(codigos.len(), QUANTOS_CODIGOS_DE_RECUPERACAO);
        assert!(codigos
            .iter()
            .all(|c| c.len() == 11 && c.as_bytes()[5] == b'-'));
        let agora = em(1_700_000_000);
        let mut outro = sf.clone();
        assert_eq!(
            outro.conferir(&codigos[0], agora, b"joao"),
            None,
            "de outra pessoa não vale"
        );
        assert_eq!(
            sf.conferir(&codigos[0].to_uppercase(), agora, b"maria"),
            Some(Aceito::Recuperacao { restantes: 7 })
        );
        assert_eq!(sf.conferir(&codigos[0], agora, b"maria"), None, "gasto");
        let codigo = sf.segredo.codigo_em(agora);
        assert_eq!(
            sf.conferir(&codigo, agora, b"maria"),
            Some(Aceito::Aplicativo)
        );
        assert_eq!(sf.ultimo_passo, Some(passo_de(agora)));
    }
}
