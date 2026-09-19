//! Interpretação do que o operador digita ou bipa no campo do PDV — lógica pura, sem UI.
//!
//! O leitor de código de barras "digita" o código e manda `Enter`; o operador também pode
//! digitar parte do nome. O mesmo campo atende os dois (`docs/modulos/pdv.md` §10), então a
//! tela precisa distinguir **código** de **busca**. E o prefixo `N*` (`3*789…`, `0,450*pao`)
//! é a quantidade — o jeito clássico de PDV de vender vários sem tocar no mouse.

use cardeal_kernel::texto::somente_digitos;

/// Comprimentos válidos de um GTIN (EAN-8, UPC-A, EAN-13, GTIN-14).
const TAMANHOS_GTIN: [usize; 4] = [8, 12, 13, 14];

/// O que o texto do campo significa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entrada {
    /// Nada digitado (ou só um prefixo `N*` sem produto ainda).
    Vazia,
    /// Um código de barras — só dígitos, com comprimento de GTIN. Vem do leitor.
    Codigo {
        /// O prefixo `N*`, como digitado, se houver.
        quantidade: Option<String>,
        /// O código, só dígitos.
        gtin: String,
    },
    /// Texto livre: busca por nome.
    Busca {
        /// O prefixo `N*`, como digitado, se houver.
        quantidade: Option<String>,
        /// O termo de busca.
        termo: String,
    },
}

impl Entrada {
    /// O prefixo de quantidade, se o texto tinha um.
    #[must_use]
    pub fn quantidade(&self) -> Option<&str> {
        match self {
            Self::Vazia => None,
            Self::Codigo { quantidade, .. } | Self::Busca { quantidade, .. } => {
                quantidade.as_deref()
            }
        }
    }
}

/// Interpreta o texto do campo.
#[must_use]
pub fn interpretar(texto: &str) -> Entrada {
    let texto = texto.trim();
    let (quantidade, resto) = separar_quantidade(texto);
    let resto = resto.trim();
    if resto.is_empty() {
        return Entrada::Vazia;
    }
    let digitos = somente_digitos(resto);
    let so_digitos = resto
        .chars()
        .all(|c| c.is_ascii_digit() || c.is_whitespace());
    if so_digitos && TAMANHOS_GTIN.contains(&digitos.len()) {
        return Entrada::Codigo {
            quantidade,
            gtin: digitos,
        };
    }
    Entrada::Busca {
        quantidade,
        termo: resto.to_owned(),
    }
}

/// Separa um prefixo `N*` (N numérico, com `,` ou `.` opcional). Se o que vem antes do `*`
/// não parece uma quantidade, não é prefixo — o `*` fica no texto.
fn separar_quantidade(texto: &str) -> (Option<String>, &str) {
    let Some((antes, depois)) = texto.split_once('*') else {
        return (None, texto);
    };
    let antes = antes.trim();
    let parece_quantidade = !antes.is_empty()
        && antes.chars().any(|c| c.is_ascii_digit())
        && antes
            .chars()
            .all(|c| c.is_ascii_digit() || c == ',' || c == '.');
    if parece_quantidade {
        (Some(antes.to_owned()), depois)
    } else {
        (None, texto)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn vazio_e_so_espacos() {
        assert_eq!(interpretar(""), Entrada::Vazia);
        assert_eq!(interpretar("   "), Entrada::Vazia);
    }

    #[test]
    fn ean13_do_leitor_e_codigo() {
        assert_eq!(
            interpretar("7891234567895"),
            Entrada::Codigo {
                quantidade: None,
                gtin: "7891234567895".to_owned()
            }
        );
    }

    #[test]
    fn leitor_que_manda_espaco_ou_quebra_de_linha_ainda_e_codigo() {
        assert!(matches!(
            interpretar("  7891234567895\n"),
            Entrada::Codigo { .. }
        ));
    }

    #[test]
    fn todos_os_comprimentos_de_gtin() {
        for n in [8, 12, 13, 14] {
            let cod = "1".repeat(n);
            assert!(matches!(interpretar(&cod), Entrada::Codigo { .. }), "{n}");
        }
    }

    #[test]
    fn numero_curto_e_busca_nao_codigo() {
        // "12" pode ser parte de um nome ("Coca 2L"); só um comprimento de GTIN é código.
        assert!(matches!(interpretar("12"), Entrada::Busca { .. }));
        assert!(matches!(interpretar("1234567"), Entrada::Busca { .. })); // 7 dígitos
        assert!(matches!(interpretar("123456789"), Entrada::Busca { .. })); // 9 dígitos
    }

    #[test]
    fn texto_e_busca() {
        assert_eq!(
            interpretar("refri cola"),
            Entrada::Busca {
                quantidade: None,
                termo: "refri cola".to_owned()
            }
        );
    }

    #[test]
    fn prefixo_de_quantidade_com_codigo() {
        let e = interpretar("3*7891234567895");
        assert_eq!(e.quantidade(), Some("3"));
        assert!(matches!(e, Entrada::Codigo { .. }));
    }

    #[test]
    fn prefixo_decimal_de_balanca() {
        let e = interpretar("0,450*pao");
        assert_eq!(e.quantidade(), Some("0,450"));
        assert!(matches!(e, Entrada::Busca { termo, .. } if termo == "pao"));
    }

    #[test]
    fn prefixo_sem_produto_ainda_e_vazio() {
        assert_eq!(interpretar("3*"), Entrada::Vazia);
    }

    #[test]
    fn asterisco_que_nao_e_quantidade_fica_no_texto() {
        // "a*b" não tem número antes do `*`: é busca literal, não prefixo.
        assert_eq!(
            interpretar("a*b"),
            Entrada::Busca {
                quantidade: None,
                termo: "a*b".to_owned()
            }
        );
    }
}
