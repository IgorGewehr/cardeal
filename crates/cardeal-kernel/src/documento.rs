//! Documentos brasileiros: CPF, CNPJ, inscrição estadual e unidade federativa.
//!
//! Os tipos aqui são **validados na construção**. Se você tem um [`Cpf`] em mãos, os dígitos
//! verificadores já conferem — não é preciso revalidar em camada nenhuma.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::erro::{CodigoErro, Erro, Resultado};

/// Extrai apenas os dígitos de um texto.
fn digitos(texto: &str) -> Vec<u8> {
    texto
        .bytes()
        .filter(u8::is_ascii_digit)
        .map(|b| b - b'0')
        .collect()
}

/// CPF válido. Guardado como 11 dígitos.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Cpf([u8; 11]);

impl Cpf {
    /// Valida e constrói. Aceita com ou sem máscara.
    ///
    /// # Errors
    /// Devolve [`CodigoErro::DOCUMENTO_INVALIDO`] se o CPF não tiver 11 dígitos, se todos
    /// forem iguais, ou se os dígitos verificadores não conferirem.
    pub fn novo(texto: &str) -> Resultado<Self> {
        let d = digitos(texto);
        if d.len() != 11 {
            return Err(Erro::novo(
                CodigoErro::DOCUMENTO_INVALIDO,
                format!("CPF deve ter 11 dígitos (recebido: {})", d.len()),
            ));
        }
        if d.iter().all(|x| *x == d[0]) {
            return Err(Erro::novo(
                CodigoErro::DOCUMENTO_INVALIDO,
                "CPF com todos os dígitos iguais é inválido",
            ));
        }

        for pos in [9usize, 10usize] {
            let soma: u32 = (0..pos)
                .map(|i| u32::from(d[i]) * u32::from(u8::try_from(pos + 1 - i).unwrap_or(0)))
                .sum();
            let resto = (soma * 10) % 11;
            let esperado = if resto == 10 { 0 } else { resto };
            if u32::from(d[pos]) != esperado {
                return Err(Erro::novo(
                    CodigoErro::DOCUMENTO_INVALIDO,
                    "CPF inválido: dígito verificador não confere",
                ));
            }
        }

        let mut bytes = [0u8; 11];
        bytes.copy_from_slice(&d);
        Ok(Self(bytes))
    }

    /// Os 11 dígitos sem máscara: `12345678909`.
    #[must_use]
    pub fn sem_mascara(&self) -> String {
        self.0.iter().map(|d| char::from(b'0' + d)).collect()
    }

    /// Formatado: `123.456.789-09`.
    #[must_use]
    pub fn formatar(&self) -> String {
        let s = self.sem_mascara();
        format!("{}.{}.{}-{}", &s[0..3], &s[3..6], &s[6..9], &s[9..11])
    }

    /// Mascarado para exibição pública e log: `123.***.**9-09`.
    ///
    /// Usado em qualquer lugar em que o CPF apareça fora do cadastro — a LGPD manda minimizar,
    /// e o logger do projeto usa esta forma. Ver `docs/08-seguranca-permissoes.md` §6.
    #[must_use]
    pub fn mascarado(&self) -> String {
        let s = self.sem_mascara();
        format!("{}.***.**{}-{}", &s[0..3], &s[8..9], &s[9..11])
    }
}

impl fmt::Display for Cpf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.formatar())
    }
}

impl fmt::Debug for Cpf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Debug nunca expõe o documento inteiro — vai parar em log.
        write!(f, "Cpf({})", self.mascarado())
    }
}

impl FromStr for Cpf {
    type Err = Erro;
    fn from_str(s: &str) -> Resultado<Self> {
        Self::novo(s)
    }
}

/// CNPJ válido. Guardado como 14 dígitos.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Cnpj([u8; 14]);

impl Cnpj {
    const PESOS_1: [u32; 12] = [5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2];
    const PESOS_2: [u32; 13] = [6, 5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2];

    /// Valida e constrói. Aceita com ou sem máscara.
    ///
    /// # Errors
    /// Devolve [`CodigoErro::DOCUMENTO_INVALIDO`] se o CNPJ não tiver 14 dígitos, se todos
    /// forem iguais, ou se os dígitos verificadores não conferirem.
    pub fn novo(texto: &str) -> Resultado<Self> {
        let d = digitos(texto);
        if d.len() != 14 {
            return Err(Erro::novo(
                CodigoErro::DOCUMENTO_INVALIDO,
                format!("CNPJ deve ter 14 dígitos (recebido: {})", d.len()),
            ));
        }
        if d.iter().all(|x| *x == d[0]) {
            return Err(Erro::novo(
                CodigoErro::DOCUMENTO_INVALIDO,
                "CNPJ com todos os dígitos iguais é inválido",
            ));
        }

        let verificador = |pesos: &[u32], ate: usize| -> u8 {
            let soma: u32 = (0..ate).map(|i| u32::from(d[i]) * pesos[i]).sum();
            let resto = soma % 11;
            u8::try_from(if resto < 2 { 0 } else { 11 - resto }).unwrap_or(0)
        };

        if d[12] != verificador(&Self::PESOS_1, 12) || d[13] != verificador(&Self::PESOS_2, 13) {
            return Err(Erro::novo(
                CodigoErro::DOCUMENTO_INVALIDO,
                "CNPJ inválido: dígito verificador não confere",
            ));
        }

        let mut bytes = [0u8; 14];
        bytes.copy_from_slice(&d);
        Ok(Self(bytes))
    }

    /// Os 14 dígitos sem máscara.
    #[must_use]
    pub fn sem_mascara(&self) -> String {
        self.0.iter().map(|d| char::from(b'0' + d)).collect()
    }

    /// Formatado: `12.345.678/0001-95`.
    #[must_use]
    pub fn formatar(&self) -> String {
        let s = self.sem_mascara();
        format!(
            "{}.{}.{}/{}-{}",
            &s[0..2],
            &s[2..5],
            &s[5..8],
            &s[8..12],
            &s[12..14]
        )
    }

    /// A raiz (os 8 primeiros dígitos), que identifica o grupo empresarial.
    /// Matriz e filiais compartilham a raiz.
    #[must_use]
    pub fn raiz(&self) -> String {
        self.sem_mascara()[0..8].to_string()
    }

    /// O número da ordem do estabelecimento: `0001` é a matriz.
    #[must_use]
    pub fn ordem(&self) -> String {
        self.sem_mascara()[8..12].to_string()
    }

    /// Verdadeiro se for a matriz (ordem `0001`).
    #[must_use]
    pub fn e_matriz(&self) -> bool {
        self.ordem() == "0001"
    }
}

impl fmt::Display for Cnpj {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.formatar())
    }
}

impl fmt::Debug for Cnpj {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Cnpj({})", self.formatar())
    }
}

impl FromStr for Cnpj {
    type Err = Erro;
    fn from_str(s: &str) -> Resultado<Self> {
        Self::novo(s)
    }
}

/// Documento de identificação de uma pessoa física ou jurídica.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "tipo", content = "valor")]
pub enum Documento {
    /// Pessoa física.
    Cpf(Cpf),
    /// Pessoa jurídica.
    Cnpj(Cnpj),
}

impl Documento {
    /// Detecta o tipo pela quantidade de dígitos e valida.
    ///
    /// # Errors
    /// Devolve [`CodigoErro::DOCUMENTO_INVALIDO`] se não for um CPF nem um CNPJ válido.
    pub fn novo(texto: &str) -> Resultado<Self> {
        match digitos(texto).len() {
            11 => Cpf::novo(texto).map(Self::Cpf),
            14 => Cnpj::novo(texto).map(Self::Cnpj),
            n => Err(Erro::novo(
                CodigoErro::DOCUMENTO_INVALIDO,
                format!("Documento deve ter 11 (CPF) ou 14 (CNPJ) dígitos; recebido: {n}"),
            )),
        }
    }

    /// Verdadeiro se for pessoa física.
    #[must_use]
    pub const fn e_fisica(&self) -> bool {
        matches!(self, Self::Cpf(_))
    }

    /// Verdadeiro se for pessoa jurídica.
    #[must_use]
    pub const fn e_juridica(&self) -> bool {
        matches!(self, Self::Cnpj(_))
    }

    /// Os dígitos sem máscara. É a forma canônica para chave única e deduplicação.
    #[must_use]
    pub fn sem_mascara(&self) -> String {
        match self {
            Self::Cpf(c) => c.sem_mascara(),
            Self::Cnpj(c) => c.sem_mascara(),
        }
    }

    /// A forma mascarada, apropriada para exibição e log.
    #[must_use]
    pub fn mascarado(&self) -> String {
        match self {
            Self::Cpf(c) => c.mascarado(),
            Self::Cnpj(c) => c.formatar(),
        }
    }
}

impl fmt::Display for Documento {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cpf(c) => fmt::Display::fmt(c, f),
            Self::Cnpj(c) => fmt::Display::fmt(c, f),
        }
    }
}

impl fmt::Debug for Documento {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Documento({})", self.mascarado())
    }
}

impl FromStr for Documento {
    type Err = Erro;
    fn from_str(s: &str) -> Resultado<Self> {
        Self::novo(s)
    }
}

/// Unidade federativa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[allow(missing_docs)]
pub enum Uf {
    Ac,
    Al,
    Am,
    Ap,
    Ba,
    Ce,
    Df,
    Es,
    Go,
    Ma,
    Mg,
    Ms,
    Mt,
    Pa,
    Pb,
    Pe,
    Pi,
    Pr,
    Rj,
    Rn,
    Ro,
    Rr,
    Rs,
    Sc,
    Se,
    Sp,
    To,
}

impl Uf {
    /// Todas as unidades federativas, em ordem alfabética de sigla.
    pub const TODAS: [Self; 27] = [
        Self::Ac,
        Self::Al,
        Self::Am,
        Self::Ap,
        Self::Ba,
        Self::Ce,
        Self::Df,
        Self::Es,
        Self::Go,
        Self::Ma,
        Self::Mg,
        Self::Ms,
        Self::Mt,
        Self::Pa,
        Self::Pb,
        Self::Pe,
        Self::Pi,
        Self::Pr,
        Self::Rj,
        Self::Rn,
        Self::Ro,
        Self::Rr,
        Self::Rs,
        Self::Sc,
        Self::Se,
        Self::Sp,
        Self::To,
    ];

    /// A sigla de duas letras maiúsculas.
    #[must_use]
    pub const fn sigla(self) -> &'static str {
        match self {
            Self::Ac => "AC",
            Self::Al => "AL",
            Self::Am => "AM",
            Self::Ap => "AP",
            Self::Ba => "BA",
            Self::Ce => "CE",
            Self::Df => "DF",
            Self::Es => "ES",
            Self::Go => "GO",
            Self::Ma => "MA",
            Self::Mg => "MG",
            Self::Ms => "MS",
            Self::Mt => "MT",
            Self::Pa => "PA",
            Self::Pb => "PB",
            Self::Pe => "PE",
            Self::Pi => "PI",
            Self::Pr => "PR",
            Self::Rj => "RJ",
            Self::Rn => "RN",
            Self::Ro => "RO",
            Self::Rr => "RR",
            Self::Rs => "RS",
            Self::Sc => "SC",
            Self::Se => "SE",
            Self::Sp => "SP",
            Self::To => "TO",
        }
    }

    /// O código IBGE de dois dígitos, usado na chave de acesso da NF-e.
    #[must_use]
    pub const fn codigo_ibge(self) -> u8 {
        match self {
            Self::Ro => 11,
            Self::Ac => 12,
            Self::Am => 13,
            Self::Rr => 14,
            Self::Pa => 15,
            Self::Ap => 16,
            Self::To => 17,
            Self::Ma => 21,
            Self::Pi => 22,
            Self::Ce => 23,
            Self::Rn => 24,
            Self::Pb => 25,
            Self::Pe => 26,
            Self::Al => 27,
            Self::Se => 28,
            Self::Ba => 29,
            Self::Mg => 31,
            Self::Es => 32,
            Self::Rj => 33,
            Self::Sp => 35,
            Self::Pr => 41,
            Self::Sc => 42,
            Self::Rs => 43,
            Self::Ms => 50,
            Self::Mt => 51,
            Self::Go => 52,
            Self::Df => 53,
        }
    }

    /// O nome por extenso.
    #[must_use]
    pub const fn nome(self) -> &'static str {
        match self {
            Self::Ac => "Acre",
            Self::Al => "Alagoas",
            Self::Am => "Amazonas",
            Self::Ap => "Amapá",
            Self::Ba => "Bahia",
            Self::Ce => "Ceará",
            Self::Df => "Distrito Federal",
            Self::Es => "Espírito Santo",
            Self::Go => "Goiás",
            Self::Ma => "Maranhão",
            Self::Mg => "Minas Gerais",
            Self::Ms => "Mato Grosso do Sul",
            Self::Mt => "Mato Grosso",
            Self::Pa => "Pará",
            Self::Pb => "Paraíba",
            Self::Pe => "Pernambuco",
            Self::Pi => "Piauí",
            Self::Pr => "Paraná",
            Self::Rj => "Rio de Janeiro",
            Self::Rn => "Rio Grande do Norte",
            Self::Ro => "Rondônia",
            Self::Rr => "Roraima",
            Self::Rs => "Rio Grande do Sul",
            Self::Sc => "Santa Catarina",
            Self::Se => "Sergipe",
            Self::Sp => "São Paulo",
            Self::To => "Tocantins",
        }
    }

    /// O fuso horário predominante da unidade federativa.
    #[must_use]
    pub const fn fuso(self) -> crate::tempo::Fuso {
        use crate::tempo::Fuso;
        match self {
            Self::Ac => Fuso::ACRE,
            Self::Am | Self::Rr | Self::Ro | Self::Mt | Self::Ms => Fuso::AMAZONAS,
            _ => Fuso::BRASILIA,
        }
    }

    /// Lê a partir da sigla, ignorando maiúsculas e minúsculas.
    ///
    /// # Errors
    /// Devolve [`CodigoErro::ENTRADA_INVALIDA`] se a sigla não existir.
    pub fn de_sigla(sigla: &str) -> Resultado<Self> {
        let s = sigla.trim().to_ascii_uppercase();
        Self::TODAS
            .into_iter()
            .find(|uf| uf.sigla() == s)
            .ok_or_else(|| {
                Erro::novo(
                    CodigoErro::ENTRADA_INVALIDA,
                    format!("\"{sigla}\" não é uma UF válida"),
                )
            })
    }

    /// Lê a partir do código IBGE.
    ///
    /// # Errors
    /// Devolve [`CodigoErro::ENTRADA_INVALIDA`] se o código não existir.
    pub fn de_codigo_ibge(codigo: u8) -> Resultado<Self> {
        Self::TODAS
            .into_iter()
            .find(|uf| uf.codigo_ibge() == codigo)
            .ok_or_else(|| {
                Erro::novo(
                    CodigoErro::ENTRADA_INVALIDA,
                    format!("{codigo} não é um código de UF válido"),
                )
            })
    }
}

impl fmt::Display for Uf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.sigla())
    }
}

impl FromStr for Uf {
    type Err = Erro;
    fn from_str(s: &str) -> Resultado<Self> {
        Self::de_sigla(s)
    }
}

/// Inscrição estadual.
///
/// A validação completa de dígito verificador varia por UF (são 27 algoritmos distintos) e
/// é delegada à API fiscal, que também confere a situação cadastral no SINTEGRA. Aqui
/// garantimos apenas forma e comprimento — o suficiente para a interface dar retorno
/// imediato ao usuário sem depender de rede.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InscricaoEstadual {
    /// Contribuinte isento (o campo vem como `ISENTO` na NF-e).
    Isento,
    /// Não contribuinte do ICMS.
    NaoContribuinte,
    /// Inscrição normal, com os dígitos e a UF de emissão.
    Numero {
        /// Somente dígitos.
        digitos: String,
        /// A UF que emitiu.
        uf: Uf,
    },
}

impl InscricaoEstadual {
    /// Constrói a partir do texto informado pelo usuário.
    ///
    /// # Errors
    /// Devolve [`CodigoErro::DOCUMENTO_INVALIDO`] se o comprimento for implausível.
    pub fn nova(texto: &str, uf: Uf) -> Resultado<Self> {
        let t = texto.trim();
        let normalizado = t.to_ascii_uppercase();
        if normalizado.is_empty() || normalizado == "ISENTO" || normalizado == "ISENTA" {
            return Ok(Self::Isento);
        }
        let d: String = t.chars().filter(char::is_ascii_digit).collect();
        if !(2..=14).contains(&d.len()) {
            return Err(Erro::novo(
                CodigoErro::DOCUMENTO_INVALIDO,
                format!("Inscrição estadual com {} dígitos é implausível", d.len()),
            ));
        }
        Ok(Self::Numero { digitos: d, uf })
    }

    /// A forma que vai no XML da nota.
    #[must_use]
    pub fn para_nfe(&self) -> String {
        match self {
            Self::Isento | Self::NaoContribuinte => "ISENTO".to_string(),
            Self::Numero { digitos, .. } => digitos.clone(),
        }
    }

    /// Verdadeiro se a pessoa é contribuinte do ICMS — determina o CFOP e o DIFAL.
    #[must_use]
    pub const fn e_contribuinte(&self) -> bool {
        matches!(self, Self::Numero { .. })
    }
}

impl fmt::Display for InscricaoEstadual {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Isento => f.write_str("ISENTO"),
            Self::NaoContribuinte => f.write_str("NÃO CONTRIBUINTE"),
            Self::Numero { digitos, uf } => write!(f, "{digitos} ({uf})"),
        }
    }
}

impl fmt::Debug for InscricaoEstadual {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "InscricaoEstadual({self})")
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn cpf_valido() {
        // CPFs sintéticos válidos.
        for texto in ["529.982.247-25", "52998224725", "111.444.777-35"] {
            let cpf = Cpf::novo(texto).unwrap();
            assert_eq!(cpf.sem_mascara().len(), 11);
        }
        assert_eq!(
            Cpf::novo("52998224725").unwrap().formatar(),
            "529.982.247-25"
        );
    }

    #[test]
    fn cpf_invalido() {
        assert!(Cpf::novo("529.982.247-26").is_err()); // dígito errado
        assert!(Cpf::novo("111.111.111-11").is_err()); // todos iguais
        assert!(Cpf::novo("123").is_err()); // curto
        assert!(Cpf::novo("").is_err());
    }

    #[test]
    fn cpf_nunca_vaza_inteiro_em_debug() {
        let cpf = Cpf::novo("52998224725").unwrap();
        let d = format!("{cpf:?}");
        assert!(
            !d.contains("982"),
            "Debug de CPF não pode expor o miolo: {d}"
        );
        assert!(d.contains("529"));
    }

    #[test]
    fn cnpj_valido() {
        let cnpj = Cnpj::novo("11.222.333/0001-81").unwrap();
        assert_eq!(cnpj.formatar(), "11.222.333/0001-81");
        assert_eq!(cnpj.raiz(), "11222333");
        assert_eq!(cnpj.ordem(), "0001");
        assert!(cnpj.e_matriz());
    }

    #[test]
    fn cnpj_invalido() {
        assert!(Cnpj::novo("11.222.333/0001-82").is_err());
        assert!(Cnpj::novo("00.000.000/0000-00").is_err());
        assert!(Cnpj::novo("112223330001").is_err());
    }

    #[test]
    fn documento_detecta_o_tipo() {
        assert!(Documento::novo("52998224725").unwrap().e_fisica());
        assert!(Documento::novo("11.222.333/0001-81").unwrap().e_juridica());
        assert!(Documento::novo("123456").is_err());
    }

    #[test]
    fn ufs() {
        assert_eq!(Uf::Sp.codigo_ibge(), 35);
        assert_eq!(Uf::de_sigla("mg").unwrap(), Uf::Mg);
        assert_eq!(Uf::de_codigo_ibge(43).unwrap(), Uf::Rs);
        assert!(Uf::de_sigla("XX").is_err());
        assert_eq!(Uf::Ac.fuso(), crate::tempo::Fuso::ACRE);
        assert_eq!(Uf::Sp.fuso(), crate::tempo::Fuso::BRASILIA);
        // Códigos IBGE são únicos.
        let mut codigos: Vec<u8> = Uf::TODAS.iter().map(|u| u.codigo_ibge()).collect();
        codigos.sort_unstable();
        codigos.dedup();
        assert_eq!(codigos.len(), 27);
    }

    #[test]
    fn inscricao_estadual() {
        assert!(matches!(
            InscricaoEstadual::nova("ISENTO", Uf::Sp).unwrap(),
            InscricaoEstadual::Isento
        ));
        assert!(matches!(
            InscricaoEstadual::nova("", Uf::Sp).unwrap(),
            InscricaoEstadual::Isento
        ));
        let ie = InscricaoEstadual::nova("110.042.490.114", Uf::Sp).unwrap();
        assert!(ie.e_contribuinte());
        assert_eq!(ie.para_nfe(), "110042490114");
    }
}
