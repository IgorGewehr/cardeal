//! A bobina térmica — 80 mm (a padrão) ou 58 mm (a mini, comum em quem está começando) — e o
//! cursor que escreve nela, de cima para baixo. A página tem a largura da bobina e a altura do
//! conteúdo: a primeira passada mede, a segunda desenha. Usado pelo DANFCE e pelo comprovante
//! de venda.

use qrcodegen::QrCode;

use crate::documento::{Canvas, Cor, Documento, MM};
use crate::fonte::{largura_texto, Fonte};

const PRETO: Cor = Cor(0.0, 0.0, 0.0);

/// A largura da bobina.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Bobina {
    /// 80 mm — a padrão.
    #[default]
    Mm80,
    /// 58 mm — a mini.
    Mm58,
}

impl Bobina {
    /// A largura do papel, em mm.
    #[must_use]
    pub const fn largura(self) -> f32 {
        match self {
            Self::Mm80 => 80.0,
            Self::Mm58 => 58.0,
        }
    }

    pub(crate) const fn margem(self) -> f32 {
        match self {
            Self::Mm80 => 4.0,
            Self::Mm58 => 3.0,
        }
    }

    pub(crate) fn util(self) -> f32 {
        self.largura() - 2.0 * self.margem()
    }

    /// O corpo do texto (pt).
    pub(crate) const fn corpo(self) -> f32 {
        match self {
            Self::Mm80 => 7.0,
            Self::Mm58 => 6.0,
        }
    }

    /// O texto miúdo (pt).
    pub(crate) const fn pequeno(self) -> f32 {
        match self {
            Self::Mm80 => 6.0,
            Self::Mm58 => 5.2,
        }
    }

    /// O nome no topo (pt).
    pub(crate) const fn titulo(self) -> f32 {
        match self {
            Self::Mm80 => 8.0,
            Self::Mm58 => 7.0,
        }
    }

    /// O lado do QR Code, em mm (o manual da NFC-e pede ao menos 25 mm).
    pub(crate) const fn qr_mm(self) -> f32 {
        match self {
            Self::Mm80 => 32.0,
            Self::Mm58 => 28.0,
        }
    }
}

/// Quebra `texto` em linhas que cabem em `largura_mm`.
pub(crate) fn quebrar(texto: &str, fonte: Fonte, tam: f32, largura_mm: f32) -> Vec<String> {
    let cabe = |s: &str| largura_texto(s, fonte, tam) / MM <= largura_mm;
    let mut linhas = Vec::new();
    let mut atual = String::new();
    for palavra in texto.split_whitespace() {
        let tentativa = if atual.is_empty() {
            palavra.to_string()
        } else {
            format!("{atual} {palavra}")
        };
        if cabe(&tentativa) {
            atual = tentativa;
            continue;
        }
        if !atual.is_empty() {
            linhas.push(std::mem::take(&mut atual));
        }
        // Uma palavra maior que a linha (uma URL): corta por caractere.
        let mut pedaco = String::new();
        for ch in palavra.chars() {
            pedaco.push(ch);
            if !cabe(&pedaco) {
                pedaco.pop();
                linhas.push(std::mem::take(&mut pedaco));
                pedaco.push(ch);
            }
        }
        atual = pedaco;
    }
    if !atual.is_empty() {
        linhas.push(atual);
    }
    linhas
}

/// O cursor de composição: escreve no canvas e desce.
pub(crate) struct Cursor {
    pub(crate) canvas: Canvas,
    pub(crate) y: f32,
    pub(crate) b: Bobina,
}

impl Cursor {
    fn novo(canvas: Canvas, b: Bobina) -> Self {
        Self {
            canvas,
            y: b.margem(),
            b,
        }
    }

    pub(crate) fn centro(&mut self, texto: &str, fonte: Fonte, tam: f32) {
        for linha in quebrar(texto, fonte, tam, self.b.util()) {
            let largura = largura_texto(&linha, fonte, tam) / MM;
            self.y += tam * 0.42;
            self.canvas.texto(
                (self.b.largura() - largura) / 2.0,
                self.y,
                fonte,
                tam,
                PRETO,
                &linha,
            );
        }
    }

    pub(crate) fn esquerda(&mut self, texto: &str, fonte: Fonte, tam: f32) {
        for linha in quebrar(texto, fonte, tam, self.b.util()) {
            self.y += tam * 0.42;
            self.canvas
                .texto(self.b.margem(), self.y, fonte, tam, PRETO, &linha);
        }
    }

    /// Rótulo à esquerda, valor à direita, na mesma linha, no corpo do texto.
    pub(crate) fn par(&mut self, rotulo: &str, valor: &str, fonte: Fonte) {
        let tam = self.b.corpo();
        self.y += tam * 0.42;
        self.canvas
            .texto(self.b.margem(), self.y, fonte, tam, PRETO, rotulo);
        let largura = largura_texto(valor, fonte, tam) / MM;
        self.canvas.texto(
            self.b.largura() - self.b.margem() - largura,
            self.y,
            fonte,
            tam,
            PRETO,
            valor,
        );
    }

    pub(crate) fn espaco(&mut self, mm: f32) {
        self.y += mm;
    }

    pub(crate) fn tracejado(&mut self) {
        let (inicio, fim) = (self.b.margem(), self.b.largura() - self.b.margem());
        self.y += 1.2;
        let mut x = inicio;
        while x < fim {
            self.canvas
                .linha(x, self.y, (x + 1.2).min(fim), self.y, 0.4, PRETO);
            x += 2.0;
        }
        self.y += 0.8;
    }

    pub(crate) fn qr(&mut self, qr: &QrCode) {
        let lado = self.b.qr_mm();
        let n = qr.size();
        let modulo = lado / f32::from(u16::try_from(n).unwrap_or(1));
        let x0 = (self.b.largura() - lado) / 2.0;
        self.y += 1.0;
        for linha in 0..n {
            let mut coluna = 0;
            while coluna < n {
                if qr.get_module(coluna, linha) {
                    // Junta os módulos pretos vizinhos da linha num retângulo só.
                    let inicio = coluna;
                    while coluna < n && qr.get_module(coluna, linha) {
                        coluna += 1;
                    }
                    let largura = (coluna - inicio) as f32 * modulo;
                    self.canvas.retangulo(
                        x0 + inicio as f32 * modulo,
                        self.y + linha as f32 * modulo,
                        largura,
                        modulo,
                        PRETO,
                    );
                } else {
                    coluna += 1;
                }
            }
        }
        self.y += lado + 1.0;
    }
}

/// Compõe duas vezes — mede numa página alta e descartada, desenha na altura certa — e devolve
/// o PDF de uma página da largura da bobina.
pub(crate) fn imprimir(b: Bobina, compor: impl Fn(&mut Cursor)) -> Vec<u8> {
    let mut medida = Cursor::novo(Canvas::nova(b.largura(), 2000.0), b);
    compor(&mut medida);
    let altura = medida.y + b.margem();
    let mut cursor = Cursor::novo(Canvas::nova(b.largura(), altura), b);
    compor(&mut cursor);
    let mut doc = Documento::novo();
    doc.pagina(cursor.canvas);
    doc.finalizar()
}
