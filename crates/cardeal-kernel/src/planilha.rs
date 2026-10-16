//! A planilha CSV que as importações leem (produtos, estrutura, clientes, contas em aberto):
//! `;` ou `,` (o que aparecer mais fora de aspas na primeira linha com conteúdo), aspas duplas
//! protegem o separador e `""` é uma aspa, linhas em branco somem e a marca de UTF-8 do Excel
//! (`\u{feff}`) é ignorada. Os títulos das colunas se comparam por [`chave_coluna`].

use crate::texto::chave_busca;

/// Uma linha lida: o número no arquivo (a partir de 1) e as células, já sem espaço nas pontas.
pub type LinhaLida = (usize, Vec<String>);

/// Separa o texto em linhas e células.
///
/// ```
/// use cardeal_kernel::planilha::ler_linhas;
///
/// let linhas = ler_linhas("\u{feff}Nome;Cidade\n\"Silva; Ana\";Porto Alegre\n\n");
/// assert_eq!(linhas[1], (2, vec!["Silva; Ana".to_string(), "Porto Alegre".to_string()]));
/// ```
#[must_use]
pub fn ler_linhas(texto: &str) -> Vec<LinhaLida> {
    let texto = texto.trim_start_matches('\u{feff}');
    let separador = texto
        .lines()
        .find(|l| !l.trim().is_empty())
        .map_or(';', |l| {
            let fora_de_aspas = |c: char| {
                let mut dentro = false;
                l.chars()
                    .filter(|x| {
                        if *x == '"' {
                            dentro = !dentro;
                        }
                        !dentro && *x == c
                    })
                    .count()
            };
            if fora_de_aspas(',') > fora_de_aspas(';') {
                ','
            } else {
                ';'
            }
        });
    texto
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .map(|(i, l)| (i + 1, celulas(l, separador)))
        .collect()
}

fn celulas(linha: &str, separador: char) -> Vec<String> {
    let mut saida = Vec::new();
    let mut atual = String::new();
    let mut dentro = false;
    let mut chars = linha.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if dentro && chars.peek() == Some(&'"') => {
                atual.push('"');
                chars.next();
            }
            '"' => dentro = !dentro,
            c if c == separador && !dentro => {
                saida.push(atual.trim().to_owned());
                atual.clear();
            }
            c => atual.push(c),
        }
    }
    saida.push(atual.trim().to_owned());
    saida
}

/// O título de uma coluna para comparar: sem caixa, acento, espaço nem pontuação
/// ("Código de barras" → `codigodebarras`, "E-mail" → `email`).
#[must_use]
pub fn chave_coluna(titulo: &str) -> String {
    chave_busca(titulo)
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn virgula_ou_ponto_e_virgula_aspas_e_bom() {
        let l = ler_linhas("a,b,\"c,d\"\n1,\"2 \"\"x\"\"\",3\n");
        assert_eq!(l[0].1, vec!["a", "b", "c,d"]);
        assert_eq!(l[1].1, vec!["1", "2 \"x\"", "3"]);
        assert_eq!(chave_coluna("  Código de Barras "), "codigodebarras");
        assert_eq!(chave_coluna("E-mail"), "email");
        assert_eq!(chave_coluna("CPF/CNPJ"), "cpfcnpj");
    }
}
