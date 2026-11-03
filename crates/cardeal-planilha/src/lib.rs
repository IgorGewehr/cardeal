//! # cardeal-planilha
//!
//! Planilhas `.xlsx` do Cardeal — a do pacote do contador (DRE, DFC, balancete, diário). Como o
//! `cardeal-pdf`, sem biblioteca externa: um `.xlsx` é um ZIP de XMLs (Office Open XML), e aqui
//! ele é escrito à mão com o mínimo que Excel, `LibreOffice` e Google Planilhas abrem — textos
//! em linha (`inlineStr`), dinheiro como número com formato `#.##0,00`, datas como data de
//! verdade, negrito, larguras de coluna e cabeçalho congelado.
//!
//! O [`zip`] também empacota os XMLs das notas do mês para o contador, comprimidos — o deflate
//! é o único pedaço de fora (`miniz_oxide`, que já vinha no grafo pelo `png`).
//!
//! ```
//! use cardeal_planilha::{Aba, Celula, Planilha};
//! let mut aba = Aba::nova("DRE").colunas(&[40.0, 16.0]);
//! aba.linha(vec![Celula::texto("Conta").negrito(), Celula::texto("Valor").negrito()]);
//! aba.linha(vec![Celula::texto("Receita"), Celula::dinheiro(cardeal_kernel::Dinheiro::reais(10))]);
//! let bytes = Planilha::nova().aba(aba).gerar();
//! assert_eq!(&bytes[..2], b"PK");
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]

pub mod zip;

use std::fmt::Write as _;

use cardeal_kernel::{Data, Dinheiro};

/// O valor de uma célula.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Valor {
    /// Nada.
    Vazio,
    /// Texto.
    Texto(String),
    /// Dinheiro (vira número com duas casas).
    Dinheiro(Dinheiro),
    /// Número inteiro.
    Inteiro(i64),
    /// Data (vira data do Excel, `dd/mm/aaaa`).
    Data(Data),
}

/// Uma célula: o valor e se vai em negrito.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Celula {
    /// O valor.
    pub valor: Valor,
    /// Negrito.
    pub negrito: bool,
}

impl Celula {
    /// Vazia.
    #[must_use]
    pub const fn vazia() -> Self {
        Self {
            valor: Valor::Vazio,
            negrito: false,
        }
    }

    /// Texto.
    #[must_use]
    pub fn texto(t: impl Into<String>) -> Self {
        Self {
            valor: Valor::Texto(t.into()),
            negrito: false,
        }
    }

    /// Dinheiro.
    #[must_use]
    pub const fn dinheiro(d: Dinheiro) -> Self {
        Self {
            valor: Valor::Dinheiro(d),
            negrito: false,
        }
    }

    /// Inteiro.
    #[must_use]
    pub const fn inteiro(n: i64) -> Self {
        Self {
            valor: Valor::Inteiro(n),
            negrito: false,
        }
    }

    /// Data.
    #[must_use]
    pub const fn data(d: Data) -> Self {
        Self {
            valor: Valor::Data(d),
            negrito: false,
        }
    }

    /// Em negrito.
    #[must_use]
    pub const fn negrito(mut self) -> Self {
        self.negrito = true;
        self
    }
}

/// Uma aba da planilha.
#[derive(Debug, Clone, PartialEq)]
pub struct Aba {
    nome: String,
    larguras: Vec<f32>,
    linhas: Vec<Vec<Celula>>,
    congelar: bool,
}

impl Aba {
    /// Uma aba com o nome dado (o Excel aceita até 31 caracteres, sem `[]:*?/\`).
    #[must_use]
    pub fn nova(nome: &str) -> Self {
        let limpo: String = nome
            .chars()
            .filter(|c| !matches!(c, '[' | ']' | ':' | '*' | '?' | '/' | '\\'))
            .take(31)
            .collect();
        Self {
            nome: if limpo.trim().is_empty() {
                "Planilha".to_owned()
            } else {
                limpo
            },
            larguras: Vec::new(),
            linhas: Vec::new(),
            congelar: false,
        }
    }

    /// As larguras das colunas, em caracteres.
    #[must_use]
    pub fn colunas(mut self, larguras: &[f32]) -> Self {
        self.larguras = larguras.to_vec();
        self
    }

    /// Congela a primeira linha (o cabeçalho continua visível ao rolar).
    #[must_use]
    pub const fn congelar_cabecalho(mut self) -> Self {
        self.congelar = true;
        self
    }

    /// Acrescenta uma linha.
    pub fn linha(&mut self, celulas: Vec<Celula>) {
        self.linhas.push(celulas);
    }

    /// O nome.
    #[must_use]
    pub fn nome(&self) -> &str {
        &self.nome
    }

    /// Quantas linhas.
    #[must_use]
    pub fn tamanho(&self) -> usize {
        self.linhas.len()
    }
}

/// Uma planilha com uma ou mais abas.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Planilha {
    abas: Vec<Aba>,
}

/// "A", "B", …, "Z", "AA"…
fn coluna(mut i: usize) -> String {
    let mut s = Vec::new();
    loop {
        s.push(b'A' + u8::try_from(i % 26).unwrap_or(0));
        if i < 26 {
            break;
        }
        i = i / 26 - 1;
    }
    s.reverse();
    String::from_utf8(s).unwrap_or_default()
}

fn escapar(t: &str) -> String {
    let mut s = String::with_capacity(t.len());
    for c in t.chars() {
        match c {
            '&' => s.push_str("&amp;"),
            '<' => s.push_str("&lt;"),
            '>' => s.push_str("&gt;"),
            '"' => s.push_str("&quot;"),
            // Caracteres de controle não cabem em XML 1.0.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => s.push(c),
        }
    }
    s
}

/// Centavos → "1234.56", exato (sem ponto flutuante).
fn decimal(d: Dinheiro) -> String {
    let c = d.em_centavos();
    let sinal = if c < 0 { "-" } else { "" };
    let a = c.unsigned_abs();
    format!("{sinal}{}.{:02}", a / 100, a % 100)
}

/// Estilos: 0 normal, 1 negrito, 2 dinheiro, 3 dinheiro negrito, 4 data, 5 data negrito.
const ESTILOS: &str = r##"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<numFmts count="2"><numFmt numFmtId="164" formatCode="#,##0.00"/><numFmt numFmtId="165" formatCode="dd/mm/yyyy"/></numFmts>
<fonts count="2"><font><sz val="11"/><name val="Calibri"/></font><font><b/><sz val="11"/><name val="Calibri"/></font></fonts>
<fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills>
<borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders>
<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>
<cellXfs count="6">
<xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/>
<xf numFmtId="0" fontId="1" fillId="0" borderId="0" xfId="0" applyFont="1"/>
<xf numFmtId="164" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/>
<xf numFmtId="164" fontId="1" fillId="0" borderId="0" xfId="0" applyNumberFormat="1" applyFont="1"/>
<xf numFmtId="165" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/>
<xf numFmtId="165" fontId="1" fillId="0" borderId="0" xfId="0" applyNumberFormat="1" applyFont="1"/>
</cellXfs>
<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles>
</styleSheet>"##;

/// 1970-01-01 (a época de `Data`) no calendário serial do Excel.
const SERIAL_DA_EPOCA: i64 = 25_569;

impl Planilha {
    /// Vazia.
    #[must_use]
    pub fn nova() -> Self {
        Self::default()
    }

    /// Acrescenta uma aba.
    #[must_use]
    pub fn aba(mut self, aba: Aba) -> Self {
        self.abas.push(aba);
        self
    }

    /// As abas.
    #[must_use]
    pub fn abas(&self) -> &[Aba] {
        &self.abas
    }

    fn folha(aba: &Aba) -> String {
        let mut x = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
             <worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">",
        );
        if aba.congelar {
            x.push_str(
                "<sheetViews><sheetView workbookViewId=\"0\"><pane ySplit=\"1\" topLeftCell=\"A2\" \
                 activePane=\"bottomLeft\" state=\"frozen\"/></sheetView></sheetViews>",
            );
        }
        if !aba.larguras.is_empty() {
            x.push_str("<cols>");
            for (i, l) in aba.larguras.iter().enumerate() {
                let _ = write!(
                    x,
                    "<col min=\"{n}\" max=\"{n}\" width=\"{l:.1}\" customWidth=\"1\"/>",
                    n = i + 1
                );
            }
            x.push_str("</cols>");
        }
        x.push_str("<sheetData>");
        for (r, linha) in aba.linhas.iter().enumerate() {
            let _ = write!(x, "<row r=\"{}\">", r + 1);
            for (c, cel) in linha.iter().enumerate() {
                let rf = format!("{}{}", coluna(c), r + 1);
                let n = usize::from(cel.negrito);
                match &cel.valor {
                    Valor::Vazio => {}
                    Valor::Texto(t) => {
                        let _ = write!(
                            x,
                            "<c r=\"{rf}\" t=\"inlineStr\" s=\"{n}\"><is><t xml:space=\"preserve\">{}</t></is></c>",
                            escapar(t)
                        );
                    }
                    Valor::Dinheiro(d) => {
                        let _ = write!(
                            x,
                            "<c r=\"{rf}\" s=\"{}\"><v>{}</v></c>",
                            2 + n,
                            decimal(*d)
                        );
                    }
                    Valor::Inteiro(i) => {
                        let _ = write!(x, "<c r=\"{rf}\" s=\"{n}\"><v>{i}</v></c>");
                    }
                    Valor::Data(d) => {
                        let _ = write!(
                            x,
                            "<c r=\"{rf}\" s=\"{}\"><v>{}</v></c>",
                            4 + n,
                            i64::from(d.em_dias()) + SERIAL_DA_EPOCA
                        );
                    }
                }
            }
            x.push_str("</row>");
        }
        x.push_str("</sheetData></worksheet>");
        x
    }

    /// Os bytes do `.xlsx`.
    #[must_use]
    pub fn gerar(&self) -> Vec<u8> {
        let abas: Vec<Aba> = if self.abas.is_empty() {
            vec![Aba::nova("Planilha")]
        } else {
            self.abas.clone()
        };
        let mut tipos = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
             <Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
             <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
             <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
             <Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>\
             <Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>",
        );
        let mut livro = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
             <workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" \
             xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets>",
        );
        let mut rels = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
             <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
        );
        let mut usados: Vec<String> = Vec::new();
        for (i, aba) in abas.iter().enumerate() {
            let n = i + 1;
            let _ = write!(
                tipos,
                "<Override PartName=\"/xl/worksheets/sheet{n}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>"
            );
            // Nomes de aba repetidos não abrem: numera.
            let mut nome = aba.nome.clone();
            if usados.contains(&nome) {
                nome = format!("{} {n}", nome.chars().take(27).collect::<String>());
            }
            usados.push(nome.clone());
            let _ = write!(
                livro,
                "<sheet name=\"{}\" sheetId=\"{n}\" r:id=\"rId{n}\"/>",
                escapar(&nome)
            );
            let _ = write!(
                rels,
                "<Relationship Id=\"rId{n}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet{n}.xml\"/>"
            );
        }
        tipos.push_str("</Types>");
        livro.push_str("</sheets></workbook>");
        let _ = write!(
            rels,
            "<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/></Relationships>",
            abas.len() + 1
        );
        let raiz = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
             <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
             <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/>\
             </Relationships>";
        let folhas: Vec<(String, Vec<u8>)> = abas
            .iter()
            .enumerate()
            .map(|(i, a)| {
                (
                    format!("xl/worksheets/sheet{}.xml", i + 1),
                    Self::folha(a).into_bytes(),
                )
            })
            .collect();
        let mut arquivos: Vec<(&str, Vec<u8>)> = vec![
            ("[Content_Types].xml", tipos.into_bytes()),
            ("_rels/.rels", raiz.as_bytes().to_vec()),
            ("xl/workbook.xml", livro.into_bytes()),
            ("xl/_rels/workbook.xml.rels", rels.into_bytes()),
            ("xl/styles.xml", ESTILOS.as_bytes().to_vec()),
        ];
        for (nome, dados) in &folhas {
            arquivos.push((nome.as_str(), dados.clone()));
        }
        zip::empacotar(&arquivos)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn colunas_em_letras() {
        assert_eq!(coluna(0), "A");
        assert_eq!(coluna(25), "Z");
        assert_eq!(coluna(26), "AA");
        assert_eq!(coluna(27), "AB");
        assert_eq!(coluna(701), "ZZ");
        assert_eq!(coluna(702), "AAA");
    }

    #[test]
    fn dinheiro_exato_e_texto_escapado() {
        assert_eq!(decimal(Dinheiro::centavos(123_456)), "1234.56");
        assert_eq!(decimal(Dinheiro::centavos(-5)), "-0.05");
        assert_eq!(
            escapar("A & B <c> \"d\"\u{1}"),
            "A &amp; B &lt;c&gt; &quot;d&quot;"
        );
        assert_eq!(Aba::nova("Balancete: 10/2026").nome(), "Balancete 102026");
    }

    #[test]
    fn gera_o_pacote_com_as_partes_e_as_celulas() {
        let mut dre = Aba::nova("DRE").colunas(&[40.0, 16.0]).congelar_cabecalho();
        dre.linha(vec![
            Celula::texto("Conta").negrito(),
            Celula::texto("Valor").negrito(),
        ]);
        dre.linha(vec![
            Celula::texto("Receita & vendas"),
            Celula::dinheiro(Dinheiro::centavos(150_050)),
        ]);
        dre.linha(vec![
            Celula::data(Data::de_ymd(2026, 10, 14).unwrap()),
            Celula::inteiro(7),
        ]);
        let bytes = Planilha::nova().aba(dre).aba(Aba::nova("DRE")).gerar();
        let partes = zip::ler(&bytes);
        let nomes: Vec<&str> = partes.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            nomes,
            [
                "[Content_Types].xml",
                "_rels/.rels",
                "xl/workbook.xml",
                "xl/_rels/workbook.xml.rels",
                "xl/styles.xml",
                "xl/worksheets/sheet1.xml",
                "xl/worksheets/sheet2.xml"
            ]
        );
        let texto = |n: &str| {
            String::from_utf8(partes.iter().find(|(p, _)| p == n).unwrap().1.clone()).unwrap()
        };
        let folha = texto("xl/worksheets/sheet1.xml");
        assert!(folha.contains("<c r=\"A2\" t=\"inlineStr\" s=\"0\"><is><t xml:space=\"preserve\">Receita &amp; vendas</t></is></c>"));
        assert!(folha.contains("<c r=\"B2\" s=\"2\"><v>1500.50</v></c>"));
        // 14/10/2026 = 46309 no calendário do Excel.
        assert!(folha.contains("<c r=\"A3\" s=\"4\"><v>46309</v></c>"));
        assert!(folha.contains("state=\"frozen\""));
        // Abas com o mesmo nome são numeradas.
        assert!(texto("xl/workbook.xml").contains("name=\"DRE 2\""));
    }
}
