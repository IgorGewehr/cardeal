//! A chave de acesso de um documento fiscal (NF-e/NFC-e): 44 dígitos.
//!
//! Esta versão valida só o comprimento — o dígito verificador módulo 11 da chave de acesso
//! (posição 44, calculado sobre os 43 primeiros dígitos) fica para quando `mod-fiscal`
//! precisar validar chave digitada manualmente pelo usuário; a que chega pela distribuição
//! `DFe` já vem correta, gerada pela própria SEFAZ.

use cardeal_kernel::texto;
use serde::{Deserialize, Serialize};

use crate::erros::ErroFiscal;

/// A chave de acesso de 44 dígitos de um documento fiscal.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ChaveAcesso(String);

impl ChaveAcesso {
    /// Valida e normaliza (remove máscara, se houver) uma chave de acesso.
    ///
    /// # Errors
    /// [`ErroFiscal::ChaveDeAcessoInvalida`] se não sobrarem exatamente 44 dígitos.
    pub fn nova(bruta: &str) -> Result<Self, ErroFiscal> {
        let digitos = texto::somente_digitos(bruta);
        if digitos.len() != 44 {
            return Err(ErroFiscal::ChaveDeAcessoInvalida);
        }
        Ok(Self(digitos))
    }

    /// Os 44 dígitos, sem máscara.
    #[must_use]
    pub fn como_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn valida_comprimento() {
        let chave = "35 2409 12345678000181 55 001 000001234 1 12345678 9";
        assert!(ChaveAcesso::nova(chave).is_ok());
        assert_eq!(
            ChaveAcesso::nova("123").unwrap_err(),
            ErroFiscal::ChaveDeAcessoInvalida
        );
    }
}
