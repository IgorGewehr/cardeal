//! As duas fontes padrão de PDF usadas nos documentos (Helvetica / Helvetica-Bold):
//! codificação `WinAnsi` (Latin-1, cobre o português) e métricas AFM para quebra de linha
//! e alinhamento à direita.
//!
//! O visualizador de PDF renderiza com as métricas reais da fonte embutida no leitor; a
//! tabela aqui só decide onde o layout quebra a linha e como alinha números — por isso
//! valores aproximados nos acentos são aceitáveis, e os dígitos (todos 556) ficam exatos.

/// Qual das duas variantes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fonte {
    /// Helvetica regular — corpo de texto.
    Regular,
    /// Helvetica-Bold — títulos, totais, cabeçalho de tabela.
    Negrito,
}

impl Fonte {
    /// O `BaseFont` do dicionário de fonte do PDF.
    pub(crate) const fn base_font(self) -> &'static str {
        match self {
            Self::Regular => "Helvetica",
            Self::Negrito => "Helvetica-Bold",
        }
    }

    /// O nome do recurso na página (`/F1` ou `/F2`).
    pub(crate) const fn recurso(self) -> &'static str {
        match self {
            Self::Regular => "F1",
            Self::Negrito => "F2",
        }
    }
}

// Larguras AFM (unidades de 1/1000 do em) para os caracteres imprimíveis 32..=126.
#[rustfmt::skip]
const HELVETICA: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278,
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556,
    1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778,
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556,
    333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556,
    556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
];

#[rustfmt::skip]
const HELVETICA_BOLD: [u16; 95] = [
    278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278, 278,
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 333, 333, 584, 584, 584, 611,
    975, 722, 722, 722, 722, 667, 611, 778, 722, 278, 556, 722, 611, 833, 722, 778,
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 333, 278, 333, 584, 556,
    333, 556, 611, 556, 611, 556, 333, 611, 611, 278, 278, 556, 278, 889, 611, 611,
    611, 611, 389, 556, 333, 611, 556, 778, 556, 556, 500, 389, 280, 389, 584,
];

/// Largura de uma string em pontos, para o tamanho de fonte dado.
#[must_use]
pub fn largura_texto(texto: &str, fonte: Fonte, tam_pt: f32) -> f32 {
    let tabela = match fonte {
        Fonte::Regular => &HELVETICA,
        Fonte::Negrito => &HELVETICA_BOLD,
    };
    let milhares: u32 = texto
        .chars()
        .map(|c| u32::from(largura_glifo(c, tabela)))
        .sum();
    (milhares as f32) * tam_pt / 1000.0
}

fn largura_glifo(c: char, tabela: &[u16; 95]) -> u16 {
    let cp = c as u32;
    if (32..=126).contains(&cp) {
        return tabela[(cp - 32) as usize];
    }
    // Acentuados Latin-1: mesma largura da letra-base.
    let base = match c {
        'á' | 'à' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'ç' => 'c',
        'ñ' => 'n',
        'ý' | 'ÿ' => 'y',
        'Á' | 'À' | 'Â' | 'Ã' | 'Ä' | 'Å' => 'A',
        'É' | 'È' | 'Ê' | 'Ë' => 'E',
        'Í' | 'Ì' | 'Î' | 'Ï' => 'I',
        'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' => 'O',
        'Ú' | 'Ù' | 'Û' | 'Ü' => 'U',
        'Ç' => 'C',
        'Ñ' => 'N',
        'º' | 'ª' | '°' => return 350,
        '“' | '”' | '„' => return 333,
        '…' => return 1000,
        _ => return 556,
    };
    tabela[(base as u32 - 32) as usize]
}

/// Escapa e recodifica uma string para uma literal de texto PDF em `WinAnsiEncoding`
/// (CP1252). Caracteres fora do repertório viram `?`.
#[must_use]
pub fn literal_pdf(texto: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(texto.len() + 2);
    out.push(b'(');
    for c in texto.chars() {
        let b = cp1252(c);
        match b {
            b'(' | b')' | b'\\' => {
                out.push(b'\\');
                out.push(b);
            }
            b'\n' => out.extend_from_slice(b"\\n"),
            b'\r' => out.extend_from_slice(b"\\r"),
            b'\t' => out.extend_from_slice(b"\\t"),
            _ => out.push(b),
        }
    }
    out.push(b')');
    out
}

fn cp1252(c: char) -> u8 {
    let cp = c as u32;
    if cp < 0x80 || (0xA0..=0xFF).contains(&cp) {
        return cp as u8;
    }
    match c {
        '€' => 0x80,
        '‚' => 0x82,
        'ƒ' => 0x83,
        '„' => 0x84,
        '…' => 0x85,
        '†' => 0x86,
        '‡' => 0x87,
        'ˆ' => 0x88,
        '‰' => 0x89,
        'Š' => 0x8A,
        '‹' => 0x8B,
        'Œ' => 0x8C,
        '‘' | '’' => 0x27,
        '“' | '”' => 0x22,
        '•' => 0x95,
        '–' => 0x96,
        '—' => 0x97,
        '™' => 0x99,
        _ => b'?',
    }
}
