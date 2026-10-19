//! `Cofre` — segredos em repouso (o certificado A1 e a senha dele, o token do CSC da NFC-e)
//! cifrados com AES-256-GCM por uma chave que **não** mora na base.
//!
//! A base vai para backup e réplica (`docs/07` §8, `ConfigReplicacao`); a chave do cofre não
//! vai. Quem copia o `.db` leva só texto cifrado. O que o cofre não protege: quem tem o disco
//! inteiro da máquina leva a chave junto. Proteger a chave com o armazenamento do sistema
//! (DPAPI no Windows) é o passo seguinte (`docs/modulos/fiscal.md` §2).
//!
//! Formato do selado: `[versão = 1][impressão da chave, 8 bytes][nonce, 12][cifrado + tag]`.
//! A impressão diz, ao abrir, se o segredo foi guardado com **outra** chave (outra instalação)
//! — o que pede reenviar o arquivo, não investigar corrupção.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::Path;

use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use cardeal_kernel::{CodigoErro, Detalhes, ErroDominio};

use crate::segredo::Segredo;

const VERSAO: u8 = 1;
const TAMANHO_IMPRESSAO: usize = 8;
const TAMANHO_NONCE: usize = 12;
const CABECALHO: usize = 1 + TAMANHO_IMPRESSAO + TAMANHO_NONCE;

/// O que pode dar errado com o cofre.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErroCofre {
    /// A chave informada não tem 64 dígitos hexadecimais (32 bytes).
    #[error("A chave do cofre precisa ter 64 dígitos hexadecimais")]
    ChaveInvalida,

    /// O segredo foi guardado com a chave de outra instalação.
    #[error(
        "Este segredo foi guardado em outra instalação e não pode ser lido aqui — envie de novo"
    )]
    OutraChave,

    /// O selado foi alterado ou está truncado.
    #[error("O segredo guardado está corrompido")]
    Corrompido,

    /// Falha ao ler ou criar o arquivo da chave.
    #[error("Não foi possível ler ou criar a chave do cofre: {0}")]
    Arquivo(String),
}

impl ErroDominio for ErroCofre {
    fn codigo(&self) -> CodigoErro {
        match self {
            Self::ChaveInvalida => CodigoErro::VALOR_INVALIDO,
            Self::OutraChave => CodigoErro::REGRA_VIOLADA,
            Self::Corrompido => CodigoErro::FALHA_INTERNA,
            Self::Arquivo(_) => CodigoErro::FALHA_DE_DISCO,
        }
    }

    fn detalhes(&self) -> Option<Detalhes> {
        None
    }
}

/// O cofre de uma instalação (o desktop monoposto ou o servidor): uma chave de 256 bits.
pub struct Cofre {
    chave: Segredo<[u8; 32]>,
    impressao: [u8; TAMANHO_IMPRESSAO],
}

impl std::fmt::Debug for Cofre {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Cofre([oculto])")
    }
}

impl Cofre {
    /// Um cofre sobre uma chave dada.
    #[must_use]
    pub fn novo(chave: [u8; 32]) -> Self {
        let mut impressao = [0u8; TAMANHO_IMPRESSAO];
        let resumo = blake3::derive_key("cardeal cofre impressao v1", &chave);
        impressao.copy_from_slice(&resumo[..TAMANHO_IMPRESSAO]);
        Self {
            chave: Segredo::novo(chave),
            impressao,
        }
    }

    /// Um cofre com uma chave aleatória — para testes e para a primeira abertura.
    #[must_use]
    pub fn aleatorio() -> Self {
        Self::novo(Aes256Gcm::generate_key(OsRng).into())
    }

    /// Um cofre a partir da chave em hexadecimal (a variável de ambiente do servidor).
    ///
    /// # Errors
    /// [`ErroCofre::ChaveInvalida`] se o texto não tiver 64 dígitos hexadecimais.
    pub fn da_chave_hex(hex: &str) -> Result<Self, ErroCofre> {
        let hex = hex.trim();
        if hex.len() != 64 || !hex.is_ascii() {
            return Err(ErroCofre::ChaveInvalida);
        }
        let mut chave = [0u8; 32];
        for (i, byte) in chave.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16)
                .map_err(|_| ErroCofre::ChaveInvalida)?;
        }
        Ok(Self::novo(chave))
    }

    /// O cofre da chave guardada em `caminho`; na primeira vez, gera a chave e cria o arquivo
    /// (só o dono do processo lê, no Unix).
    ///
    /// # Errors
    /// [`ErroCofre::Arquivo`] se o arquivo não puder ser lido/criado;
    /// [`ErroCofre::ChaveInvalida`] se o conteúdo não for uma chave.
    pub fn do_arquivo(caminho: &Path) -> Result<Self, ErroCofre> {
        match std::fs::read_to_string(caminho) {
            Ok(texto) => return Self::da_chave_hex(&texto),
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(ErroCofre::Arquivo(e.to_string()));
            }
            Err(_) => {}
        }
        let cofre = Self::aleatorio();
        let hex = cofre
            .chave
            .expor()
            .iter()
            .fold(String::new(), |mut hex, b| {
                let _ = write!(hex, "{b:02x}");
                hex
            });
        let mut opcoes = std::fs::OpenOptions::new();
        opcoes.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut opcoes, 0o600);
        match opcoes.open(caminho) {
            Ok(mut arquivo) => {
                arquivo
                    .write_all(format!("{hex}\n").as_bytes())
                    .and_then(|()| arquivo.sync_all())
                    .map_err(|e| ErroCofre::Arquivo(e.to_string()))?;
                Ok(cofre)
            }
            // Outro processo criou ao mesmo tempo: vale a dele.
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let texto = std::fs::read_to_string(caminho)
                    .map_err(|e| ErroCofre::Arquivo(e.to_string()))?;
                Self::da_chave_hex(&texto)
            }
            Err(e) => Err(ErroCofre::Arquivo(e.to_string())),
        }
    }

    /// Cifra `claro`, amarrado a `contexto` (ex.: empresa + finalidade) — um selado copiado
    /// para outro contexto não abre.
    #[must_use]
    pub fn selar(&self, claro: &[u8], contexto: &[u8]) -> Vec<u8> {
        let cifra = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(self.chave.expor()));
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let aad = self.aad(contexto);
        let cifrado = cifra
            .encrypt(
                &nonce,
                Payload {
                    msg: claro,
                    aad: &aad,
                },
            )
            .unwrap_or_else(|_| unreachable!("AES-GCM só falha acima de 64 GiB"));
        let mut selado = Vec::with_capacity(CABECALHO + cifrado.len());
        selado.push(VERSAO);
        selado.extend_from_slice(&self.impressao);
        selado.extend_from_slice(&nonce);
        selado.extend_from_slice(&cifrado);
        selado
    }

    /// Decifra o que [`Self::selar`] produziu com o mesmo `contexto`.
    ///
    /// # Errors
    /// [`ErroCofre::OutraChave`] se foi selado com outra chave; [`ErroCofre::Corrompido`] se
    /// foi alterado, truncado ou selado em outro contexto.
    pub fn abrir(&self, selado: &[u8], contexto: &[u8]) -> Result<Segredo<Vec<u8>>, ErroCofre> {
        if selado.len() < CABECALHO || selado[0] != VERSAO {
            return Err(ErroCofre::Corrompido);
        }
        if selado[1..=TAMANHO_IMPRESSAO] != self.impressao {
            return Err(ErroCofre::OutraChave);
        }
        let nonce = Nonce::from_slice(&selado[1 + TAMANHO_IMPRESSAO..CABECALHO]);
        let cifra = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(self.chave.expor()));
        let aad = self.aad(contexto);
        cifra
            .decrypt(
                nonce,
                Payload {
                    msg: &selado[CABECALHO..],
                    aad: &aad,
                },
            )
            .map(Segredo::novo)
            .map_err(|_| ErroCofre::Corrompido)
    }

    fn aad(&self, contexto: &[u8]) -> Vec<u8> {
        let mut aad = Vec::with_capacity(1 + TAMANHO_IMPRESSAO + contexto.len());
        aad.push(VERSAO);
        aad.extend_from_slice(&self.impressao);
        aad.extend_from_slice(contexto);
        aad
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn sela_e_abre_no_mesmo_contexto() {
        let cofre = Cofre::aleatorio();
        let selado = cofre.selar(b"senha do certificado", b"empresa-1/certificado");
        assert!(!selado.windows(5).any(|w| w == b"senha"));
        let aberto = cofre.abrir(&selado, b"empresa-1/certificado").unwrap();
        assert_eq!(aberto.expor(), b"senha do certificado");
        // Nonce novo a cada selagem.
        assert_ne!(
            selado,
            cofre.selar(b"senha do certificado", b"empresa-1/certificado")
        );
    }

    #[test]
    fn recusa_outro_contexto_outra_chave_e_alteracao() {
        let cofre = Cofre::aleatorio();
        let selado = cofre.selar(b"x", b"empresa-1");
        assert_eq!(
            cofre.abrir(&selado, b"empresa-2").unwrap_err(),
            ErroCofre::Corrompido
        );
        assert_eq!(
            Cofre::aleatorio().abrir(&selado, b"empresa-1").unwrap_err(),
            ErroCofre::OutraChave
        );
        let mut alterado = selado.clone();
        *alterado.last_mut().unwrap() ^= 1;
        assert_eq!(
            cofre.abrir(&alterado, b"empresa-1").unwrap_err(),
            ErroCofre::Corrompido
        );
        assert_eq!(
            cofre.abrir(&selado[..10], b"empresa-1").unwrap_err(),
            ErroCofre::Corrompido
        );
    }

    #[test]
    fn chave_em_hex_e_em_arquivo() {
        let hex = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
        let a = Cofre::da_chave_hex(hex).unwrap();
        let b = Cofre::da_chave_hex(&hex.to_uppercase()).unwrap();
        assert_eq!(a.abrir(&b.selar(b"ok", b""), b"").unwrap().expor(), b"ok");
        assert_eq!(
            Cofre::da_chave_hex("curta").unwrap_err(),
            ErroCofre::ChaveInvalida
        );
        assert_eq!(
            Cofre::da_chave_hex(&"zz".repeat(32)).unwrap_err(),
            ErroCofre::ChaveInvalida
        );

        let dir = tempfile::tempdir().unwrap();
        let caminho = dir.path().join("cofre.chave");
        let primeiro = Cofre::do_arquivo(&caminho).unwrap();
        let selado = primeiro.selar(b"guardado", b"c");
        // Reabrir lê a mesma chave.
        let segundo = Cofre::do_arquivo(&caminho).unwrap();
        assert_eq!(segundo.abrir(&selado, b"c").unwrap().expor(), b"guardado");
        assert_eq!(format!("{segundo:?}"), "Cofre([oculto])");
    }
}
