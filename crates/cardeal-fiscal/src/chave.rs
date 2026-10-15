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

    /// Monta uma chave a partir das partes (layout da NF-e 4.00), calculando o dígito
    /// verificador módulo 11: UF, ano e mês (`AAMM`), CNPJ, modelo, série, número, tipo de
    /// emissão e o código numérico.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn montar(
        uf_ibge: u8,
        aamm: &str,
        cnpj: &str,
        modelo: &str,
        serie: u32,
        numero: u64,
        tipo_emissao: u8,
        codigo_numerico: u32,
    ) -> Self {
        let base = format!(
            "{uf_ibge:02}{aamm}{cnpj}{modelo}{serie:03}{numero:09}{tipo_emissao}{codigo_numerico:08}"
        );
        Self(format!("{base}{}", digito_modulo_11(&base)))
    }

    /// O número do documento (posições 26 a 34).
    #[must_use]
    pub fn numero(&self) -> u64 {
        self.0[25..34].parse().unwrap_or(0)
    }

    /// Verdadeiro se o dígito verificador confere.
    #[must_use]
    pub fn dv_confere(&self) -> bool {
        u32::from(self.0.as_bytes()[43] - b'0') == digito_modulo_11(&self.0[..43])
    }
}

/// O dígito módulo 11 da chave (pesos 2 a 9 da direita para a esquerda; resto 0 ou 1 → 0).
fn digito_modulo_11(base: &str) -> u32 {
    let mut soma = 0;
    let mut peso = 2;
    for b in base.bytes().rev() {
        soma += u32::from(b - b'0') * peso;
        peso = if peso == 9 { 2 } else { peso + 1 };
    }
    match soma % 11 {
        0 | 1 => 0,
        r => 11 - r,
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

    #[test]
    fn monta_com_digito_verificador() {
        // O exemplo do Manual de Integração da NF-e (DV 5).
        let chave = ChaveAcesso::nova("52060433009911002506550120000007800267301615").unwrap();
        assert!(chave.dv_confere());
        assert_eq!(chave.numero(), 780);
        let montada = ChaveAcesso::montar(43, "2610", "11222333000181", "65", 1, 42, 1, 12_345_678);
        assert_eq!(montada.como_str().len(), 44);
        assert!(montada.dv_confere());
        assert_eq!(montada.numero(), 42);
    }
}
