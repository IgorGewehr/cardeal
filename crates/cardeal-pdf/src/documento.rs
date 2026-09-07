//! Um escritor de PDF 1.7 mínimo: páginas A4, as duas fontes padrão, retângulos, linhas,
//! texto e imagens RGB (sem filtro — o documento fica um pouco maior, mas o código não
//! depende de nenhuma biblioteca de compressão).

use std::fmt::Write as _;

use crate::fonte::{literal_pdf, Fonte};

/// Largura de uma página A4 em pontos (210 mm).
pub const A4_LARGURA: f32 = 595.276;
/// Altura de uma página A4 em pontos (297 mm).
pub const A4_ALTURA: f32 = 841.890;

/// Pontos por milímetro.
pub const MM: f32 = 72.0 / 25.4;

/// Uma cor RGB (componentes 0.0..=1.0).
#[derive(Debug, Clone, Copy)]
pub struct Cor(pub f32, pub f32, pub f32);

impl Cor {
    /// De bytes `0..=255`.
    #[must_use]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
    }
}

/// Uma imagem RGB de 8 bits por componente, pronta para embutir.
pub struct Imagem {
    pub(crate) largura: u32,
    pub(crate) altura: u32,
    pub(crate) rgb: Vec<u8>,
}

impl Imagem {
    /// Cria a partir de pixels RGB entrelaçados (`largura*altura*3` bytes).
    ///
    /// # Panics
    /// Se `rgb.len() != largura * altura * 3`.
    #[must_use]
    pub fn rgb8(largura: u32, altura: u32, rgb: Vec<u8>) -> Self {
        assert_eq!(rgb.len(), (largura * altura * 3) as usize, "buffer RGB inconsistente");
        Self { largura, altura, rgb }
    }

    /// A proporção largura/altura.
    #[must_use]
    pub fn proporcao(&self) -> f32 {
        self.largura as f32 / self.altura.max(1) as f32
    }
}

/// Uma superfície de desenho de uma página, em coordenadas de milímetro com origem no
/// canto superior esquerdo (y cresce para baixo).
pub struct Canvas {
    ops: String,
    altura_pt: f32,
}

impl Canvas {
    /// Uma superfície A4 em branco.
    #[must_use]
    pub fn nova_a4() -> Self {
        Self {
            ops: String::new(),
            altura_pt: A4_ALTURA,
        }
    }

    /// Consome a superfície e devolve o fluxo de operadores de conteúdo.
    #[must_use]
    pub fn em_operadores(self) -> String {
        self.ops
    }

    fn y(&self, y_mm: f32) -> f32 {
        self.altura_pt - y_mm * MM
    }

    /// Retângulo preenchido.
    pub fn retangulo(&mut self, x_mm: f32, y_mm: f32, larg_mm: f32, alt_mm: f32, cor: Cor) {
        let _ = writeln!(
            self.ops,
            "{:.3} {:.3} {:.3} rg {:.2} {:.2} {:.2} {:.2} re f",
            cor.0,
            cor.1,
            cor.2,
            x_mm * MM,
            self.y(y_mm + alt_mm),
            larg_mm * MM,
            alt_mm * MM,
        );
    }

    /// Linha reta.
    pub fn linha(
        &mut self,
        x1_mm: f32,
        y1_mm: f32,
        x2_mm: f32,
        y2_mm: f32,
        espessura_pt: f32,
        cor: Cor,
    ) {
        let _ = writeln!(
            self.ops,
            "{:.3} {:.3} {:.3} RG {:.2} w {:.2} {:.2} m {:.2} {:.2} l S",
            cor.0,
            cor.1,
            cor.2,
            espessura_pt,
            x1_mm * MM,
            self.y(y1_mm),
            x2_mm * MM,
            self.y(y2_mm),
        );
    }

    /// Texto com a âncora na linha de base, alinhado à esquerda de `x_mm`.
    pub fn texto(&mut self, x_mm: f32, y_base_mm: f32, fonte: Fonte, tam_pt: f32, cor: Cor, s: &str) {
        let literal = String::from_utf8_lossy(&literal_pdf(s)).into_owned();
        let _ = writeln!(
            self.ops,
            "BT /{} {:.2} Tf {:.3} {:.3} {:.3} rg 1 0 0 1 {:.2} {:.2} Tm {} Tj ET",
            fonte.recurso(),
            tam_pt,
            cor.0,
            cor.1,
            cor.2,
            x_mm * MM,
            self.y(y_base_mm),
            literal,
        );
    }

    /// Desenha a `indice`-ésima imagem do documento numa caixa em mm.
    pub fn imagem(&mut self, indice: usize, x_mm: f32, y_mm: f32, larg_mm: f32, alt_mm: f32) {
        let _ = writeln!(
            self.ops,
            "q {:.2} 0 0 {:.2} {:.2} {:.2} cm /Im{} Do Q",
            larg_mm * MM,
            alt_mm * MM,
            x_mm * MM,
            self.y(y_mm + alt_mm),
            indice,
        );
    }
}

/// O documento em construção.
pub struct Documento {
    paginas: Vec<String>,
    imagens: Vec<Imagem>,
}

impl Documento {
    /// Um documento vazio.
    #[must_use]
    pub fn novo() -> Self {
        Self {
            paginas: Vec::new(),
            imagens: Vec::new(),
        }
    }

    /// Registra uma imagem e devolve o índice para `Canvas::imagem`.
    pub fn adicionar_imagem(&mut self, imagem: Imagem) -> usize {
        self.imagens.push(imagem);
        self.imagens.len() - 1
    }

    /// Adiciona uma página a partir de um fluxo de operadores já montado.
    pub fn pagina_bruta(&mut self, operadores: String) {
        self.paginas.push(operadores);
    }

    /// Serializa o PDF.
    #[must_use]
    pub fn finalizar(self) -> Vec<u8> {
        let ni = self.imagens.len();
        let np = self.paginas.len().max(1);
        // 1 Catalog, 2 Pages, 3 F1, 4 F2, 5..(5+ni) imagens, depois 2 objetos por página.
        let base_pag = 5 + ni;

        let mut objs: Vec<Vec<u8>> = Vec::new();
        let mut push = |b: Vec<u8>| objs.push(b);

        // 1 — Catalog
        push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());

        // 2 — Pages
        let mut kids = String::new();
        for p in 0..np {
            let _ = write!(kids, "{} 0 R ", base_pag + p * 2);
        }
        push(format!("<< /Type /Pages /Kids [{kids}] /Count {np} >>").into_bytes());

        // 3, 4 — fontes
        for f in [Fonte::Regular, Fonte::Negrito] {
            push(
                format!(
                    "<< /Type /Font /Subtype /Type1 /BaseFont /{} /Encoding /WinAnsiEncoding >>",
                    f.base_font()
                )
                .into_bytes(),
            );
        }

        // 5.. — imagens
        for img in &self.imagens {
            let mut o = format!(
                "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB \
                 /BitsPerComponent 8 /Length {} >>\nstream\n",
                img.largura,
                img.altura,
                img.rgb.len()
            )
            .into_bytes();
            o.extend_from_slice(&img.rgb);
            o.extend_from_slice(b"\nendstream");
            push(o);
        }

        // páginas
        let xobject = if ni == 0 {
            String::new()
        } else {
            let mut entradas = String::new();
            for i in 0..ni {
                let _ = write!(entradas, "/Im{} {} 0 R ", i, 5 + i);
            }
            format!(" /XObject << {entradas}>>")
        };
        for (p, conteudo) in self.paginas.iter().enumerate() {
            let pag_obj = base_pag + p * 2;
            let cont_obj = pag_obj + 1;
            push(
                format!(
                    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {A4_LARGURA:.3} {A4_ALTURA:.3}] \
                     /Resources << /Font << /F1 3 0 R /F2 4 0 R >>{xobject} >> /Contents {cont_obj} 0 R >>"
                )
                .into_bytes(),
            );
            let mut o =
                format!("<< /Length {} >>\nstream\n", conteudo.len()).into_bytes();
            o.extend_from_slice(conteudo.as_bytes());
            o.extend_from_slice(b"\nendstream");
            push(o);
        }
        if self.paginas.is_empty() {
            // página em branco de segurança
            push(
                format!(
                    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {A4_LARGURA:.3} {A4_ALTURA:.3}] \
                     /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents {} 0 R >>",
                    base_pag + 1
                )
                .into_bytes(),
            );
            push(b"<< /Length 0 >>\nstream\n\nendstream".to_vec());
        }

        // montagem com xref
        let mut out: Vec<u8> = Vec::with_capacity(4096);
        out.extend_from_slice(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n");
        let mut offsets = Vec::with_capacity(objs.len() + 1);
        for (i, body) in objs.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
            out.extend_from_slice(body);
            out.extend_from_slice(b"\nendobj\n");
        }
        let xref_pos = out.len();
        let total = objs.len() + 1;
        out.extend_from_slice(format!("xref\n0 {total}\n").as_bytes());
        out.extend_from_slice(b"0000000000 65535 f \n");
        for off in &offsets {
            out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {total} /Root 1 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n"
            )
            .as_bytes(),
        );
        out
    }
}
