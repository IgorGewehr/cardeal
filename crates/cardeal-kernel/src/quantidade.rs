//! Quantidade e preço unitário.
//!
//! Duas escalas distintas, ditadas pelo leiaute da NF-e:
//!
//! - [`Quantidade`] — escala **1e-4** (`qCom` admite 4 casas decimais).
//! - [`Preco`] — escala **1e-6** (`vUnCom` admite até 10 casas; 6 cobrem todo caso real:
//!   combustível a `5,499000`, medicamento fracionado, venda a granel).
//!
//! A conversão para [`crate::Dinheiro`] é feita por [`crate::Dinheiro::de_total`], que
//! multiplica em `i128` na escala combinada e só então arredonda — nunca antes.

use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Neg, Sub, SubAssign};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::erro::{CodigoErro, Erro, Resultado};

const ESCALA_QTD: i64 = 10_000;
const ESCALA_PRECO: i64 = 1_000_000;

/// Separa a parte inteira da decimal aceitando os dois padrões que aparecem na prática.
///
/// Regra: se houver `,`, ela é o separador decimal e todo `.` é milhar. Havendo apenas `.`,
/// ele é milhar quando são exatamente 3 dígitos depois do último ponto (`1.234` = mil duzentos
/// e trinta e quatro) e decimal caso contrário (`2.5`, `2.5678`).
fn normalizar_decimal_br(corpo: &str) -> String {
    if corpo.contains(',') {
        return corpo.replace('.', "").replace(',', ".");
    }
    match corpo.rfind('.') {
        Some(pos) if corpo.len() - pos - 1 == 3 => corpo.replace('.', ""),
        Some(pos) => {
            let (int, dec) = corpo.split_at(pos);
            format!("{}.{}", int.replace('.', ""), &dec[1..])
        }
        None => corpo.to_string(),
    }
}

/// Lê um decimal em texto para a escala inteira pedida, arredondando o excedente
/// pela metade para cima.
fn ler_decimal(texto: &str, casas: u32, o_que: &str) -> Resultado<i64> {
    let bruto = texto.trim();
    let (corpo, negativo) = match bruto.strip_prefix('-') {
        Some(r) => (r.trim(), true),
        None => (bruto.trim_start_matches('+').trim(), false),
    };
    if corpo.is_empty() {
        return Err(Erro::novo(
            CodigoErro::VALOR_INVALIDO,
            format!("{o_que} vazio(a)"),
        ));
    }

    let normalizado = normalizar_decimal_br(corpo);
    let (int_txt, dec_txt) = match normalizado.split_once('.') {
        Some((i, d)) => (i, d),
        None => (normalizado.as_str(), ""),
    };
    if !int_txt.chars().all(|c| c.is_ascii_digit()) || !dec_txt.chars().all(|c| c.is_ascii_digit())
    {
        return Err(Erro::novo(
            CodigoErro::VALOR_INVALIDO,
            format!("\"{texto}\" não é um(a) {o_que} válido(a)"),
        ));
    }

    let inteiros: i64 = if int_txt.is_empty() {
        0
    } else {
        int_txt
            .parse()
            .map_err(|_| Erro::novo(CodigoErro::VALOR_INVALIDO, format!("{o_que} fora da faixa")))?
    };

    // Uma casa a mais para poder arredondar corretamente.
    let mut dec = dec_txt.to_string();
    let alvo = casas as usize + 1;
    dec.truncate(alvo);
    while dec.len() < alvo {
        dec.push('0');
    }
    let com_guarda: i64 = dec.parse().unwrap_or(0);
    let fracao = (com_guarda + 5) / 10;

    let escala = 10i64.pow(casas);
    let total = inteiros
        .checked_mul(escala)
        .and_then(|v| v.checked_add(fracao))
        .ok_or_else(|| Erro::novo(CodigoErro::VALOR_INVALIDO, format!("{o_que} fora da faixa")))?;

    Ok(if negativo { -total } else { total })
}

/// Formata um inteiro escalado no padrão brasileiro, removendo zeros à direita
/// mas mantendo ao menos `casas_minimas`.
fn formatar_decimal(valor: i64, casas: u32, casas_minimas: u32) -> String {
    let negativo = valor < 0;
    let abs = valor.unsigned_abs();
    let escala = 10u64.pow(casas);
    let inteiros = abs / escala;
    let fracao = abs % escala;

    let mut frac_txt = format!("{fracao:0largura$}", largura = casas as usize);
    while frac_txt.len() > casas_minimas as usize && frac_txt.ends_with('0') {
        frac_txt.pop();
    }

    let sinal = if negativo { "-" } else { "" };
    if frac_txt.is_empty() {
        format!("{sinal}{inteiros}")
    } else {
        format!("{sinal}{inteiros},{frac_txt}")
    }
}

/// Quantidade de itens, com 4 casas decimais.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Quantidade(i64);

impl Quantidade {
    /// Zero.
    pub const ZERO: Self = Self(0);
    /// Uma unidade.
    pub const UM: Self = Self(ESCALA_QTD);

    /// Constrói a partir de unidades inteiras.
    #[inline]
    pub const fn unidades(unidades: i64) -> Self {
        Self(unidades * ESCALA_QTD)
    }

    /// Constrói a partir de milésimos. `Quantidade::milesimos(1_500)` é `1,5`.
    #[inline]
    pub const fn milesimos(milesimos: i64) -> Self {
        Self(milesimos * 10)
    }

    /// Constrói diretamente das unidades internas (escala 1e-4).
    #[inline]
    pub const fn interna(unidades: i64) -> Self {
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

    /// Verdadeiro se for maior que zero.
    #[inline]
    pub const fn e_positiva(self) -> bool {
        self.0 > 0
    }

    /// Verdadeiro se for menor que zero.
    #[inline]
    pub const fn e_negativa(self) -> bool {
        self.0 < 0
    }

    /// Verdadeiro se não tiver parte fracionária (relevante para itens indivisíveis).
    #[inline]
    pub const fn e_inteira(self) -> bool {
        self.0 % ESCALA_QTD == 0
    }

    /// Valor absoluto.
    #[inline]
    pub const fn abs(self) -> Self {
        Self(self.0.abs())
    }

    /// O menor entre duas quantidades.
    #[inline]
    pub fn min(self, outra: Self) -> Self {
        Self(self.0.min(outra.0))
    }

    /// A maior entre duas quantidades.
    #[inline]
    pub fn max(self, outra: Self) -> Self {
        Self(self.0.max(outra.0))
    }

    /// Limita a zero pela esquerda.
    #[inline]
    pub fn nao_negativa(self) -> Self {
        Self(self.0.max(0))
    }

    /// Multiplica por um fator inteiro.
    #[inline]
    pub fn vezes(self, fator: i64) -> Self {
        Self(self.0.checked_mul(fator).expect("estouro em quantidade"))
    }

    /// Converte para outra unidade aplicando um fator de conversão racional.
    /// Exemplo: caixa com 12 unidades → `converter(12, 1)`.
    ///
    /// # Panics
    /// Se `denominador` for zero.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn converter(self, numerador: i64, denominador: i64) -> Self {
        assert!(denominador != 0, "fator de conversão com denominador zero");
        let bruto = i128::from(self.0) * i128::from(numerador);
        Self((bruto / i128::from(denominador)) as i64)
    }

    /// Formata no padrão brasileiro, com no mínimo `casas_minimas` decimais.
    pub fn formatar(self, casas_minimas: u8) -> String {
        formatar_decimal(self.0, 4, u32::from(casas_minimas))
    }

    /// Lê uma quantidade. Aceita `2`, `2,5`, `2.5`, `1.234,5678`.
    ///
    /// # Errors
    /// Devolve [`CodigoErro::VALOR_INVALIDO`] se o texto não representar um número.
    pub fn de_str(texto: &str) -> Resultado<Self> {
        ler_decimal(&texto.replace(' ', ""), 4, "quantidade").map(Self)
    }
}

/// Preço unitário, com 6 casas decimais.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Preco(i64);

impl Preco {
    /// Zero.
    pub const ZERO: Self = Self(0);

    /// Constrói a partir de reais inteiros.
    #[inline]
    pub const fn reais(reais: i64) -> Self {
        Self(reais * ESCALA_PRECO)
    }

    /// Constrói a partir de centavos. `Preco::centavos(549)` é `R$ 5,49`.
    #[inline]
    pub const fn centavos(centavos: i64) -> Self {
        Self(centavos * 10_000)
    }

    /// Constrói a partir de milésimos de real. `Preco::milesimos(5_499)` é `R$ 5,499`.
    #[inline]
    pub const fn milesimos(milesimos: i64) -> Self {
        Self(milesimos * 1_000)
    }

    /// Constrói diretamente das unidades internas (escala 1e-6).
    #[inline]
    pub const fn interna(unidades: i64) -> Self {
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

    /// Formata no padrão brasileiro, com no mínimo duas casas: `5,499`, `18,90`.
    pub fn formatar(self) -> String {
        formatar_decimal(self.0, 6, 2)
    }

    /// Formata com símbolo: `R$ 5,499`.
    pub fn formatar_com_simbolo(self) -> String {
        let s = self.formatar();
        if let Some(resto) = s.strip_prefix('-') {
            format!("-R$ {resto}")
        } else {
            format!("R$ {s}")
        }
    }

    /// Lê um preço. Aceita `5,49`, `5.499`, `R$ 5,499`, `1.234,5678`.
    ///
    /// # Errors
    /// Devolve [`CodigoErro::VALOR_INVALIDO`] se o texto não representar um número.
    pub fn de_str(texto: &str) -> Resultado<Self> {
        let sem_moeda = texto
            .trim()
            .trim_start_matches("R$")
            .trim_start_matches("r$")
            .trim();
        ler_decimal(&sem_moeda.replace(' ', ""), 6, "preço").map(Self)
    }
}

/// Unidade de medida comercial. O código de duas letras é o que vai na NF-e (`uCom`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Unidade {
    /// Unidade.
    Un,
    /// Peça.
    Pc,
    /// Caixa.
    Cx,
    /// Fardo.
    Fd,
    /// Pacote.
    Pct,
    /// Quilograma.
    Kg,
    /// Grama.
    G,
    /// Tonelada.
    Ton,
    /// Litro.
    L,
    /// Mililitro.
    Ml,
    /// Metro.
    M,
    /// Metro quadrado.
    M2,
    /// Metro cúbico.
    M3,
    /// Centímetro.
    Cm,
    /// Par.
    Par,
    /// Dúzia.
    Duzia,
    /// Hora (serviços).
    Hora,
    /// Diária (hotelaria, locação).
    Diaria,
    /// Mês (contratos recorrentes).
    Mes,
    /// Kilowatt-hora.
    Kwh,
    /// Serviço (unidade genérica de prestação).
    Serv,
}

impl Unidade {
    /// O código usado na nota fiscal.
    pub const fn codigo(self) -> &'static str {
        match self {
            Self::Un => "UN",
            Self::Pc => "PC",
            Self::Cx => "CX",
            Self::Fd => "FD",
            Self::Pct => "PCT",
            Self::Kg => "KG",
            Self::G => "G",
            Self::Ton => "TON",
            Self::L => "L",
            Self::Ml => "ML",
            Self::M => "M",
            Self::M2 => "M2",
            Self::M3 => "M3",
            Self::Cm => "CM",
            Self::Par => "PAR",
            Self::Duzia => "DZ",
            Self::Hora => "H",
            Self::Diaria => "DIA",
            Self::Mes => "MES",
            Self::Kwh => "KWH",
            Self::Serv => "SERV",
        }
    }

    /// Verdadeiro se a unidade admite quantidade fracionária (granel, peso, volume).
    pub const fn admite_fracao(self) -> bool {
        matches!(
            self,
            Self::Kg
                | Self::G
                | Self::Ton
                | Self::L
                | Self::Ml
                | Self::M
                | Self::M2
                | Self::M3
                | Self::Cm
                | Self::Hora
                | Self::Kwh
        )
    }

    /// Nome por extenso, para exibição.
    pub const fn nome(self) -> &'static str {
        match self {
            Self::Un => "unidade",
            Self::Pc => "peça",
            Self::Cx => "caixa",
            Self::Fd => "fardo",
            Self::Pct => "pacote",
            Self::Kg => "quilograma",
            Self::G => "grama",
            Self::Ton => "tonelada",
            Self::L => "litro",
            Self::Ml => "mililitro",
            Self::M => "metro",
            Self::M2 => "metro quadrado",
            Self::M3 => "metro cúbico",
            Self::Cm => "centímetro",
            Self::Par => "par",
            Self::Duzia => "dúzia",
            Self::Hora => "hora",
            Self::Diaria => "diária",
            Self::Mes => "mês",
            Self::Kwh => "kWh",
            Self::Serv => "serviço",
        }
    }
}

impl fmt::Display for Unidade {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.codigo())
    }
}

// ── operadores de Quantidade ─────────────────────────────────────────────────

impl Add for Quantidade {
    type Output = Self;
    #[inline]
    fn add(self, o: Self) -> Self {
        Self(self.0.checked_add(o.0).expect("estouro em quantidade"))
    }
}

impl Sub for Quantidade {
    type Output = Self;
    #[inline]
    fn sub(self, o: Self) -> Self {
        Self(self.0.checked_sub(o.0).expect("estouro em quantidade"))
    }
}

impl Neg for Quantidade {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Self(-self.0)
    }
}

impl AddAssign for Quantidade {
    #[inline]
    fn add_assign(&mut self, o: Self) {
        *self = *self + o;
    }
}

impl SubAssign for Quantidade {
    #[inline]
    fn sub_assign(&mut self, o: Self) {
        *self = *self - o;
    }
}

impl Sum for Quantidade {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ZERO, |a, b| a + b)
    }
}

impl fmt::Display for Quantidade {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.formatar(0))
    }
}

impl fmt::Debug for Quantidade {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Quantidade({})", self.formatar(0))
    }
}

impl FromStr for Quantidade {
    type Err = Erro;
    fn from_str(s: &str) -> Resultado<Self> {
        Self::de_str(s)
    }
}

impl fmt::Display for Preco {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.formatar())
    }
}

impl fmt::Debug for Preco {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Preco({})", self.formatar())
    }
}

impl FromStr for Preco {
    type Err = Erro;
    fn from_str(s: &str) -> Resultado<Self> {
        Self::de_str(s)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn quantidade_leitura_e_formatacao() {
        assert_eq!(Quantidade::de_str("2").unwrap(), Quantidade::unidades(2));
        assert_eq!(
            Quantidade::de_str("2,5").unwrap(),
            Quantidade::milesimos(2_500)
        );
        assert_eq!(
            Quantidade::de_str("1.234,5678").unwrap(),
            Quantidade::interna(12_345_678)
        );
        assert_eq!(Quantidade::unidades(3).formatar(0), "3");
        assert_eq!(Quantidade::unidades(3).formatar(3), "3,000");
        assert_eq!(Quantidade::milesimos(2_500).formatar(0), "2,5");
    }

    #[test]
    fn preco_leitura_e_formatacao() {
        assert_eq!(Preco::de_str("5,49").unwrap(), Preco::centavos(549));
        assert_eq!(Preco::de_str("5,499").unwrap(), Preco::milesimos(5_499));
        assert_eq!(Preco::de_str("R$ 18,90").unwrap(), Preco::centavos(1890));
        assert_eq!(Preco::centavos(1890).formatar(), "18,90");
        assert_eq!(Preco::milesimos(5_499).formatar(), "5,499");
        assert_eq!(Preco::milesimos(5_499).formatar_com_simbolo(), "R$ 5,499");
    }

    #[test]
    fn conversao_de_unidade() {
        // Uma caixa com 12 unidades.
        assert_eq!(
            Quantidade::unidades(3).converter(12, 1),
            Quantidade::unidades(36)
        );
        assert_eq!(
            Quantidade::unidades(36).converter(1, 12),
            Quantidade::unidades(3)
        );
    }

    #[test]
    fn unidades_conhecidas() {
        assert_eq!(Unidade::Kg.codigo(), "KG");
        assert!(Unidade::Kg.admite_fracao());
        assert!(!Unidade::Un.admite_fracao());
    }
}
