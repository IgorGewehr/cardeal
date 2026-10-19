//! Um leitor mínimo de ASN.1 (BER/DER) — só o necessário para abrir um PKCS#12 e ler um
//! X.509 em [`crate::certificado`]. Aceita o que exportadores reais produzem além do DER
//! estrito: comprimento indefinido e `OCTET STRING` construída (em pedaços).

use std::borrow::Cow;

/// Uma falha de formato — o arquivo não é o ASN.1 que se esperava.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FormatoInvalido(pub String);

pub(crate) type Res<T> = Result<T, FormatoInvalido>;

pub(crate) fn invalido<T>(motivo: impl Into<String>) -> Res<T> {
    Err(FormatoInvalido(motivo.into()))
}

/// As tags usadas aqui (classe + construída + número, num byte só).
pub(crate) mod tag {
    pub const BOOLEANO: u8 = 0x01;
    pub const INTEIRO: u8 = 0x02;
    pub const OCTETOS: u8 = 0x04;
    pub const OID: u8 = 0x06;
    pub const UTF8: u8 = 0x0C;
    pub const IMPRIMIVEL: u8 = 0x13;
    pub const T61: u8 = 0x14;
    pub const IA5: u8 = 0x16;
    pub const UTC: u8 = 0x17;
    pub const GENERALIZADO: u8 = 0x18;
    pub const BMP: u8 = 0x1E;
    pub const SEQUENCIA: u8 = 0x30;
    /// `[0]` construído — o `EXPLICIT` mais comum.
    pub const CONTEXTO_0: u8 = 0xA0;
    /// `[3]` construído — as extensões do X.509.
    pub const CONTEXTO_3: u8 = 0xA3;
}

/// Profundidade máxima de aninhamento — um arquivo malicioso não estoura a pilha.
const PROFUNDIDADE_MAXIMA: usize = 64;

/// Um elemento lido: a tag, o conteúdo (para comprimento indefinido, os filhos sem o
/// marcador de fim) e os bytes do elemento inteiro, como vieram.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Tlv<'a> {
    pub tag: u8,
    pub valor: &'a [u8],
    pub bruto: &'a [u8],
}

impl<'a> Tlv<'a> {
    /// Lê o primeiro elemento de `entrada`; devolve o elemento e o que sobra.
    pub fn ler(entrada: &'a [u8]) -> Res<(Self, &'a [u8])> {
        ler(entrada, 0)
    }

    /// Lê um elemento que ocupa `entrada` inteira (sobras são toleradas — alguns
    /// exportadores completam com zeros).
    pub fn unico(entrada: &'a [u8]) -> Res<Self> {
        Self::ler(entrada).map(|(t, _)| t)
    }

    /// Confere a tag.
    pub fn exigir(self, esperada: u8, o_que: &str) -> Res<Self> {
        if self.tag == esperada {
            Ok(self)
        } else {
            invalido(format!(
                "{o_que}: esperava a tag {esperada:#04x}, veio {:#04x}",
                self.tag
            ))
        }
    }

    pub const fn construido(self) -> bool {
        self.tag & 0x20 != 0
    }

    /// Os filhos de um elemento construído, em ordem.
    pub fn filhos(self) -> Res<Vec<Tlv<'a>>> {
        let mut resto = self.valor;
        let mut filhos = Vec::new();
        while !resto.is_empty() {
            let (filho, depois) = Self::ler(resto)?;
            filhos.push(filho);
            resto = depois;
        }
        Ok(filhos)
    }

    /// O conteúdo de uma `OCTET STRING` (ou de um `[n] IMPLICIT OCTET STRING`) — junta os
    /// pedaços quando ela vem construída.
    pub fn octetos(self) -> Res<Cow<'a, [u8]>> {
        if !self.construido() {
            return Ok(Cow::Borrowed(self.valor));
        }
        let mut junto = Vec::new();
        for pedaco in self.filhos()? {
            junto.extend_from_slice(&pedaco.octetos()?);
        }
        Ok(Cow::Owned(junto))
    }

    /// Um `OBJECT IDENTIFIER` em notação de pontos ("1.2.840.113549.1.7.1").
    pub fn oid(self) -> Res<String> {
        let t = self.exigir(tag::OID, "OID")?;
        let Some((&primeiro, resto)) = t.valor.split_first() else {
            return invalido("OID vazio");
        };
        let mut partes = vec![u64::from(primeiro / 40), u64::from(primeiro % 40)];
        let mut acumulado: u64 = 0;
        for &b in resto {
            if acumulado > (u64::MAX >> 7) {
                return invalido("OID longo demais");
            }
            acumulado = (acumulado << 7) | u64::from(b & 0x7F);
            if b & 0x80 == 0 {
                partes.push(acumulado);
                acumulado = 0;
            }
        }
        Ok(partes
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join("."))
    }

    /// Um `INTEGER` pequeno, não negativo (contagem de iterações, versão).
    pub fn inteiro(self) -> Res<u64> {
        let t = self.exigir(tag::INTEIRO, "inteiro")?;
        let bytes = sem_zeros_a_esquerda(t.valor);
        if bytes.len() > 8 || t.valor.first().is_some_and(|b| b & 0x80 != 0) {
            return invalido("inteiro fora da faixa");
        }
        Ok(bytes.iter().fold(0, |a, &b| (a << 8) | u64::from(b)))
    }

    /// Um texto de qualquer dos tipos de string do X.509.
    pub fn texto(self) -> Res<String> {
        Ok(match self.tag {
            tag::UTF8 | tag::IMPRIMIVEL | tag::IA5 => {
                String::from_utf8_lossy(self.valor).into_owned()
            }
            // T.61 na prática é Latin-1.
            tag::T61 => self.valor.iter().map(|&b| char::from(b)).collect(),
            tag::BMP => {
                let unidades: Vec<u16> = self
                    .valor
                    .chunks_exact(2)
                    .map(|p| u16::from_be_bytes([p[0], p[1]]))
                    .collect();
                String::from_utf16_lossy(&unidades)
            }
            tag::OCTETOS => String::from_utf8_lossy(&self.octetos()?).into_owned(),
            outra => return invalido(format!("tipo de texto não suportado: {outra:#04x}")),
        })
    }
}

/// Um `INTEGER` sem o zero de sinal à esquerda.
pub(crate) fn sem_zeros_a_esquerda(bytes: &[u8]) -> &[u8] {
    let inicio = bytes.iter().position(|&b| b != 0).unwrap_or(bytes.len());
    &bytes[inicio..]
}

fn ler(entrada: &[u8], profundidade: usize) -> Res<(Tlv<'_>, &[u8])> {
    if profundidade > PROFUNDIDADE_MAXIMA {
        return invalido("aninhamento profundo demais");
    }
    let [tag_lida, primeiro, resto @ ..] = entrada else {
        return invalido("fim inesperado do arquivo");
    };
    if tag_lida & 0x1F == 0x1F {
        return invalido("tag de número alto não suportada");
    }
    if *primeiro == 0x80 {
        // Comprimento indefinido: os filhos vão até o marcador de fim (00 00).
        if tag_lida & 0x20 == 0 {
            return invalido("comprimento indefinido num elemento primitivo");
        }
        let mut cursor = resto;
        loop {
            if let [0, 0, depois @ ..] = cursor {
                let usado = resto.len() - cursor.len();
                let total = entrada.len() - depois.len();
                return Ok((
                    Tlv {
                        tag: *tag_lida,
                        valor: &resto[..usado],
                        bruto: &entrada[..total],
                    },
                    depois,
                ));
            }
            let (_, depois) = ler(cursor, profundidade + 1)?;
            cursor = depois;
        }
    }
    let (tamanho, resto) = if primeiro & 0x80 == 0 {
        (usize::from(*primeiro), resto)
    } else {
        let n = usize::from(primeiro & 0x7F);
        if n > 4 || resto.len() < n {
            return invalido("comprimento inválido");
        }
        let tamanho = resto[..n]
            .iter()
            .fold(0usize, |a, &b| (a << 8) | usize::from(b));
        (tamanho, &resto[n..])
    };
    if resto.len() < tamanho {
        return invalido("elemento maior que o arquivo");
    }
    let (valor, depois) = resto.split_at(tamanho);
    let total = entrada.len() - depois.len();
    Ok((
        Tlv {
            tag: *tag_lida,
            valor,
            bruto: &entrada[..total],
        },
        depois,
    ))
}

/// Escreve um elemento DER.
pub(crate) fn elemento(tag: u8, conteudo: &[u8]) -> Vec<u8> {
    let n = conteudo.len();
    let mut saida = Vec::with_capacity(n + 6);
    saida.push(tag);
    if n < 0x80 {
        saida.push(u8::try_from(n).unwrap_or(0));
    } else {
        let bytes = n.to_be_bytes();
        let significativos = sem_zeros_a_esquerda(&bytes);
        saida.push(0x80 | u8::try_from(significativos.len()).unwrap_or(0));
        saida.extend_from_slice(significativos);
    }
    saida.extend_from_slice(conteudo);
    saida
}

/// Uma `SEQUENCE` com as partes já codificadas.
pub(crate) fn sequencia(partes: &[&[u8]]) -> Vec<u8> {
    elemento(tag::SEQUENCIA, &partes.concat())
}

/// Um `OBJECT IDENTIFIER` a partir da notação de pontos.
pub(crate) fn oid_der(pontos: &str) -> Vec<u8> {
    let numeros: Vec<u64> = pontos.split('.').filter_map(|p| p.parse().ok()).collect();
    let mut conteudo = vec![u8::try_from(numeros[0] * 40 + numeros[1]).unwrap_or(0)];
    for &n in &numeros[2..] {
        let mut grupos = vec![u8::try_from(n & 0x7F).unwrap_or(0)];
        let mut resto = n >> 7;
        while resto > 0 {
            grupos.push(0x80 | u8::try_from(resto & 0x7F).unwrap_or(0));
            resto >>= 7;
        }
        conteudo.extend(grupos.iter().rev());
    }
    elemento(tag::OID, &conteudo)
}

/// Um `INTEGER` não negativo.
pub(crate) fn inteiro_der(n: u64) -> Vec<u8> {
    let bytes = n.to_be_bytes();
    let mut conteudo = sem_zeros_a_esquerda(&bytes).to_vec();
    if conteudo.first().is_none_or(|b| b & 0x80 != 0) {
        conteudo.insert(0, 0);
    }
    elemento(tag::INTEIRO, &conteudo)
}

/// Uma `OCTET STRING`.
pub(crate) fn octetos_der(bytes: &[u8]) -> Vec<u8> {
    elemento(tag::OCTETOS, bytes)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_oid_e_inteiro() {
        // 1.2.840.113549.1.7.1 (pkcs7-data)
        let der = [
            0x06, 0x09, 0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x01,
        ];
        assert_eq!(
            Tlv::unico(&der).unwrap().oid().unwrap(),
            "1.2.840.113549.1.7.1"
        );
        let der = [0x02, 0x02, 0x08, 0x00];
        assert_eq!(Tlv::unico(&der).unwrap().inteiro().unwrap(), 2048);
    }

    #[test]
    fn comprimento_indefinido_e_octetos_em_pedacos() {
        // SEQUENCE (indefinida) { OCTET STRING construída { "ab", "c" } } 00 00
        let ber = [
            0x30, 0x80, 0x24, 0x80, 0x04, 0x02, b'a', b'b', 0x04, 0x01, b'c', 0, 0, 0, 0,
        ];
        let (seq, resto) = Tlv::ler(&ber).unwrap();
        assert!(resto.is_empty());
        let filhos = seq.filhos().unwrap();
        assert_eq!(filhos.len(), 1);
        assert_eq!(&*filhos[0].octetos().unwrap(), b"abc");
    }

    #[test]
    fn escreve_o_que_le() {
        let oid = oid_der("1.2.840.113549.1.12.10.1.3");
        assert_eq!(
            Tlv::unico(&oid).unwrap().oid().unwrap(),
            "1.2.840.113549.1.12.10.1.3"
        );
        for n in [0u64, 1, 127, 128, 255, 2048, 51_200] {
            assert_eq!(Tlv::unico(&inteiro_der(n)).unwrap().inteiro().unwrap(), n);
        }
        let grande = vec![7u8; 70_000];
        let seq = sequencia(&[&octetos_der(&grande), &inteiro_der(3)]);
        let t = Tlv::unico(&seq).unwrap();
        assert_eq!(t.bruto.len(), seq.len());
        let filhos = t.filhos().unwrap();
        assert_eq!(&*filhos[0].octetos().unwrap(), grande.as_slice());
        assert_eq!(filhos[1].inteiro().unwrap(), 3);
    }

    #[test]
    fn recusa_lixo_sem_entrar_em_panico() {
        for lixo in [
            &[][..],
            &[0x30],
            &[0x30, 0x85, 1, 2, 3, 4, 5],
            &[0x30, 0x05, 1],
        ] {
            assert!(Tlv::ler(lixo).is_err());
        }
        let profundo: Vec<u8> = std::iter::repeat_n([0x30u8, 0x80], 100).flatten().collect();
        assert!(Tlv::ler(&profundo).is_err());
    }
}
