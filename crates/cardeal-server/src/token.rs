//! O token de sessão: 32 bytes aleatórios do SO, entregues em base64url. O servidor guarda só
//! o BLAKE3 dele — um vazamento do diretório não entrega sessões válidas.

use base64::Engine as _;
use rand::RngCore as _;

/// O hash de um token, como fica no diretório e no cache.
pub(crate) type HashToken = [u8; 32];

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// Um token novo e o hash dele.
pub(crate) fn gerar() -> (String, HashToken) {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let texto = B64.encode(bytes);
    let hash = hash(&texto);
    (texto, hash)
}

/// O hash de um token recebido. Tokens com formato errado também têm hash — simplesmente não
/// existem no diretório, e a resposta é a mesma de um token expirado.
pub(crate) fn hash(token: &str) -> HashToken {
    *blake3::hash(token.as_bytes()).as_bytes()
}

/// Uma senha aleatória inutilizável, para o `nucleo_usuario` de uma empresa provisionada pelo
/// servidor: online quem vale é a senha da conta (ADR-0016, consequências).
pub(crate) fn senha_inutilizavel() -> String {
    gerar().0
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn tokens_sao_unicos_e_o_hash_e_deterministico() {
        let (a, ha) = gerar();
        let (b, hb) = gerar();
        assert_ne!(a, b);
        assert_ne!(ha, hb);
        assert_eq!(hash(&a), ha);
        assert_eq!(a.len(), 43, "32 bytes em base64url sem preenchimento");
    }
}
