//! Percentual exato — alíquotas, descontos, juros, comissões.
//!
//! Guardado como `i64` na escala de **um milionésimo de ponto percentual**.
//! `18%` é `18_000_000`. Isso cobre com folga a precisão exigida pela legislação
//! (alíquotas têm no máximo 4 casas) e evita o erro clássico de guardar `0.18` em `f64`.

use std::fmt;
use std::ops::{Add, Neg, Sub};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::erro::{CodigoErro, Erro, Resultado};

/// Unidades internas por ponto percentual.
const ESCALA: i64 = 1_000_000;

/// Um percentual, com precisão de 1e-6 ponto percentual.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Percentual(i64);

impl Percentual {
    /// Zero por cento.
    pub const ZERO: Self = Self(0);
    /// Cem por cento.
    pub const CEM: Self = Self(100 * ESCALA);

    /// Constrói a partir de pontos percentuais inteiros. `Percentual::pontos(18)` é `18%`.
    #[inline]
    pub const fn pontos(pontos: i64) -> Self {
        Self(pontos * ESCALA)
    }

    /// Constrói a partir de milésimos de ponto percentual.
    /// `Percentual::milesimos(18_500)` é `18,5%`.
    #[inline]
    pub const fn milesimos(milesimos: i64) -> Self {
        Self(milesimos * 1_000)
    }

    /// Constrói diretamente das unidades internas (1e-6 ponto percentual).
    #[inline]
    pub const fn unidades(unidades: i64) -> Self {
        Self(unidades)
    }

    /// As unidades internas. Use apenas em conversões dentro do kernel.
    #[inline]
    pub const fn unidades_internas(self) -> i64 {
        self.0
    }

    /// Verdadeiro se for exatamente zero.
    #[inline]
    pub const fn e_zero(self) -> bool {
        self.0 == 0
    }

    /// Verdadeiro se estiver entre 0% e 100%, inclusive.
    #[inline]
    pub const fn e_fracao_valida(self) -> bool {
        self.0 >= 0 && self.0 <= Self::CEM.0
    }

    /// O complemento para 100%. `20%` devolve `80%`. Útil em desconto → fator de preço.
    #[inline]
    pub const fn complemento(self) -> Self {
        Self(Self::CEM.0 - self.0)
    }

    /// Constrói a partir da razão entre duas grandezas inteiras: `parte / todo`.
    /// Devolve [`Percentual::ZERO`] se `todo` for zero.
    #[allow(clippy::cast_possible_truncation)]
    pub fn da_razao(parte: i64, todo: i64) -> Self {
        if todo == 0 {
            return Self::ZERO;
        }
        let bruto = i128::from(parte) * i128::from(Self::CEM.0);
        Self((bruto / i128::from(todo)) as i64)
    }

    /// Formata com o número de casas pedido, no padrão brasileiro: `18,50`.
    pub fn formatar(self, casas: u8) -> String {
        let casas = u32::from(casas.min(6));
        let divisor = 10i64.pow(6 - casas);
        let escalado = self.0 / divisor;
        let negativo = escalado < 0;
        let abs = escalado.unsigned_abs();
        let base = 10u64.pow(casas);
        let inteiros = abs / base;
        let fracao = abs % base;
        let sinal = if negativo { "-" } else { "" };
        if casas == 0 {
            format!("{sinal}{inteiros}")
        } else {
            format!("{sinal}{inteiros},{fracao:0casas$}", casas = casas as usize)
        }
    }

    /// Formata com o símbolo: `18,50%`. Usa 2 casas, omitindo as decimais quando são zero.
    pub fn formatar_com_simbolo(self) -> String {
        if self.0 % ESCALA == 0 {
            format!("{}%", self.formatar(0))
        } else {
            format!("{}%", self.formatar(2))
        }
    }

    /// Lê um percentual no padrão brasileiro. Aceita `18`, `18,5`, `18.5`, `18,5%`.
    ///
    /// # Errors
    /// Devolve [`CodigoErro::VALOR_INVALIDO`] se o texto não representar um número.
    pub fn de_str(texto: &str) -> Resultado<Self> {
        let bruto = texto.trim().trim_end_matches('%').trim();
        let (corpo, negativo) = match bruto.strip_prefix('-') {
            Some(r) => (r.trim(), true),
            None => (bruto.trim_start_matches('+').trim(), false),
        };
        if corpo.is_empty() {
            return Err(Erro::novo(CodigoErro::VALOR_INVALIDO, "Percentual vazio"));
        }

        let normalizado = corpo.replace(',', ".");
        let (inteiros_txt, decimais_txt) = match normalizado.split_once('.') {
            Some((i, d)) => (i, d),
            None => (normalizado.as_str(), ""),
        };

        if !inteiros_txt.chars().all(|c| c.is_ascii_digit())
            || !decimais_txt.chars().all(|c| c.is_ascii_digit())
        {
            return Err(Erro::novo(
                CodigoErro::VALOR_INVALIDO,
                format!("\"{texto}\" não é um percentual"),
            ));
        }

        let inteiros: i64 = if inteiros_txt.is_empty() {
            0
        } else {
            inteiros_txt
                .parse()
                .map_err(|_| Erro::novo(CodigoErro::VALOR_INVALIDO, "Percentual fora da faixa"))?
        };

        let mut decimais = decimais_txt.to_string();
        decimais.truncate(6);
        while decimais.len() < 6 {
            decimais.push('0');
        }
        let fracao: i64 = decimais.parse().unwrap_or(0);

        let total = inteiros
            .checked_mul(ESCALA)
            .and_then(|v| v.checked_add(fracao))
            .ok_or_else(|| Erro::novo(CodigoErro::VALOR_INVALIDO, "Percentual fora da faixa"))?;

        Ok(Self(if negativo { -total } else { total }))
    }
}

impl Add for Percentual {
    type Output = Self;
    #[inline]
    fn add(self, outro: Self) -> Self {
        Self(self.0 + outro.0)
    }
}

impl Sub for Percentual {
    type Output = Self;
    #[inline]
    fn sub(self, outro: Self) -> Self {
        Self(self.0 - outro.0)
    }
}

impl Neg for Percentual {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Self(-self.0)
    }
}

impl fmt::Display for Percentual {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.formatar_com_simbolo())
    }
}

impl fmt::Debug for Percentual {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Percentual({})", self.formatar_com_simbolo())
    }
}

impl FromStr for Percentual {
    type Err = Erro;
    fn from_str(s: &str) -> Resultado<Self> {
        Self::de_str(s)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn construcao_e_formatacao() {
        assert_eq!(Percentual::pontos(18).formatar_com_simbolo(), "18%");
        assert_eq!(
            Percentual::milesimos(18_500).formatar_com_simbolo(),
            "18,50%"
        );
        assert_eq!(Percentual::pontos(18).formatar(2), "18,00");
        assert_eq!(Percentual::pontos(-5).formatar(2), "-5,00");
        assert_eq!(Percentual::CEM.formatar_com_simbolo(), "100%");
    }

    #[test]
    fn leitura() {
        assert_eq!(Percentual::de_str("18").unwrap(), Percentual::pontos(18));
        assert_eq!(
            Percentual::de_str("18,5").unwrap(),
            Percentual::milesimos(18_500)
        );
        assert_eq!(
            Percentual::de_str("18.5%").unwrap(),
            Percentual::milesimos(18_500)
        );
        assert_eq!(
            Percentual::de_str(" 7,65 % ").unwrap(),
            Percentual::milesimos(7_650)
        );
        assert!(Percentual::de_str("x").is_err());
    }

    #[test]
    fn complemento_e_razao() {
        assert_eq!(Percentual::pontos(20).complemento(), Percentual::pontos(80));
        assert_eq!(Percentual::da_razao(1, 4), Percentual::pontos(25));
        assert_eq!(Percentual::da_razao(1, 0), Percentual::ZERO);
    }
}
