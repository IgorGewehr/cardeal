//! `Segredo<T>` — um valor sensível que não vaza em `Debug`/`Display` e é zerado ao ser
//! descartado.
//!
//! Usado para a chave de criptografia da base (`docs/07-persistencia-sqlite.md` §8) e para
//! qualquer credencial que passe pela configuração.

use std::fmt;

use zeroize::Zeroize;

/// Encapsula um valor sensível. `Debug` imprime `[oculto]`; o conteúdo é zerado no `Drop`.
#[derive(Clone, PartialEq, Eq)]
pub struct Segredo<T: Zeroize>(T);

impl<T: Zeroize> Segredo<T> {
    /// Encapsula um valor.
    pub const fn novo(valor: T) -> Self {
        Self(valor)
    }

    /// Empresta o valor por dentro. Use no menor escopo possível.
    pub const fn expor(&self) -> &T {
        &self.0
    }
}

impl<T: Zeroize> fmt::Debug for Segredo<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Segredo([oculto])")
    }
}

impl<T: Zeroize> Drop for Segredo<T> {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl<T: Zeroize> From<T> for Segredo<T> {
    fn from(valor: T) -> Self {
        Self(valor)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn debug_nao_vaza_o_conteudo() {
        let s = Segredo::novo(String::from("chave-super-secreta"));
        assert_eq!(format!("{s:?}"), "Segredo([oculto])");
        assert_eq!(s.expor(), "chave-super-secreta");
    }
}
