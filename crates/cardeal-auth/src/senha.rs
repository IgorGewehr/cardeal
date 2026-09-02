//! Hash e verificação de senha com Argon2id.
//!
//! Ver `docs/08-seguranca-permissoes.md` §2.1: parâmetros `m=19 MiB, t=2, p=1`
//! (recomendação OWASP), calibrados para ~250 ms na máquina real. Nunca logamos, nunca
//! exibimos, nunca comparamos senha em texto puro — a verificação é feita pelo próprio
//! `argon2`, que compara os hashes em tempo constante.

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use std::fmt;

use crate::erros::ErroAuth;

/// Custo de memória em KiB — 19 MiB, o piso recomendado pela OWASP para Argon2id em 2024+.
const CUSTO_MEMORIA_KIB: u32 = 19 * 1024;
/// Número de iterações.
const CUSTO_TEMPO: u32 = 2;
/// Paralelismo.
const CUSTO_PARALELISMO: u32 = 1;

fn motor() -> Argon2<'static> {
    // `Params::new` só falha se os custos forem absurdos (zero, ou memória menor que o
    // paralelismo exige) — os literais acima nunca acionam isso, então o unwrap é seguro.
    // SEGURO: parâmetros constantes e válidos, verificados por teste.
    let params = Params::new(CUSTO_MEMORIA_KIB, CUSTO_TEMPO, CUSTO_PARALELISMO, None)
        .expect("parâmetros de Argon2id constantes são válidos");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

/// O hash de uma senha, no formato PHC padrão (`$argon2id$v=19$m=...,t=...,p=...$salt$hash`).
///
/// `Debug` nunca imprime o hash inteiro — mesmo não sendo a senha em si, um hash vazado
/// facilita ataque offline de força bruta, e `docs/08-seguranca-permissoes.md` §7 pede que
/// segredos nunca apareçam em log.
#[derive(Clone, PartialEq, Eq)]
pub struct HashDeSenha(String);

impl HashDeSenha {
    /// Constrói a partir de uma string PHC já existente (lida do banco).
    #[must_use]
    pub const fn de_phc(phc: String) -> Self {
        Self(phc)
    }

    /// A string PHC, para persistir.
    #[must_use]
    pub fn como_phc(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for HashDeSenha {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("HashDeSenha([oculto])")
    }
}

/// Deriva o hash Argon2id de uma senha em texto puro, com um salt aleatório novo.
///
/// # Errors
/// [`ErroAuth::FalhaDeHash`] só em caso de erro interno do Argon2 — não acontece com os
/// parâmetros fixos deste módulo; existe para não expor `panic!` na fronteira pública.
pub fn hash_senha(senha: &str) -> Result<HashDeSenha, ErroAuth> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = motor()
        .hash_password(senha.as_bytes(), &salt)
        .map_err(|_| ErroAuth::FalhaDeHash)?;
    Ok(HashDeSenha(hash.to_string()))
}

/// Verifica uma senha contra um hash. Nunca vaza *quanto* da senha acertou — o `argon2`
/// compara o resultado final em tempo constante.
#[must_use]
pub fn verificar_senha(senha: &str, hash: &HashDeSenha) -> bool {
    let Ok(analisado) = PasswordHash::new(&hash.0) else {
        return false;
    };
    motor()
        .verify_password(senha.as_bytes(), &analisado)
        .is_ok()
}

/// Uma amostra reduzida das senhas mais comuns em vazamentos públicos, só para dar
/// feedback imediato na interface sem round-trip ao servidor. A lista completa (milhares
/// de entradas) é responsabilidade do adaptador de persistência
/// (`docs/08-seguranca-permissoes.md` §2.1) e roda como segunda verificação no servidor.
const SENHAS_FRACAS_AMOSTRA: &[&str] = &[
    "123456",
    "12345678",
    "123456789",
    "senha",
    "senha123",
    "password",
    "qwerty",
    "111111",
    "000000",
    "admin",
    "admin123",
    "1234",
    "12345",
    "abc123",
    "letmein",
    "iloveyou",
    "654321",
    "123123",
    "welcome",
    "monkey",
];

/// A política de senha da empresa. Ver `docs/08-seguranca-permissoes.md` §2.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoliticaSenha {
    /// O comprimento mínimo aceito, em caracteres.
    pub minimo_caracteres: u8,
}

impl PoliticaSenha {
    /// A política padrão do Cardeal: mínimo de 8 caracteres.
    pub const PADRAO: Self = Self {
        minimo_caracteres: 8,
    };

    /// Valida uma senha contra esta política.
    ///
    /// # Errors
    /// [`ErroAuth::SenhaCurta`] se `senha` tiver menos caracteres que o mínimo,
    /// [`ErroAuth::SenhaComum`] se estiver na amostra de senhas mais vazadas.
    pub fn validar(&self, senha: &str) -> Result<(), ErroAuth> {
        let comprimento = u8::try_from(senha.chars().count()).unwrap_or(u8::MAX);
        if comprimento < self.minimo_caracteres {
            return Err(ErroAuth::SenhaCurta {
                minimo: self.minimo_caracteres,
            });
        }
        let normalizada = senha.to_lowercase();
        if SENHAS_FRACAS_AMOSTRA.contains(&normalizada.as_str()) {
            return Err(ErroAuth::SenhaComum);
        }
        Ok(())
    }
}

impl Default for PoliticaSenha {
    fn default() -> Self {
        Self::PADRAO
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn hash_e_verificacao_completam_o_ciclo() {
        let hash = hash_senha("um segredo bem forte").unwrap();
        assert!(verificar_senha("um segredo bem forte", &hash));
        assert!(!verificar_senha("senha errada", &hash));
    }

    #[test]
    fn hashes_do_mesmo_texto_sao_diferentes_por_causa_do_salt() {
        let a = hash_senha("mesma senha").unwrap();
        let b = hash_senha("mesma senha").unwrap();
        assert_ne!(a.como_phc(), b.como_phc());
        // Mas as duas ainda verificam a senha original.
        assert!(verificar_senha("mesma senha", &a));
        assert!(verificar_senha("mesma senha", &b));
    }

    #[test]
    fn hash_nunca_aparece_em_debug() {
        let hash = hash_senha("segredo").unwrap();
        assert_eq!(format!("{hash:?}"), "HashDeSenha([oculto])");
    }

    #[test]
    fn hash_malformado_nao_verifica_e_nao_gera_panico() {
        let malformado = HashDeSenha::de_phc("isto não é um hash PHC válido".into());
        assert!(!verificar_senha("qualquer coisa", &malformado));
    }

    #[test]
    fn politica_recusa_senha_curta() {
        let erro = PoliticaSenha::PADRAO.validar("abc123").unwrap_err();
        assert!(matches!(erro, ErroAuth::SenhaCurta { minimo: 8 }));
    }

    #[test]
    fn politica_recusa_senha_comum() {
        let erro = PoliticaSenha::PADRAO.validar("Senha123").unwrap_err();
        assert!(matches!(erro, ErroAuth::SenhaComum));
    }

    #[test]
    fn politica_aceita_senha_forte() {
        assert!(PoliticaSenha::PADRAO.validar("Corr3io-Azul-Serra!").is_ok());
    }
}
