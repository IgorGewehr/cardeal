//! O XML que o Cardeal escreve para o Sistema Nacional NFS-e já sai **canônico** (C14N 1.0
//! inclusivo): sem espaços entre elementos, sem elemento vazio abreviado, atributos na ordem do
//! C14N e só os escapes que o C14N usa. Assim o resumo (`DigestValue`) é calculado sobre os
//! mesmos bytes que o verificador da Sefin reconstrói — sem um canonicalizador genérico.

/// Um escritor mínimo de XML canônico.
#[derive(Debug, Default)]
pub(crate) struct Xml {
    texto: String,
}

impl Xml {
    pub(crate) fn novo() -> Self {
        Self::default()
    }

    /// `<nome a="v" ...>` — os atributos já na ordem do C14N (`xmlns` primeiro, depois os sem
    /// namespace em ordem alfabética).
    pub(crate) fn abrir(&mut self, nome: &str, atributos: &[(&str, &str)]) -> &mut Self {
        self.texto.push('<');
        self.texto.push_str(nome);
        for (chave, valor) in atributos {
            self.texto.push(' ');
            self.texto.push_str(chave);
            self.texto.push_str("=\"");
            escapar_atributo(valor, &mut self.texto);
            self.texto.push('"');
        }
        self.texto.push('>');
        self
    }

    pub(crate) fn fechar(&mut self, nome: &str) -> &mut Self {
        self.texto.push_str("</");
        self.texto.push_str(nome);
        self.texto.push('>');
        self
    }

    /// `<nome>texto</nome>`.
    pub(crate) fn folha(&mut self, nome: &str, texto: &str) -> &mut Self {
        self.abrir(nome, &[]);
        escapar_texto(texto, &mut self.texto);
        self.fechar(nome)
    }

    /// A folha, só se houver valor.
    pub(crate) fn talvez(&mut self, nome: &str, texto: Option<&str>) -> &mut Self {
        if let Some(t) = texto {
            self.folha(nome, t);
        }
        self
    }

    pub(crate) fn texto(self) -> String {
        self.texto
    }
}

/// Os escapes de texto do C14N: `&`, `<`, `>` e o `\r`.
fn escapar_texto(s: &str, saida: &mut String) {
    for c in s.chars() {
        match c {
            '&' => saida.push_str("&amp;"),
            '<' => saida.push_str("&lt;"),
            '>' => saida.push_str("&gt;"),
            '\r' => saida.push_str("&#xD;"),
            outro => saida.push(outro),
        }
    }
}

/// Os escapes de atributo do C14N: `&`, `<`, `"`, `\t`, `\n`, `\r`.
fn escapar_atributo(s: &str, saida: &mut String) {
    for c in s.chars() {
        match c {
            '&' => saida.push_str("&amp;"),
            '<' => saida.push_str("&lt;"),
            '"' => saida.push_str("&quot;"),
            '\t' => saida.push_str("&#x9;"),
            '\n' => saida.push_str("&#xA;"),
            '\r' => saida.push_str("&#xD;"),
            outro => saida.push(outro),
        }
    }
}

/// O texto como os tipos `TSString`/`TSStringComQuebraDeLinha` do XSD aceitam: só caracteres
/// de U+0020 a U+00FF (a pontuação tipográfica comum vira a equivalente ASCII; o resto sai),
/// sem espaço nas pontas, até `maximo` caracteres. Com `quebras`, mantém `\n` (o `\r` sai —
/// o leitor de XML o normalizaria e o resumo mudaria). Vazio vira `None`.
pub(crate) fn texto_xsd(s: &str, maximo: usize, quebras: bool) -> Option<String> {
    let mut saida = String::with_capacity(s.len());
    for c in s.chars() {
        let trocado = match c {
            '\u{2013}' | '\u{2014}' | '\u{2212}' | '\u{2022}' | '\u{00B7}' => Some('-'),
            '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{2032}' => Some('\''),
            '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{2033}' => Some('"'),
            '\u{2026}' => {
                saida.push_str("...");
                None
            }
            '\n' if quebras => Some('\n'),
            '\r' if quebras => None,
            '\u{00A0}' | '\u{2007}' | '\u{202F}' | '\t' | '\n' | '\r' => Some(' '),
            c if ('\u{20}'..='\u{7E}').contains(&c) || ('\u{A1}'..='\u{FF}').contains(&c) => {
                Some(c)
            }
            _ => None,
        };
        if let Some(t) = trocado {
            saida.push(t);
        }
    }
    let limpo: String = saida.trim().chars().take(maximo).collect();
    let limpo = limpo.trim_end().to_string();
    (!limpo.is_empty()).then_some(limpo)
}

/// Só os dígitos.
pub(crate) fn digitos(s: &str) -> String {
    s.chars().filter(char::is_ascii_digit).collect()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn escapa_como_o_c14n() {
        let mut x = Xml::novo();
        x.abrir("a", &[("Id", "x\"<&")])
            .folha("b", "1 < 2 & 3 > 0 \"ok\"")
            .fechar("a");
        assert_eq!(
            x.texto(),
            "<a Id=\"x&quot;&lt;&amp;\"><b>1 &lt; 2 &amp; 3 &gt; 0 \"ok\"</b></a>"
        );
    }

    #[test]
    fn o_texto_fica_no_que_o_xsd_aceita() {
        assert_eq!(
            texto_xsd("  Troca de tela – iPhone 13 “Pro”… ✅  ", 200, false).as_deref(),
            Some("Troca de tela - iPhone 13 \"Pro\"...")
        );
        assert_eq!(
            texto_xsd("linha 1\r\nlinha 2", 50, true).as_deref(),
            Some("linha 1\nlinha 2")
        );
        assert_eq!(
            texto_xsd("linha 1\r\nlinha 2", 50, false).as_deref(),
            Some("linha 1  linha 2")
        );
        assert_eq!(
            texto_xsd("Ação é útil", 50, false).as_deref(),
            Some("Ação é útil")
        );
        assert_eq!(texto_xsd(" 🔧 ", 50, false), None);
        assert_eq!(texto_xsd("abcdef  ", 4, false).as_deref(), Some("abcd"));
        assert_eq!(
            texto_xsd("ab  cdef", 3, false).as_deref(),
            Some("ab"),
            "sem espaço no fim"
        );
    }
}
