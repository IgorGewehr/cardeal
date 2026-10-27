//! A bobina em **ESC/POS** — os bytes que a impressora térmica entende, para imprimir direto, sem
//! abrir o PDF: o DANFCE e o comprovante de venda saem do **mesmo** `compor` do PDF (a interface
//! [`Escrita`]), então o conteúdo das duas saídas não diverge.
//!
//! O que se usa é o núcleo comum das térmicas do balcão (Epson TM, Elgin i9, Bematech MP-4200 em
//! modo ESC/POS, Daruma, Tanca): `ESC @`, alinhamento, negrito, tamanho duplo, avanço, corte
//! parcial e a imagem em `GS v 0` — o QR Code vai como imagem (os módulos desenhados ponto a
//! ponto), que toda ESC/POS imprime, em vez do `GS ( k`, que nem todas têm.
//!
//! Acentos: a página de código varia de marca para marca; o padrão é **sem acento** (o texto é
//! transliterado: "Descrição" → "Descricao"), que sai certo em qualquer uma. Com
//! [`OpcoesEscPos::acentos`], a impressora é posta na PC860 (português, `ESC t 3`).

use qrcodegen::{QrCode, QrCodeEcc};

use crate::bobina::{Bobina, Escrita};
use crate::fonte::Fonte;
use crate::{ComprovantePdf, DanfcePdf, ErroPdf};

const ESC: u8 = 0x1B;
const GS: u8 = 0x1D;

/// Como falar com a impressora.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OpcoesEscPos {
    /// Imprimir com acento (página de código PC860). Sem: o texto sai transliterado.
    pub acentos: bool,
}

impl Bobina {
    /// Quantos caracteres cabem numa linha (fonte A, 12 × 24 pontos).
    #[must_use]
    pub const fn colunas(self) -> usize {
        match self {
            Self::Mm80 => 48,
            Self::Mm58 => 32,
        }
    }

    /// A largura impressa, em pontos (203 dpi: 8 pontos por mm).
    const fn pontos(self) -> usize {
        match self {
            Self::Mm80 => 576,
            Self::Mm58 => 384,
        }
    }
}

/// O escritor ESC/POS: acumula os comandos.
pub(crate) struct EscPos {
    b: Bobina,
    opcoes: OpcoesEscPos,
    bytes: Vec<u8>,
}

impl EscPos {
    fn nova(b: Bobina, opcoes: OpcoesEscPos) -> Self {
        let mut e = Self {
            b,
            opcoes,
            bytes: vec![ESC, b'@'],
        };
        if opcoes.acentos {
            e.bytes.extend([ESC, b't', 3]);
        }
        e
    }

    /// Avança o papel e corta (parcial, que deixa a via presa pela ponta).
    fn finalizar(mut self) -> Vec<u8> {
        self.bytes.extend([ESC, b'd', 4, GS, b'V', 66, 0]);
        self.bytes
    }

    fn texto(&mut self, s: &str) {
        for ch in s.chars() {
            self.bytes.push(if self.opcoes.acentos {
                pc860(ch)
            } else {
                ascii(ch)
            });
        }
        self.bytes.push(b'\n');
    }

    /// O título (maior que o corpo) sai em tamanho duplo — e cabe a metade por linha.
    fn linhas(&mut self, texto: &str, fonte: Fonte, tam: f32, centro: bool) {
        let duplo = tam > self.b.corpo();
        let colunas = if duplo {
            self.b.colunas() / 2
        } else {
            self.b.colunas()
        };
        self.bytes.extend([ESC, b'a', u8::from(centro)]);
        self.bytes
            .extend([ESC, b'E', u8::from(fonte == Fonte::Negrito)]);
        if duplo {
            self.bytes.extend([GS, b'!', 0x11]);
        }
        for linha in quebrar(texto, colunas) {
            self.texto(&linha);
        }
        if duplo {
            self.bytes.extend([GS, b'!', 0]);
        }
        self.bytes.extend([ESC, b'E', 0, ESC, b'a', 0]);
    }
}

impl Escrita for EscPos {
    fn bobina(&self) -> Bobina {
        self.b
    }

    fn centro(&mut self, texto: &str, fonte: Fonte, tam: f32) {
        self.linhas(texto, fonte, tam, true);
    }

    fn esquerda(&mut self, texto: &str, fonte: Fonte, tam: f32) {
        self.linhas(texto, fonte, tam, false);
    }

    fn par(&mut self, rotulo: &str, valor: &str, fonte: Fonte) {
        let colunas = self.b.colunas();
        let (r, v) = (rotulo.chars().count(), valor.chars().count());
        self.bytes
            .extend([ESC, b'E', u8::from(fonte == Fonte::Negrito)]);
        if r + 1 + v <= colunas {
            self.texto(&format!("{rotulo}{}{valor}", " ".repeat(colunas - r - v)));
        } else {
            // Não cabe numa linha: o rótulo (quebrado) e o valor à direita, embaixo.
            for linha in quebrar(rotulo, colunas) {
                self.texto(&linha);
            }
            self.texto(&format!("{valor:>colunas$}"));
        }
        self.bytes.extend([ESC, b'E', 0]);
    }

    fn espaco(&mut self, mm: f32) {
        // Uma linha tem uns 3 mm; o espaço miúdo do PDF (menos de 1,5 mm) some aqui.
        let linhas = (mm / 3.0).round();
        if linhas >= 1.0 {
            self.bytes.extend([ESC, b'd', linhas.min(10.0) as u8]);
        }
    }

    fn tracejado(&mut self) {
        let linha = "-".repeat(self.b.colunas());
        self.texto(&linha);
    }

    fn qr(&mut self, qr: &QrCode) {
        let n = usize::try_from(qr.size()).unwrap_or(0);
        if n == 0 {
            return;
        }
        // O lado pedido (o manual da NFC-e: ao menos 25 mm), em pontos inteiros por módulo.
        let alvo = (self.b.qr_mm() * 8.0) as usize;
        let modulo = (alvo / n).max(2);
        let lado = n * modulo;
        let largura = self.b.pontos();
        let bytes_por_linha = largura / 8;
        let x0 = largura.saturating_sub(lado) / 2;
        // GS v 0: largura em bytes e altura em pontos, a imagem linha a linha.
        let (xl, xh) = ((bytes_por_linha & 0xFF) as u8, (bytes_por_linha >> 8) as u8);
        let (yl, yh) = ((lado & 0xFF) as u8, (lado >> 8) as u8);
        self.bytes
            .extend([ESC, b'a', 0, GS, b'v', b'0', 0, xl, xh, yl, yh]);
        for y in 0..lado {
            let linha_qr = i32::try_from(y / modulo).unwrap_or(0);
            let mut linha = vec![0u8; bytes_por_linha];
            for x in 0..lado {
                let coluna_qr = i32::try_from(x / modulo).unwrap_or(0);
                if qr.get_module(coluna_qr, linha_qr) {
                    let ponto = x0 + x;
                    linha[ponto / 8] |= 0x80 >> (ponto % 8);
                }
            }
            self.bytes.extend(linha);
        }
        self.bytes.push(b'\n');
    }
}

/// Quebra o texto em linhas de até `colunas` caracteres (por palavra; a palavra maior que a
/// linha — uma URL — é cortada).
fn quebrar(texto: &str, colunas: usize) -> Vec<String> {
    let mut linhas = Vec::new();
    let mut atual = String::new();
    for palavra in texto.split_whitespace() {
        let cabe = atual.chars().count() + usize::from(!atual.is_empty()) + palavra.chars().count()
            <= colunas;
        if cabe {
            if !atual.is_empty() {
                atual.push(' ');
            }
            atual.push_str(palavra);
            continue;
        }
        if !atual.is_empty() {
            linhas.push(std::mem::take(&mut atual));
        }
        let mut letras: Vec<char> = palavra.chars().collect();
        while letras.len() > colunas {
            linhas.push(letras.drain(..colunas).collect());
        }
        atual = letras.into_iter().collect();
    }
    if !atual.is_empty() {
        linhas.push(atual);
    }
    linhas
}

/// O caractere sem acento — o que qualquer térmica imprime.
fn ascii(ch: char) -> u8 {
    let base = match ch {
        'á' | 'à' | 'â' | 'ã' | 'ä' | 'ª' => 'a',
        'Á' | 'À' | 'Â' | 'Ã' | 'Ä' => 'A',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'É' | 'È' | 'Ê' | 'Ë' => 'E',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'Í' | 'Ì' | 'Î' | 'Ï' => 'I',
        'ó' | 'ò' | 'ô' | 'õ' | 'ö' | 'º' | '°' => 'o',
        'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' => 'O',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'Ú' | 'Ù' | 'Û' | 'Ü' => 'U',
        'ç' => 'c',
        'Ç' => 'C',
        'ñ' => 'n',
        'Ñ' => 'N',
        '·' | '—' | '–' => '-',
        '×' => 'x',
        c if c.is_ascii() && !c.is_ascii_control() => c,
        _ => '?',
    };
    base as u8
}

/// O caractere na página de código PC860 (português).
fn pc860(ch: char) -> u8 {
    match ch {
        'Ç' => 0x80,
        'ü' => 0x81,
        'é' => 0x82,
        'â' => 0x83,
        'ã' => 0x84,
        'à' => 0x85,
        'Á' => 0x86,
        'ç' => 0x87,
        'ê' => 0x88,
        'Ê' => 0x89,
        'è' => 0x8A,
        'Í' => 0x8B,
        'Ô' => 0x8C,
        'ì' => 0x8D,
        'Ã' => 0x8E,
        'Â' => 0x8F,
        'É' => 0x90,
        'À' => 0x91,
        'È' => 0x92,
        'ô' => 0x93,
        'õ' => 0x94,
        'ò' => 0x95,
        'Ú' => 0x96,
        'ù' => 0x97,
        'Ì' => 0x98,
        'Õ' => 0x99,
        'Ü' => 0x9A,
        'Ù' => 0x9D,
        'Ó' => 0x9F,
        'á' => 0xA0,
        'í' => 0xA1,
        'ó' => 0xA2,
        'ú' => 0xA3,
        'ñ' => 0xA4,
        'Ñ' => 0xA5,
        'ª' => 0xA6,
        'º' | '°' => 0xA7,
        'Ò' => 0xA9,
        c => ascii(c),
    }
}

/// O DANFCE em ESC/POS.
///
/// # Errors
/// [`ErroPdf::QrCode`] se o conteúdo do QR Code não couber num QR Code.
pub fn danfce_escpos(
    d: &DanfcePdf,
    bobina: Bobina,
    opcoes: OpcoesEscPos,
) -> Result<Vec<u8>, ErroPdf> {
    let qr = d
        .qr_code
        .as_deref()
        .map(|t| QrCode::encode_text(t, QrCodeEcc::Medium))
        .transpose()
        .map_err(|e| ErroPdf::QrCode(format!("{e:?}")))?;
    let mut e = EscPos::nova(bobina, opcoes);
    crate::danfce::compor(&mut e, d, qr.as_ref());
    Ok(e.finalizar())
}

/// O comprovante de venda em ESC/POS.
#[must_use]
pub fn comprovante_escpos(d: &ComprovantePdf, bobina: Bobina, opcoes: OpcoesEscPos) -> Vec<u8> {
    let mut e = EscPos::nova(bobina, opcoes);
    crate::comprovante::compor(&mut e, d);
    e.finalizar()
}

/// A página de teste: a largura da bobina, os acentos e o corte.
#[must_use]
pub fn teste_escpos(bobina: Bobina, opcoes: OpcoesEscPos) -> Vec<u8> {
    let mut e = EscPos::nova(bobina, opcoes);
    e.centro("Cardeal", Fonte::Negrito, bobina.titulo());
    e.centro("Teste de impressão", Fonte::Regular, bobina.corpo());
    e.tracejado();
    e.esquerda(
        &format!(
            "Bobina de {} mm, {} colunas.",
            bobina.largura(),
            bobina.colunas()
        ),
        Fonte::Regular,
        bobina.corpo(),
    );
    e.esquerda(
        if opcoes.acentos {
            "Acentos: ação, coração, café, você. Se saiu esquisito, desligue os acentos."
        } else {
            "Sem acentos (o padrão): sai certo em qualquer impressora."
        },
        Fonte::Regular,
        bobina.corpo(),
    );
    e.par("Esquerda", "Direita", Fonte::Negrito);
    e.tracejado();
    e.finalizar()
}

#[cfg(test)]
mod testes {
    use super::*;

    fn tem(bytes: &[u8], seq: &[u8]) -> bool {
        bytes.windows(seq.len()).any(|w| w == seq)
    }

    #[test]
    fn comeca_inicializando_e_termina_cortando() {
        let b = teste_escpos(Bobina::Mm80, OpcoesEscPos::default());
        assert!(b.starts_with(&[ESC, b'@']));
        assert!(b.ends_with(&[GS, b'V', 66, 0]));
        // Sem acento: nada fora do ASCII, e a página de código não é trocada.
        assert!(!tem(&b, &[ESC, b't']));
        assert!(tem(&b, b"Teste de impressao"));
        assert!(b.iter().all(|&c| c < 0x80));
    }

    #[test]
    fn com_acentos_usa_a_pc860() {
        let b = teste_escpos(Bobina::Mm58, OpcoesEscPos { acentos: true });
        assert!(tem(&b, &[ESC, b't', 3]));
        // "ação" na PC860: a, ç, ã, o.
        assert!(tem(&b, &[b'a', 0x87, 0x84, b'o']));
    }

    #[test]
    fn o_par_alinha_a_direita_e_o_texto_quebra_na_largura() {
        let mut e = EscPos::nova(Bobina::Mm58, OpcoesEscPos::default());
        e.par("Valor a pagar R$", "45,00", Fonte::Negrito);
        let linha = format!("Valor a pagar R${}45,00\n", " ".repeat(32 - 16 - 5));
        assert!(tem(&e.bytes, linha.as_bytes()));
        assert_eq!(
            quebrar("https://www.sefaz.rs.gov.br/NFCE/NFCE-COM.aspx", 20),
            vec!["https://www.sefaz.rs", ".gov.br/NFCE/NFCE-CO", "M.aspx"]
        );
        assert_eq!(quebrar("um dois tres", 7), vec!["um dois", "tres"]);
    }

    #[test]
    fn o_qr_vai_como_imagem_centrada_de_ao_menos_25_mm() {
        let qr = QrCode::encode_text("https://x/q?p=4326|3|2", QrCodeEcc::Medium).unwrap();
        let mut e = EscPos::nova(Bobina::Mm80, OpcoesEscPos::default());
        e.qr(&qr);
        let i = e
            .bytes
            .windows(3)
            .position(|w| w == [GS, b'v', b'0'])
            .unwrap();
        let cab = &e.bytes[i..i + 8];
        let (largura, altura) = (
            usize::from(cab[4]) | usize::from(cab[5]) << 8,
            usize::from(cab[6]) | usize::from(cab[7]) << 8,
        );
        assert_eq!(
            largura, 72,
            "a largura inteira da bobina de 80 mm, em bytes"
        );
        assert!(altura >= 25 * 8, "{altura} pontos");
        assert_eq!(e.bytes.len(), i + 8 + largura * altura + 1);
        // O primeiro e o último byte de cada linha ficam em branco: centrado.
        let imagem = &e.bytes[i + 8..i + 8 + largura * altura];
        assert!(imagem
            .chunks(largura)
            .all(|l| l[0] == 0 && l[largura - 1] == 0));
    }
}
