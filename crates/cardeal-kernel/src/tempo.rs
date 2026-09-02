//! Datas, instantes e períodos.
//!
//! # Por que dois tipos separados
//!
//! Data de competência é **civil**: "10 de março" não tem fuso horário. Guardar isso como
//! timestamp produz a classe de bug em que o lançamento do dia 1º aparece no mês anterior.
//! Por isso:
//!
//! - [`Data`] é um `i32` de dias desde 1970-01-01, sem hora e sem fuso.
//! - [`Instante`] é um `i64` de microssegundos UTC desde a época.
//!
//! Converter de um para o outro **exige um [`Fuso`] explícito**.
//!
//! # Fuso horário
//!
//! O Brasil aboliu o horário de verão em 2019 (Decreto 9.772/2019), então todos os fusos
//! brasileiros são deslocamentos fixos. Isso nos permite representar fuso como minutos de
//! deslocamento e dispensar uma base de dados de fusos inteira — decisão coerente com o
//! Pilar I (`docs/02-pilar-eficiencia.md`).

use std::fmt;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::erro::{CodigoErro, Erro, Resultado};

const MICROS_POR_SEGUNDO: i64 = 1_000_000;
const SEGUNDOS_POR_DIA: i64 = 86_400;
const MICROS_POR_DIA: i64 = SEGUNDOS_POR_DIA * MICROS_POR_SEGUNDO;

/// Dias desde 1970-01-01 para uma data civil. Algoritmo de Howard Hinnant.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
const fn dias_de_civil(ano: i32, mes: u32, dia: u32) -> i32 {
    let a = if mes <= 2 { ano - 1 } else { ano };
    let era = if a >= 0 { a } else { a - 399 } / 400;
    let ano_na_era = (a - era * 400) as u32; // [0, 399]
    let mes_deslocado = if mes > 2 { mes - 3 } else { mes + 9 };
    let dia_no_ano = (153 * mes_deslocado + 2) / 5 + dia - 1; // [0, 365]
    let dia_na_era = ano_na_era * 365 + ano_na_era / 4 - ano_na_era / 100 + dia_no_ano;
    era * 146_097 + dia_na_era as i32 - 719_468
}

/// A data civil correspondente a um número de dias desde 1970-01-01.
///
/// As conversões `u32 as i32` abaixo nunca invertem sinal na prática: [`Data`] restringe
/// o intervalo aceito a [`Data::MINIMA`]..=[`Data::MAXIMA`] (anos 1900–2199), então
/// `dia_na_era` e `ano_na_era` ficam muito abaixo de `i32::MAX`.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
const fn civil_de_dias(dias: i32) -> (i32, u32, u32) {
    let z = dias + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let dia_na_era = (z - era * 146_097) as u32; // [0, 146096]
    let ano_na_era =
        (dia_na_era - dia_na_era / 1460 + dia_na_era / 36_524 - dia_na_era / 146_096) / 365;
    let ano = ano_na_era as i32 + era * 400;
    let dia_no_ano = dia_na_era - (365 * ano_na_era + ano_na_era / 4 - ano_na_era / 100);
    let mes_deslocado = (5 * dia_no_ano + 2) / 153; // [0, 11]
    let dia = dia_no_ano - (153 * mes_deslocado + 2) / 5 + 1;
    let mes = if mes_deslocado < 10 {
        mes_deslocado + 3
    } else {
        mes_deslocado - 9
    };
    (if mes <= 2 { ano + 1 } else { ano }, mes, dia)
}

const fn e_bissexto(ano: i32) -> bool {
    (ano % 4 == 0 && ano % 100 != 0) || ano % 400 == 0
}

const fn dias_no_mes(ano: i32, mes: u32) -> u32 {
    match mes {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if e_bissexto(ano) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Fuso horário como deslocamento fixo em minutos em relação ao UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Fuso(i16);

impl Fuso {
    /// UTC.
    pub const UTC: Self = Self(0);
    /// Horário de Brasília, UTC−3. O fuso da maior parte do país.
    pub const BRASILIA: Self = Self(-180);
    /// Amazonas, Mato Grosso, Rondônia, Roraima: UTC−4.
    pub const AMAZONAS: Self = Self(-240);
    /// Acre e sudoeste do Amazonas: UTC−5.
    pub const ACRE: Self = Self(-300);
    /// Fernando de Noronha e demais ilhas oceânicas: UTC−2.
    pub const NORONHA: Self = Self(-120);

    /// Constrói a partir de minutos de deslocamento.
    pub const fn minutos(minutos: i16) -> Self {
        Self(minutos)
    }

    /// O deslocamento em minutos.
    pub const fn em_minutos(self) -> i16 {
        self.0
    }
}

impl Default for Fuso {
    fn default() -> Self {
        Self::BRASILIA
    }
}

/// Dia da semana.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum DiaDaSemana {
    /// Domingo.
    Domingo,
    /// Segunda-feira.
    Segunda,
    /// Terça-feira.
    Terca,
    /// Quarta-feira.
    Quarta,
    /// Quinta-feira.
    Quinta,
    /// Sexta-feira.
    Sexta,
    /// Sábado.
    Sabado,
}

impl DiaDaSemana {
    /// Nome por extenso.
    pub const fn nome(self) -> &'static str {
        match self {
            Self::Domingo => "domingo",
            Self::Segunda => "segunda-feira",
            Self::Terca => "terça-feira",
            Self::Quarta => "quarta-feira",
            Self::Quinta => "quinta-feira",
            Self::Sexta => "sexta-feira",
            Self::Sabado => "sábado",
        }
    }

    /// Abreviação de três letras, para cabeçalho de grade.
    pub const fn abreviacao(self) -> &'static str {
        match self {
            Self::Domingo => "dom",
            Self::Segunda => "seg",
            Self::Terca => "ter",
            Self::Quarta => "qua",
            Self::Quinta => "qui",
            Self::Sexta => "sex",
            Self::Sabado => "sáb",
        }
    }

    /// Verdadeiro para sábado e domingo.
    pub const fn e_fim_de_semana(self) -> bool {
        matches!(self, Self::Sabado | Self::Domingo)
    }
}

/// Data civil, sem hora e sem fuso. Dias desde 1970-01-01.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Data(i32);

impl Data {
    /// 1970-01-01, a época.
    pub const EPOCA: Self = Self(0);
    /// Menor data aceita: 1900-01-01. Datas anteriores em um ERP são erro de digitação.
    pub const MINIMA: Self = Self(dias_de_civil(1900, 1, 1));
    /// Maior data aceita: 2199-12-31.
    pub const MAXIMA: Self = Self(dias_de_civil(2199, 12, 31));

    /// Constrói a partir de ano, mês e dia.
    ///
    /// # Errors
    /// Devolve [`CodigoErro::DATA_INVALIDA`] se a data não existir no calendário.
    pub fn de_ymd(ano: i32, mes: u32, dia: u32) -> Resultado<Self> {
        if !(1..=12).contains(&mes) || dia == 0 || dia > dias_no_mes(ano, mes) {
            return Err(Erro::novo(
                CodigoErro::DATA_INVALIDA,
                format!("{dia:02}/{mes:02}/{ano} não existe no calendário"),
            ));
        }
        let d = Self(dias_de_civil(ano, mes, dia));
        if d < Self::MINIMA || d > Self::MAXIMA {
            return Err(Erro::novo(
                CodigoErro::DATA_INVALIDA,
                format!("{dia:02}/{mes:02}/{ano} está fora da faixa aceita"),
            ));
        }
        Ok(d)
    }

    /// Constrói diretamente do número de dias desde a época.
    pub const fn de_dias(dias: i32) -> Self {
        Self(dias)
    }

    /// O número de dias desde a época.
    pub const fn em_dias(self) -> i32 {
        self.0
    }

    /// A data de hoje no fuso informado.
    pub fn hoje(fuso: Fuso) -> Self {
        Instante::agora().data(fuso)
    }

    /// O ano.
    pub const fn ano(self) -> i32 {
        civil_de_dias(self.0).0
    }

    /// O mês, de 1 a 12.
    pub const fn mes(self) -> u32 {
        civil_de_dias(self.0).1
    }

    /// O dia do mês, de 1 a 31.
    pub const fn dia(self) -> u32 {
        civil_de_dias(self.0).2
    }

    /// Ano, mês e dia de uma vez (mais barato que chamar os três).
    pub const fn ymd(self) -> (i32, u32, u32) {
        civil_de_dias(self.0)
    }

    /// O dia da semana.
    pub const fn dia_da_semana(self) -> DiaDaSemana {
        // 1970-01-01 foi uma quinta-feira.
        let d = (self.0 + 4).rem_euclid(7);
        match d {
            0 => DiaDaSemana::Domingo,
            1 => DiaDaSemana::Segunda,
            2 => DiaDaSemana::Terca,
            3 => DiaDaSemana::Quarta,
            4 => DiaDaSemana::Quinta,
            5 => DiaDaSemana::Sexta,
            _ => DiaDaSemana::Sabado,
        }
    }

    /// Soma (ou subtrai, se negativo) dias.
    #[must_use]
    pub const fn mais_dias(self, dias: i32) -> Self {
        Self(self.0 + dias)
    }

    /// Quantos dias desta data até `outra`. Negativo se `outra` for anterior.
    pub const fn dias_ate(self, outra: Self) -> i32 {
        outra.0 - self.0
    }

    /// Soma meses preservando o dia quando possível. `31/01` mais 1 mês vira `28/02`
    /// (ou `29/02` em ano bissexto) — o comportamento que o mercado espera de vencimento.
    #[must_use]
    pub fn mais_meses(self, meses: i32) -> Self {
        let (ano, mes, dia) = self.ymd();
        let total = i64::from(ano) * 12 + i64::from(mes) - 1 + i64::from(meses);
        #[allow(clippy::cast_possible_truncation)]
        let novo_ano = total.div_euclid(12) as i32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let novo_mes = (total.rem_euclid(12) + 1) as u32;
        let novo_dia = dia.min(dias_no_mes(novo_ano, novo_mes));
        Self(dias_de_civil(novo_ano, novo_mes, novo_dia))
    }

    /// O primeiro dia do mês desta data.
    #[must_use]
    pub fn inicio_do_mes(self) -> Self {
        let (ano, mes, _) = self.ymd();
        Self(dias_de_civil(ano, mes, 1))
    }

    /// O último dia do mês desta data.
    #[must_use]
    pub fn fim_do_mes(self) -> Self {
        let (ano, mes, _) = self.ymd();
        Self(dias_de_civil(ano, mes, dias_no_mes(ano, mes)))
    }

    /// O primeiro dia do ano desta data.
    #[must_use]
    pub fn inicio_do_ano(self) -> Self {
        Self(dias_de_civil(self.ano(), 1, 1))
    }

    /// A competência (ano-mês) desta data.
    pub fn competencia(self) -> Competencia {
        let (ano, mes, _) = self.ymd();
        Competencia::nova(ano, mes)
    }

    /// Formata como `dd/mm/aaaa`.
    pub fn formatar(self) -> String {
        let (ano, mes, dia) = self.ymd();
        format!("{dia:02}/{mes:02}/{ano:04}")
    }

    /// Formata como `dd/mm`, para eixos de gráfico e grades compactas.
    pub fn formatar_curta(self) -> String {
        let (_, mes, dia) = self.ymd();
        format!("{dia:02}/{mes:02}")
    }

    /// Formata em ISO `aaaa-mm-dd`, para arquivos e integrações.
    pub fn formatar_iso(self) -> String {
        let (ano, mes, dia) = self.ymd();
        format!("{ano:04}-{mes:02}-{dia:02}")
    }

    /// Lê uma data. Aceita `dd/mm/aaaa`, `dd/mm/aa`, `ddmmaaaa`, `aaaa-mm-dd` e `dd/mm`
    /// (assumindo o ano de `referencia`).
    ///
    /// # Errors
    /// Devolve [`CodigoErro::DATA_INVALIDA`] se o texto não representar uma data válida.
    pub fn de_str_com_referencia(texto: &str, referencia: Self) -> Resultado<Self> {
        let t = texto.trim();
        let invalida = || {
            Erro::novo(
                CodigoErro::DATA_INVALIDA,
                format!("\"{texto}\" não é uma data válida"),
            )
        };

        // ISO: aaaa-mm-dd
        if t.len() == 10 && t.as_bytes().get(4) == Some(&b'-') {
            let ano = t[0..4].parse().map_err(|_| invalida())?;
            let mes = t[5..7].parse().map_err(|_| invalida())?;
            let dia = t[8..10].parse().map_err(|_| invalida())?;
            return Self::de_ymd(ano, mes, dia);
        }

        let digitos: String = t.chars().filter(char::is_ascii_digit).collect();
        let partes: Vec<&str> = t.split(['/', '.', '-']).filter(|p| !p.is_empty()).collect();

        let (dia, mes, ano) = match (partes.len(), digitos.len()) {
            (3, _) => (
                partes[0].parse::<u32>().map_err(|_| invalida())?,
                partes[1].parse::<u32>().map_err(|_| invalida())?,
                {
                    let a: i32 = partes[2].parse().map_err(|_| invalida())?;
                    if partes[2].len() <= 2 {
                        if a <= 79 {
                            2000 + a
                        } else {
                            1900 + a
                        }
                    } else {
                        a
                    }
                },
            ),
            (2, _) => (
                partes[0].parse::<u32>().map_err(|_| invalida())?,
                partes[1].parse::<u32>().map_err(|_| invalida())?,
                referencia.ano(),
            ),
            (1, 8) => (
                digitos[0..2].parse().map_err(|_| invalida())?,
                digitos[2..4].parse().map_err(|_| invalida())?,
                digitos[4..8].parse().map_err(|_| invalida())?,
            ),
            (1, 6) => (
                digitos[0..2].parse().map_err(|_| invalida())?,
                digitos[2..4].parse().map_err(|_| invalida())?,
                {
                    let a: i32 = digitos[4..6].parse().map_err(|_| invalida())?;
                    if a <= 79 {
                        2000 + a
                    } else {
                        1900 + a
                    }
                },
            ),
            (1, 4) => (
                digitos[0..2].parse().map_err(|_| invalida())?,
                digitos[2..4].parse().map_err(|_| invalida())?,
                referencia.ano(),
            ),
            _ => return Err(invalida()),
        };

        Self::de_ymd(ano, mes, dia)
    }

    /// Lê uma data usando hoje (horário de Brasília) como referência para o ano.
    ///
    /// # Errors
    /// Devolve [`CodigoErro::DATA_INVALIDA`] se o texto não representar uma data válida.
    pub fn de_str(texto: &str) -> Resultado<Self> {
        Self::de_str_com_referencia(texto, Self::hoje(Fuso::BRASILIA))
    }
}

impl fmt::Display for Data {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.formatar())
    }
}

impl fmt::Debug for Data {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Data({})", self.formatar())
    }
}

impl FromStr for Data {
    type Err = Erro;
    fn from_str(s: &str) -> Resultado<Self> {
        Self::de_str(s)
    }
}

/// Hora do dia, em segundos desde a meia-noite.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Hora(u32);

impl Hora {
    /// Meia-noite.
    pub const MEIA_NOITE: Self = Self(0);

    /// Constrói a partir de hora, minuto e segundo.
    ///
    /// # Errors
    /// Devolve [`CodigoErro::DATA_INVALIDA`] se os componentes estiverem fora da faixa.
    pub fn de_hms(hora: u32, minuto: u32, segundo: u32) -> Resultado<Self> {
        if hora > 23 || minuto > 59 || segundo > 59 {
            return Err(Erro::novo(
                CodigoErro::DATA_INVALIDA,
                format!("{hora:02}:{minuto:02}:{segundo:02} não é uma hora válida"),
            ));
        }
        Ok(Self(hora * 3600 + minuto * 60 + segundo))
    }

    /// Segundos desde a meia-noite.
    pub const fn em_segundos(self) -> u32 {
        self.0
    }

    /// A hora, de 0 a 23.
    pub const fn hora(self) -> u32 {
        self.0 / 3600
    }

    /// O minuto, de 0 a 59.
    pub const fn minuto(self) -> u32 {
        (self.0 % 3600) / 60
    }

    /// O segundo, de 0 a 59.
    pub const fn segundo(self) -> u32 {
        self.0 % 60
    }

    /// Formata como `HH:MM`.
    pub fn formatar(self) -> String {
        format!("{:02}:{:02}", self.hora(), self.minuto())
    }

    /// Formata como `HH:MM:SS`.
    pub fn formatar_com_segundos(self) -> String {
        format!(
            "{:02}:{:02}:{:02}",
            self.hora(),
            self.minuto(),
            self.segundo()
        )
    }
}

impl fmt::Display for Hora {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.formatar())
    }
}

impl fmt::Debug for Hora {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Hora({})", self.formatar_com_segundos())
    }
}

/// Um instante no tempo, em microssegundos UTC desde 1970-01-01T00:00:00Z.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Instante(i64);

impl Instante {
    /// A época.
    pub const EPOCA: Self = Self(0);

    /// O instante atual, lido do relógio do sistema.
    ///
    /// Em código de domínio, **não chame isto** — receba o instante pela porta de relógio,
    /// para que os testes possam controlar o tempo.
    pub fn agora() -> Self {
        let d = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        Self(d.as_micros() as i64)
    }

    /// Constrói a partir de microssegundos desde a época.
    pub const fn de_micros(micros: i64) -> Self {
        Self(micros)
    }

    /// Microssegundos desde a época.
    pub const fn em_micros(self) -> i64 {
        self.0
    }

    /// Segundos desde a época.
    pub const fn em_segundos(self) -> i64 {
        self.0.div_euclid(MICROS_POR_SEGUNDO)
    }

    /// A data civil deste instante no fuso informado.
    #[allow(clippy::cast_possible_truncation)]
    pub const fn data(self, fuso: Fuso) -> Data {
        let local = self.0 + fuso.0 as i64 * 60 * MICROS_POR_SEGUNDO;
        Data(local.div_euclid(MICROS_POR_DIA) as i32)
    }

    /// A hora local deste instante no fuso informado.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub const fn hora(self, fuso: Fuso) -> Hora {
        let local = self.0 + fuso.0 as i64 * 60 * MICROS_POR_SEGUNDO;
        let no_dia = local.rem_euclid(MICROS_POR_DIA);
        Hora((no_dia / MICROS_POR_SEGUNDO) as u32)
    }

    /// Compõe um instante a partir de data, hora e fuso.
    pub const fn de_data_hora(data: Data, hora: Hora, fuso: Fuso) -> Self {
        let local = data.0 as i64 * MICROS_POR_DIA + hora.0 as i64 * MICROS_POR_SEGUNDO;
        Self(local - fuso.0 as i64 * 60 * MICROS_POR_SEGUNDO)
    }

    /// Soma microssegundos.
    #[must_use]
    pub const fn mais_micros(self, micros: i64) -> Self {
        Self(self.0 + micros)
    }

    /// Soma segundos.
    #[must_use]
    pub const fn mais_segundos(self, segundos: i64) -> Self {
        Self(self.0 + segundos * MICROS_POR_SEGUNDO)
    }

    /// Quantos microssegundos deste instante até `outro`.
    pub const fn micros_ate(self, outro: Self) -> i64 {
        outro.0 - self.0
    }

    /// Formata como `dd/mm/aaaa HH:MM` no fuso informado.
    pub fn formatar(self, fuso: Fuso) -> String {
        format!(
            "{} {}",
            self.data(fuso).formatar(),
            self.hora(fuso).formatar()
        )
    }

    /// Formata em ISO 8601 UTC, para logs e integrações.
    pub fn formatar_iso(self) -> String {
        let data = self.data(Fuso::UTC);
        let hora = self.hora(Fuso::UTC);
        format!("{}T{}Z", data.formatar_iso(), hora.formatar_com_segundos())
    }
}

impl fmt::Display for Instante {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.formatar(Fuso::BRASILIA))
    }
}

impl fmt::Debug for Instante {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Instante({})", self.formatar_iso())
    }
}

/// Competência contábil: ano e mês, guardados como `AAAAMM`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Competencia(u32);

impl Competencia {
    /// Constrói a partir de ano e mês.
    ///
    /// # Panics
    /// Em `debug`, se o mês estiver fora de 1..=12.
    #[allow(clippy::cast_sign_loss)]
    pub const fn nova(ano: i32, mes: u32) -> Self {
        debug_assert!(mes >= 1 && mes <= 12);
        Self(ano as u32 * 100 + mes)
    }

    /// O ano.
    #[allow(clippy::cast_possible_wrap)]
    pub const fn ano(self) -> i32 {
        (self.0 / 100) as i32
    }

    /// O mês, de 1 a 12.
    pub const fn mes(self) -> u32 {
        self.0 % 100
    }

    /// O valor `AAAAMM`, como guardado no banco.
    pub const fn em_aaaamm(self) -> u32 {
        self.0
    }

    /// A competência seguinte.
    #[must_use]
    pub const fn proxima(self) -> Self {
        if self.mes() == 12 {
            Self::nova(self.ano() + 1, 1)
        } else {
            Self::nova(self.ano(), self.mes() + 1)
        }
    }

    /// A competência anterior.
    #[must_use]
    pub const fn anterior(self) -> Self {
        if self.mes() == 1 {
            Self::nova(self.ano() - 1, 12)
        } else {
            Self::nova(self.ano(), self.mes() - 1)
        }
    }

    /// O primeiro dia da competência.
    pub fn primeiro_dia(self) -> Data {
        Data(dias_de_civil(self.ano(), self.mes(), 1))
    }

    /// O último dia da competência.
    pub fn ultimo_dia(self) -> Data {
        Data(dias_de_civil(
            self.ano(),
            self.mes(),
            dias_no_mes(self.ano(), self.mes()),
        ))
    }

    /// O período que a competência cobre.
    pub fn periodo(self) -> Periodo {
        Periodo {
            de: self.primeiro_dia(),
            ate: self.ultimo_dia(),
        }
    }

    /// Nome do mês por extenso, com o ano: `março de 2026`.
    pub fn formatar_extenso(self) -> String {
        const MESES: [&str; 12] = [
            "janeiro",
            "fevereiro",
            "março",
            "abril",
            "maio",
            "junho",
            "julho",
            "agosto",
            "setembro",
            "outubro",
            "novembro",
            "dezembro",
        ];
        let idx = (self.mes().clamp(1, 12) - 1) as usize;
        format!("{} de {}", MESES[idx], self.ano())
    }

    /// Formata como `mm/aaaa`.
    pub fn formatar(self) -> String {
        format!("{:02}/{:04}", self.mes(), self.ano())
    }
}

impl fmt::Display for Competencia {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.formatar())
    }
}

impl fmt::Debug for Competencia {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Competencia({})", self.formatar())
    }
}

/// Intervalo fechado de datas: `de` e `ate` fazem parte do período.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Periodo {
    /// Primeiro dia, inclusive.
    pub de: Data,
    /// Último dia, inclusive.
    pub ate: Data,
}

impl Periodo {
    /// Constrói um período, normalizando a ordem se vier invertida.
    pub fn novo(de: Data, ate: Data) -> Self {
        if de <= ate {
            Self { de, ate }
        } else {
            Self { de: ate, ate: de }
        }
    }

    /// Período de um único dia.
    pub const fn dia(data: Data) -> Self {
        Self {
            de: data,
            ate: data,
        }
    }

    /// Os últimos `n` dias, terminando em `fim` (inclusive).
    pub fn ultimos_dias(fim: Data, n: i32) -> Self {
        Self {
            de: fim.mais_dias(-(n - 1)),
            ate: fim,
        }
    }

    /// Os próximos `n` dias, começando em `inicio` (inclusive).
    pub fn proximos_dias(inicio: Data, n: i32) -> Self {
        Self {
            de: inicio,
            ate: inicio.mais_dias(n - 1),
        }
    }

    /// Quantidade de dias do período.
    pub const fn dias(self) -> i32 {
        self.de.dias_ate(self.ate) + 1
    }

    /// Verdadeiro se a data está dentro do período.
    pub const fn contem(self, data: Data) -> bool {
        data.0 >= self.de.0 && data.0 <= self.ate.0
    }

    /// Verdadeiro se os dois períodos têm algum dia em comum.
    pub const fn intersecta(self, outro: Self) -> bool {
        self.de.0 <= outro.ate.0 && outro.de.0 <= self.ate.0
    }

    /// Um iterador sobre todos os dias do período.
    pub fn dias_iter(self) -> impl Iterator<Item = Data> {
        (self.de.0..=self.ate.0).map(Data)
    }

    /// Formata como `dd/mm/aaaa a dd/mm/aaaa`.
    pub fn formatar(self) -> String {
        format!("{} a {}", self.de.formatar(), self.ate.formatar())
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn ida_e_volta_de_datas() {
        for (ano, mes, dia) in [
            (1970, 1, 1),
            (2000, 2, 29),
            (2026, 9, 1),
            (2024, 12, 31),
            (1999, 6, 15),
            (2100, 3, 1),
        ] {
            let d = Data::de_ymd(ano, mes, dia).unwrap();
            assert_eq!(
                d.ymd(),
                (ano, mes, dia),
                "falhou em {dia:02}/{mes:02}/{ano}"
            );
        }
    }

    #[test]
    fn epoca_e_dia_da_semana() {
        assert_eq!(Data::EPOCA.ymd(), (1970, 1, 1));
        assert_eq!(Data::EPOCA.dia_da_semana(), DiaDaSemana::Quinta);
        // 01/09/2026 é uma terça-feira.
        assert_eq!(
            Data::de_ymd(2026, 9, 1).unwrap().dia_da_semana(),
            DiaDaSemana::Terca
        );
    }

    #[test]
    fn datas_invalidas_sao_recusadas() {
        assert!(Data::de_ymd(2026, 2, 30).is_err());
        assert!(Data::de_ymd(2026, 13, 1).is_err());
        assert!(Data::de_ymd(2025, 2, 29).is_err());
        assert!(Data::de_ymd(2024, 2, 29).is_ok());
    }

    #[test]
    fn soma_de_meses_preserva_fim_de_mes() {
        let jan31 = Data::de_ymd(2026, 1, 31).unwrap();
        assert_eq!(jan31.mais_meses(1).ymd(), (2026, 2, 28));
        assert_eq!(jan31.mais_meses(3).ymd(), (2026, 4, 30));
        assert_eq!(jan31.mais_meses(12).ymd(), (2027, 1, 31));
        assert_eq!(jan31.mais_meses(-1).ymd(), (2025, 12, 31));
        let jan31_bissexto = Data::de_ymd(2024, 1, 31).unwrap();
        assert_eq!(jan31_bissexto.mais_meses(1).ymd(), (2024, 2, 29));
    }

    #[test]
    fn leitura_de_datas_no_padrao_brasileiro() {
        let ref_ = Data::de_ymd(2026, 9, 1).unwrap();
        let casos = [
            ("10/03/2026", (2026, 3, 10)),
            ("10/03/26", (2026, 3, 10)),
            ("10/03", (2026, 3, 10)),
            ("10032026", (2026, 3, 10)),
            ("100326", (2026, 3, 10)),
            ("1003", (2026, 3, 10)),
            ("2026-03-10", (2026, 3, 10)),
            ("10.03.2026", (2026, 3, 10)),
        ];
        for (texto, esperado) in casos {
            assert_eq!(
                Data::de_str_com_referencia(texto, ref_).unwrap().ymd(),
                esperado,
                "falhou ao ler {texto:?}"
            );
        }
        assert!(Data::de_str_com_referencia("32/13/2026", ref_).is_err());
    }

    #[test]
    fn inicio_e_fim_de_mes() {
        let d = Data::de_ymd(2026, 2, 14).unwrap();
        assert_eq!(d.inicio_do_mes().ymd(), (2026, 2, 1));
        assert_eq!(d.fim_do_mes().ymd(), (2026, 2, 28));
    }

    #[test]
    fn competencia() {
        let c = Competencia::nova(2026, 3);
        assert_eq!(c.em_aaaamm(), 202_603);
        assert_eq!(c.proxima(), Competencia::nova(2026, 4));
        assert_eq!(
            Competencia::nova(2026, 12).proxima(),
            Competencia::nova(2027, 1)
        );
        assert_eq!(
            Competencia::nova(2026, 1).anterior(),
            Competencia::nova(2025, 12)
        );
        assert_eq!(c.formatar_extenso(), "março de 2026");
        assert_eq!(c.periodo().dias(), 31);
    }

    #[test]
    fn instante_e_fuso() {
        let data = Data::de_ymd(2026, 9, 1).unwrap();
        let hora = Hora::de_hms(23, 30, 0).unwrap();
        let i = Instante::de_data_hora(data, hora, Fuso::BRASILIA);
        // 23:30 em Brasília é 02:30 do dia seguinte em UTC.
        assert_eq!(i.data(Fuso::UTC).ymd(), (2026, 9, 2));
        assert_eq!(i.hora(Fuso::UTC).hora(), 2);
        // E volta corretamente.
        assert_eq!(i.data(Fuso::BRASILIA), data);
        assert_eq!(i.hora(Fuso::BRASILIA), hora);
    }

    #[test]
    fn periodos() {
        let p = Periodo::novo(
            Data::de_ymd(2026, 9, 1).unwrap(),
            Data::de_ymd(2026, 9, 30).unwrap(),
        );
        assert_eq!(p.dias(), 30);
        assert!(p.contem(Data::de_ymd(2026, 9, 15).unwrap()));
        assert!(!p.contem(Data::de_ymd(2026, 10, 1).unwrap()));
        assert_eq!(p.dias_iter().count(), 30);

        let outro = Periodo::novo(
            Data::de_ymd(2026, 9, 25).unwrap(),
            Data::de_ymd(2026, 10, 5).unwrap(),
        );
        assert!(p.intersecta(outro));
    }
}
