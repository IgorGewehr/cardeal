//! Valor monetário exato.
//!
//! [`Dinheiro`] é um `i64` de centavos. Nunca ponto flutuante — nem no cálculo intermediário,
//! nem "só para exibir". Ver `docs/15-convencoes-codigo.md` §8.

use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::erro::{CodigoErro, Erro, Resultado};
use crate::percentual::Percentual;
use crate::quantidade::{Preco, Quantidade};

/// Modo de arredondamento. Sempre explícito — não existe conversão implícita.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Arredondamento {
    /// Metade para longe do zero. É o arredondamento comercial brasileiro e o padrão
    /// esperado pela SEFAZ no cálculo de tributos. `2,345 → 2,35`, `-2,345 → -2,35`.
    MeioAcima,
    /// Metade para o par mais próximo ("do banqueiro"). Não enviesa somas longas;
    /// usado em rateios internos e em estatística. `2,345 → 2,34`, `2,355 → 2,36`.
    MeioPar,
    /// Sempre para baixo (piso). `2,999 → 2,99`, `-2,001 → -2,01`.
    Baixo,
    /// Sempre para cima (teto). `2,001 → 2,01`, `-2,999 → -2,99`.
    Cima,
    /// Descarta a fração, em direção ao zero. `2,999 → 2,99`, `-2,999 → -2,99`.
    Truncar,
}

/// Sinal de um valor. Usado na interface para escolher cor e ícone sem repetir comparações.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Sinal {
    /// Entrada de dinheiro.
    Positivo,
    /// Saída de dinheiro.
    Negativo,
    /// Exatamente zero.
    Zero,
}

/// Divisão inteira com arredondamento controlado. `denominador` deve ser positivo.
#[allow(clippy::cast_possible_truncation)]
fn div_arredondada(numerador: i128, denominador: i128, modo: Arredondamento) -> i128 {
    debug_assert!(denominador > 0, "denominador deve ser positivo");
    let quociente = numerador / denominador; // trunca em direção ao zero
    let resto = numerador % denominador; // mesmo sinal do numerador
    if resto == 0 {
        return quociente;
    }
    let negativo = numerador < 0;
    let dobro = resto.abs() * 2;
    let para_longe = if negativo {
        quociente - 1
    } else {
        quociente + 1
    };

    match modo {
        Arredondamento::Truncar => quociente,
        Arredondamento::Baixo => {
            if negativo {
                quociente - 1
            } else {
                quociente
            }
        }
        Arredondamento::Cima => {
            if negativo {
                quociente
            } else {
                quociente + 1
            }
        }
        Arredondamento::MeioAcima => {
            if dobro >= denominador {
                para_longe
            } else {
                quociente
            }
        }
        Arredondamento::MeioPar => {
            if dobro > denominador {
                para_longe
            } else if dobro < denominador {
                quociente
            } else if quociente % 2 == 0 {
                quociente
            } else {
                para_longe
            }
        }
    }
}

/// Valor monetário em centavos de real.
///
/// A faixa representável vai de aproximadamente `-92 quatrilhões` a `+92 quatrilhões` de reais.
/// Toda operação aritmética é checada contra estouro (`checked_*`); as versões com operadores
/// entram em pânico em `debug` e saturam em `release` — mas na prática nenhum valor de ERP
/// brasileiro chega perto do limite.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Dinheiro(i64);

impl Dinheiro {
    /// Zero.
    pub const ZERO: Self = Self(0);
    /// Maior valor representável.
    pub const MAX: Self = Self(i64::MAX);
    /// Menor valor representável.
    pub const MIN: Self = Self(i64::MIN);

    /// Constrói a partir de centavos.
    #[inline]
    pub const fn centavos(centavos: i64) -> Self {
        Self(centavos)
    }

    /// Constrói a partir de reais inteiros. `Dinheiro::reais(15)` é `R$ 15,00`.
    #[inline]
    pub const fn reais(reais: i64) -> Self {
        Self(reais * 100)
    }

    /// Constrói a partir de reais e centavos separados. O sinal vem de `reais`.
    #[inline]
    pub const fn partes(reais: i64, centavos: u8) -> Self {
        let c = centavos as i64;
        if reais < 0 {
            Self(reais * 100 - c)
        } else {
            Self(reais * 100 + c)
        }
    }

    /// O valor em centavos.
    #[inline]
    pub const fn em_centavos(self) -> i64 {
        self.0
    }

    /// A parte inteira, em reais (trunca em direção ao zero).
    #[inline]
    pub const fn parte_inteira(self) -> i64 {
        self.0 / 100
    }

    /// Os centavos, sem sinal, de 0 a 99.
    #[inline]
    pub const fn parte_centavos(self) -> u8 {
        (self.0 % 100).unsigned_abs() as u8
    }

    /// Valor absoluto.
    #[inline]
    pub const fn abs(self) -> Self {
        Self(self.0.abs())
    }

    /// O sinal do valor.
    #[inline]
    pub const fn sinal(self) -> Sinal {
        if self.0 > 0 {
            Sinal::Positivo
        } else if self.0 < 0 {
            Sinal::Negativo
        } else {
            Sinal::Zero
        }
    }

    /// Verdadeiro se for exatamente zero.
    #[inline]
    pub const fn e_zero(self) -> bool {
        self.0 == 0
    }

    /// Verdadeiro se for maior que zero.
    #[inline]
    pub const fn e_positivo(self) -> bool {
        self.0 > 0
    }

    /// Verdadeiro se for menor que zero.
    #[inline]
    pub const fn e_negativo(self) -> bool {
        self.0 < 0
    }

    /// Soma checada.
    #[inline]
    pub const fn soma_checada(self, outro: Self) -> Option<Self> {
        match self.0.checked_add(outro.0) {
            Some(v) => Some(Self(v)),
            None => None,
        }
    }

    /// Subtração checada.
    #[inline]
    pub const fn subtracao_checada(self, outro: Self) -> Option<Self> {
        match self.0.checked_sub(outro.0) {
            Some(v) => Some(Self(v)),
            None => None,
        }
    }

    /// O menor entre dois valores.
    #[inline]
    pub fn min(self, outro: Self) -> Self {
        Self(self.0.min(outro.0))
    }

    /// O maior entre dois valores.
    #[inline]
    pub fn max(self, outro: Self) -> Self {
        Self(self.0.max(outro.0))
    }

    /// Limita o valor a zero pela esquerda (nunca negativo).
    #[inline]
    pub fn nao_negativo(self) -> Self {
        Self(self.0.max(0))
    }

    // ── conversões ───────────────────────────────────────────────────────────

    /// Total de uma linha: quantidade × preço unitário.
    ///
    /// A multiplicação é feita em `i128` na escala combinada (1e-4 × 1e-6 = 1e-10) e só então
    /// reduzida a centavos. Arredondar antes produziria divergência de centavos na nota fiscal.
    #[allow(clippy::cast_possible_truncation)]
    pub fn de_total(quantidade: Quantidade, preco: Preco, modo: Arredondamento) -> Self {
        let bruto =
            i128::from(quantidade.unidades_internas()) * i128::from(preco.unidades_internas());
        // 1e-10 → 1e-2 exige dividir por 1e8.
        Self(div_arredondada(bruto, 100_000_000, modo) as i64)
    }

    /// Aplica um percentual sobre o valor. `R$ 100,00` a `18%` dá `R$ 18,00`.
    #[allow(clippy::cast_possible_truncation)]
    pub fn aplicar(self, percentual: Percentual, modo: Arredondamento) -> Self {
        let bruto = i128::from(self.0) * i128::from(percentual.unidades_internas());
        // percentual tem escala 1e-6 de ponto percentual → dividir por 100 * 1e6.
        Self(div_arredondada(bruto, 100_000_000, modo) as i64)
    }

    /// Multiplica por uma fração `numerador/denominador` com arredondamento explícito.
    /// Útil em rateio de frete, seguro e despesas acessórias.
    ///
    /// # Panics
    /// Se `denominador` for zero.
    #[allow(clippy::cast_possible_truncation)]
    pub fn fracao(self, numerador: i64, denominador: i64, modo: Arredondamento) -> Self {
        assert!(denominador != 0, "denominador não pode ser zero");
        let (num, den) = if denominador < 0 {
            (-numerador, -denominador)
        } else {
            (numerador, denominador)
        };
        let bruto = i128::from(self.0) * i128::from(num);
        Self(div_arredondada(bruto, i128::from(den), modo) as i64)
    }

    // ── rateio ───────────────────────────────────────────────────────────────

    /// Divide o valor em `partes` iguais **sem perder nem criar centavos**.
    ///
    /// Os centavos restantes são distribuídos um a um nas primeiras parcelas — o mesmo
    /// comportamento que o mercado espera de um carnê (a primeira parcela é a "quebrada").
    /// A diferença entre a maior e a menor parcela nunca passa de um centavo.
    ///
    /// ```
    /// # use cardeal_kernel::Dinheiro;
    /// let partes = Dinheiro::reais(100).ratear(3);
    /// assert_eq!(partes[0], Dinheiro::centavos(3334));
    /// assert_eq!(partes[1], Dinheiro::centavos(3333));
    /// assert_eq!(partes[2], Dinheiro::centavos(3333));
    /// assert_eq!(partes.iter().copied().sum::<Dinheiro>(), Dinheiro::reais(100));
    /// ```
    ///
    /// # Panics
    /// Se `partes` for zero.
    pub fn ratear(self, partes: usize) -> Vec<Self> {
        assert!(partes > 0, "não é possível ratear em zero partes");
        let n = i64::try_from(partes).expect("número de parcelas absurdo");
        let base = self.0 / n;
        let mut restante = (self.0 % n).abs();
        let passo = if self.0 < 0 { -1 } else { 1 };

        (0..partes)
            .map(|_| {
                let mut valor = base;
                if restante > 0 {
                    valor += passo;
                    restante -= 1;
                }
                Self(valor)
            })
            .collect()
    }

    /// Rateia o valor proporcionalmente a pesos, pelo método do maior resto.
    ///
    /// A soma do resultado é sempre exatamente igual ao valor original. Pesos negativos são
    /// tratados como zero. Se todos os pesos forem zero, cai no rateio igualitário.
    ///
    /// ```
    /// # use cardeal_kernel::Dinheiro;
    /// // Rateio de R$ 100,00 de frete por valor de item.
    /// let partes = Dinheiro::reais(100).ratear_por_pesos(&[7000, 2000, 1000]);
    /// assert_eq!(partes.iter().copied().sum::<Dinheiro>(), Dinheiro::reais(100));
    /// ```
    ///
    /// # Panics
    /// Se `pesos` estiver vazio.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn ratear_por_pesos(self, pesos: &[i64]) -> Vec<Self> {
        assert!(!pesos.is_empty(), "é preciso ao menos um peso");
        let total: i128 = pesos.iter().map(|p| i128::from((*p).max(0))).sum();
        if total == 0 {
            return self.ratear(pesos.len());
        }

        let valor = i128::from(self.0);
        let mut partes: Vec<i64> = Vec::with_capacity(pesos.len());
        // (resto, índice) para decidir quem recebe os centavos que sobraram.
        let mut restos: Vec<(i128, usize)> = Vec::with_capacity(pesos.len());
        let mut distribuido: i128 = 0;

        for (i, peso) in pesos.iter().enumerate() {
            let p = i128::from((*peso).max(0));
            let bruto = valor * p;
            let quociente = bruto / total;
            let resto = (bruto % total).abs();
            partes.push(quociente as i64);
            restos.push((resto, i));
            distribuido += quociente;
        }

        let mut sobra = valor - distribuido;
        let passo: i64 = if sobra < 0 { -1 } else { 1 };
        // Maior resto primeiro; empate resolvido pela ordem original (determinístico).
        restos.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        let mut k = 0;
        while sobra != 0 && k < restos.len() {
            partes[restos[k].1] += passo;
            sobra -= i128::from(passo);
            k += 1;
        }

        partes.into_iter().map(Self).collect()
    }

    // ── formatação ───────────────────────────────────────────────────────────

    /// Formata no padrão brasileiro com separador de milhar: `1.234,56`.
    pub fn formatar(self) -> String {
        let negativo = self.0 < 0;
        let abs = self.0.unsigned_abs();
        let inteiros = abs / 100;
        let centavos = abs % 100;

        let d = inteiros.to_string();
        let mut com_pontos = String::with_capacity(d.len() + d.len() / 3 + 4);
        for (i, c) in d.chars().enumerate() {
            if i > 0 && (d.len() - i) % 3 == 0 {
                com_pontos.push('.');
            }
            com_pontos.push(c);
        }

        if negativo {
            format!("-{com_pontos},{centavos:02}")
        } else {
            format!("{com_pontos},{centavos:02}")
        }
    }

    /// Formata com o símbolo da moeda: `R$ 1.234,56`.
    pub fn formatar_com_simbolo(self) -> String {
        let s = self.formatar();
        if let Some(resto) = s.strip_prefix('-') {
            format!("-R$ {resto}")
        } else {
            format!("R$ {s}")
        }
    }

    /// Lê um valor no padrão brasileiro ou "de máquina".
    ///
    /// Aceita `1.234,56`, `1234,56`, `1234.56`, `1234`, `R$ 1.234,56`, `-1.234,56`, `(1.234,56)`
    /// (parênteses = negativo, convenção contábil).
    ///
    /// # Errors
    /// Devolve [`CodigoErro::VALOR_INVALIDO`] se o texto não representar um valor monetário.
    pub fn de_str(texto: &str) -> Resultado<Self> {
        let bruto = texto.trim();
        let sem_moeda = bruto
            .trim_start_matches("R$")
            .trim_start_matches("r$")
            .trim();

        let (sem_moeda, negativo_parenteses) = match sem_moeda
            .strip_prefix('(')
            .and_then(|s| s.strip_suffix(')'))
        {
            Some(interno) => (interno.trim(), true),
            None => (sem_moeda, false),
        };

        let (corpo, negativo) = match sem_moeda.strip_prefix('-') {
            Some(resto) => (resto.trim(), true),
            None => (sem_moeda.trim_start_matches('+').trim(), false),
        };
        let negativo = negativo || negativo_parenteses;

        if corpo.is_empty() {
            return Err(Erro::novo(
                CodigoErro::VALOR_INVALIDO,
                "Valor monetário vazio",
            ));
        }

        // Decide qual caractere é o separador decimal: o último entre '.' e ','.
        let pos_virgula = corpo.rfind(',');
        let pos_ponto = corpo.rfind('.');
        let separador = match (pos_virgula, pos_ponto) {
            (Some(v), Some(p)) => Some(if v > p { ',' } else { '.' }),
            (Some(_), None) => Some(','),
            (None, Some(p)) => {
                // Um ponto isolado só é decimal se sobrarem 1 ou 2 dígitos depois dele;
                // "1.234" é milhar, "1.23" é decimal.
                let depois = corpo.len() - p - 1;
                if depois <= 2 {
                    Some('.')
                } else {
                    None
                }
            }
            (None, None) => None,
        };

        let (parte_inteira, parte_decimal) = match separador {
            Some(sep) => {
                let pos = corpo.rfind(sep).expect("separador localizado acima");
                (&corpo[..pos], &corpo[pos + 1..])
            }
            None => (corpo, ""),
        };

        let limpar = |s: &str| -> Resultado<u64> {
            let filtrado: String = s
                .chars()
                .filter(|c| !matches!(c, '.' | ',' | ' ' | '\u{a0}' | '_'))
                .collect();
            if filtrado.is_empty() {
                return Ok(0);
            }
            if !filtrado.chars().all(|c| c.is_ascii_digit()) {
                return Err(Erro::novo(
                    CodigoErro::VALOR_INVALIDO,
                    format!("\"{s}\" não é um número"),
                ));
            }
            filtrado.parse::<u64>().map_err(|_| {
                Erro::novo(CodigoErro::VALOR_INVALIDO, "Valor monetário fora da faixa")
            })
        };

        let inteiros = limpar(parte_inteira)?;
        let centavos = match parte_decimal.len() {
            0 => 0,
            1 => limpar(parte_decimal)? * 10,
            2 => limpar(parte_decimal)?,
            _ => {
                // Mais de duas casas: arredonda comercialmente para centavos.
                let digitos: String = parte_decimal.chars().filter(char::is_ascii_digit).collect();
                let escala = 10u64.pow(u32::try_from(digitos.len()).unwrap_or(2));
                let bruto: u64 = digitos.parse().map_err(|_| {
                    Erro::novo(CodigoErro::VALOR_INVALIDO, "Casas decimais em excesso")
                })?;
                let n = div_arredondada(
                    i128::from(bruto) * 100,
                    i128::from(escala),
                    Arredondamento::MeioAcima,
                );
                u64::try_from(n).unwrap_or(0)
            }
        };

        let total = inteiros
            .checked_mul(100)
            .and_then(|v| v.checked_add(centavos))
            .and_then(|v| i64::try_from(v).ok())
            .ok_or_else(|| {
                Erro::novo(CodigoErro::VALOR_INVALIDO, "Valor monetário fora da faixa")
            })?;

        Ok(Self(if negativo { -total } else { total }))
    }
}

// ── operadores ───────────────────────────────────────────────────────────────

impl Add for Dinheiro {
    type Output = Self;
    #[inline]
    fn add(self, outro: Self) -> Self {
        Self(
            self.0
                .checked_add(outro.0)
                .expect("estouro ao somar dinheiro"),
        )
    }
}

impl Sub for Dinheiro {
    type Output = Self;
    #[inline]
    fn sub(self, outro: Self) -> Self {
        Self(
            self.0
                .checked_sub(outro.0)
                .expect("estouro ao subtrair dinheiro"),
        )
    }
}

impl Neg for Dinheiro {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Self(-self.0)
    }
}

impl AddAssign for Dinheiro {
    #[inline]
    fn add_assign(&mut self, outro: Self) {
        *self = *self + outro;
    }
}

impl SubAssign for Dinheiro {
    #[inline]
    fn sub_assign(&mut self, outro: Self) {
        *self = *self - outro;
    }
}

impl Mul<i64> for Dinheiro {
    type Output = Self;
    #[inline]
    fn mul(self, fator: i64) -> Self {
        Self(
            self.0
                .checked_mul(fator)
                .expect("estouro ao multiplicar dinheiro"),
        )
    }
}

impl Mul<Dinheiro> for i64 {
    type Output = Dinheiro;
    #[inline]
    fn mul(self, valor: Dinheiro) -> Dinheiro {
        valor * self
    }
}

impl Div<i64> for Dinheiro {
    type Output = Self;
    /// Divisão **truncada**. Para dividir sem perder centavos, use
    /// [`Dinheiro::ratear`] ou [`Dinheiro::ratear_por_pesos`].
    #[inline]
    fn div(self, divisor: i64) -> Self {
        assert!(divisor != 0, "divisão de dinheiro por zero");
        Self(self.0 / divisor)
    }
}

impl Sum for Dinheiro {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ZERO, |a, b| a + b)
    }
}

impl<'a> Sum<&'a Dinheiro> for Dinheiro {
    fn sum<I: Iterator<Item = &'a Self>>(iter: I) -> Self {
        iter.copied().sum()
    }
}

impl fmt::Display for Dinheiro {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.formatar())
    }
}

impl fmt::Debug for Dinheiro {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Dinheiro({})", self.formatar())
    }
}

impl FromStr for Dinheiro {
    type Err = Erro;
    fn from_str(s: &str) -> Resultado<Self> {
        Self::de_str(s)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn formatacao_brasileira() {
        assert_eq!(Dinheiro::centavos(0).formatar(), "0,00");
        assert_eq!(Dinheiro::centavos(5).formatar(), "0,05");
        assert_eq!(Dinheiro::centavos(123_456).formatar(), "1.234,56");
        assert_eq!(Dinheiro::centavos(-123_456).formatar(), "-1.234,56");
        assert_eq!(Dinheiro::centavos(100_000_000).formatar(), "1.000.000,00");
        assert_eq!(Dinheiro::reais(15).formatar_com_simbolo(), "R$ 15,00");
        assert_eq!(Dinheiro::reais(-15).formatar_com_simbolo(), "-R$ 15,00");
    }

    #[test]
    fn leitura_de_texto() {
        let casos = [
            ("1.234,56", 123_456),
            ("1234,56", 123_456),
            ("1234.56", 123_456),
            ("R$ 1.234,56", 123_456),
            ("-1.234,56", -123_456),
            ("(1.234,56)", -123_456),
            ("1.234", 123_400),
            ("0,05", 5),
            ("5", 500),
            ("2,999", 300), // arredondamento comercial
        ];
        for (texto, esperado) in casos {
            assert_eq!(
                Dinheiro::de_str(texto).unwrap(),
                Dinheiro::centavos(esperado),
                "falhou ao ler {texto:?}"
            );
        }
        assert!(Dinheiro::de_str("abc").is_err());
        assert!(Dinheiro::de_str("").is_err());
    }

    #[test]
    fn rateio_conserva_o_total() {
        for total in [1, 7, 100, 10_000, 999_999, -100, -7] {
            for n in 1..=13usize {
                let partes = Dinheiro::centavos(total).ratear(n);
                assert_eq!(partes.len(), n);
                assert_eq!(
                    partes.iter().copied().sum::<Dinheiro>(),
                    Dinheiro::centavos(total),
                    "total {total} em {n} partes"
                );
                let maior = partes.iter().map(|d| d.0).max().unwrap();
                let menor = partes.iter().map(|d| d.0).min().unwrap();
                assert!(maior - menor <= 1);
            }
        }
    }

    #[test]
    fn rateio_por_pesos_conserva_o_total() {
        let casos: [&[i64]; 5] = [
            &[1, 1, 1],
            &[7000, 2000, 1000],
            &[1, 0, 0],
            &[0, 0, 0],
            &[333, 333, 334, 1],
        ];
        for pesos in casos {
            let partes = Dinheiro::centavos(10_000).ratear_por_pesos(pesos);
            assert_eq!(partes.len(), pesos.len());
            assert_eq!(
                partes.iter().copied().sum::<Dinheiro>(),
                Dinheiro::centavos(10_000),
                "pesos {pesos:?}"
            );
        }
    }

    #[test]
    fn arredondamento_comercial_e_bancario() {
        // 2,345 arredondado a duas casas
        let bruto = 2345i128;
        assert_eq!(div_arredondada(bruto, 10, Arredondamento::MeioAcima), 235);
        assert_eq!(div_arredondada(bruto, 10, Arredondamento::MeioPar), 234);
        assert_eq!(div_arredondada(-bruto, 10, Arredondamento::MeioAcima), -235);
        assert_eq!(div_arredondada(-bruto, 10, Arredondamento::MeioPar), -234);
        assert_eq!(div_arredondada(bruto, 10, Arredondamento::Baixo), 234);
        assert_eq!(div_arredondada(bruto, 10, Arredondamento::Cima), 235);
        assert_eq!(div_arredondada(-bruto, 10, Arredondamento::Baixo), -235);
        assert_eq!(div_arredondada(-bruto, 10, Arredondamento::Cima), -234);
        assert_eq!(div_arredondada(-bruto, 10, Arredondamento::Truncar), -234);
    }

    #[test]
    fn aplicar_percentual() {
        let base = Dinheiro::reais(100);
        assert_eq!(
            base.aplicar(Percentual::pontos(18), Arredondamento::MeioAcima),
            Dinheiro::reais(18)
        );
        assert_eq!(
            Dinheiro::centavos(1999).aplicar(
                Percentual::de_str("18,00").unwrap(),
                Arredondamento::MeioAcima
            ),
            Dinheiro::centavos(360) // 3,5982 → 3,60
        );
    }

    #[test]
    fn total_de_linha() {
        let qtd = Quantidade::de_str("2,5").unwrap();
        let preco = Preco::de_str("18,90").unwrap();
        assert_eq!(
            Dinheiro::de_total(qtd, preco, Arredondamento::MeioAcima),
            Dinheiro::centavos(4725)
        );
        // Combustível: 37,482 litros a R$ 5,499
        let litros = Quantidade::de_str("37,482").unwrap();
        let preco_litro = Preco::de_str("5,499").unwrap();
        assert_eq!(
            Dinheiro::de_total(litros, preco_litro, Arredondamento::MeioAcima),
            Dinheiro::centavos(20611) // 206,113518... → 206,11
        );
    }

    #[test]
    fn partes_e_sinal() {
        let d = Dinheiro::partes(-12, 34);
        assert_eq!(d, Dinheiro::centavos(-1234));
        assert_eq!(d.parte_inteira(), -12);
        assert_eq!(d.parte_centavos(), 34);
        assert_eq!(d.sinal(), Sinal::Negativo);
        assert_eq!(Dinheiro::ZERO.sinal(), Sinal::Zero);
    }
}
