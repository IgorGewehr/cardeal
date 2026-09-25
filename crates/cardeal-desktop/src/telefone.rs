//! Telefone de cliente: comparar dois números digitados de jeitos diferentes e achar quem já
//! está cadastrado com ele — o aviso "já existe Maria com esse número" que evita cliente
//! duplicado no balcão.

use mod_clientes::ItemPessoa;

/// Os dois números são o mesmo celular/telefone? Compara os 8 últimos dígitos (ignora DDI,
/// DDD e o 9 da frente quando um tem e o outro não). Menos de 8 dígitos em qualquer um: não.
pub fn mesmo_numero(a: &str, b: &str) -> bool {
    let (a, b) = (
        cardeal_kernel::texto::somente_digitos(a),
        cardeal_kernel::texto::somente_digitos(b),
    );
    a.len() >= 8 && b.len() >= 8 && a[a.len() - 8..] == b[b.len() - 8..]
}

/// A primeira pessoa do catálogo com esse telefone, se houver.
pub fn quem_tem<'a>(catalogo: &'a [ItemPessoa], telefone: &str) -> Option<&'a ItemPessoa> {
    catalogo.iter().find(|c| {
        c.telefone
            .as_deref()
            .is_some_and(|t| mesmo_numero(t, telefone))
    })
}

/// "(31) 98888-7777" a partir do que estiver gravado; devolve como veio se não reconhecer.
pub fn formatar(telefone: &str) -> String {
    let d = cardeal_kernel::texto::somente_digitos(telefone);
    let d = d.strip_prefix("55").filter(|r| r.len() >= 10).unwrap_or(&d);
    match d.len() {
        11 => format!("({}) {}-{}", &d[..2], &d[2..7], &d[7..]),
        10 => format!("({}) {}-{}", &d[..2], &d[2..6], &d[6..]),
        _ => telefone.to_owned(),
    }
}

/// O link `wa.me` para o telefone (com DDI 55 quando vier só DDD + número) já com o texto.
/// `None` quando o telefone não tem dígitos suficientes para ser um celular.
pub fn link_whatsapp(telefone: &str, texto: &str) -> Option<String> {
    let digitos = cardeal_kernel::texto::somente_digitos(telefone);
    let numero = match digitos.len() {
        10 | 11 => format!("55{digitos}"),
        12 | 13 if digitos.starts_with("55") => digitos,
        _ => return None,
    };
    Some(format!(
        "https://wa.me/{numero}?text={}",
        codificar_url(texto)
    ))
}

/// Codificação de URL (RFC 3986): mantém letras, dígitos e `-._~`; o resto vira `%XX` por
/// byte UTF-8.
fn codificar_url(texto: &str) -> String {
    let mut saida = String::with_capacity(texto.len() * 3);
    for b in texto.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            saida.push(char::from(b));
        } else {
            saida.push_str(&format!("%{b:02X}"));
        }
    }
    saida
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn mesmo_numero_ignora_mascara_ddi_e_ddd() {
        assert!(mesmo_numero("(31) 98888-7777", "+55 31 988887777"));
        assert!(mesmo_numero("98888-7777", "31 98888 7777"));
        assert!(!mesmo_numero("(31) 98888-7777", "(31) 98888-7778"));
        assert!(!mesmo_numero("7777", "7777"));
    }

    #[test]
    fn link_whatsapp_poe_ddi_e_codifica_o_texto() {
        assert_eq!(
            link_whatsapp("(31) 99999-8888", "Olá, Zé!").as_deref(),
            Some("https://wa.me/5531999998888?text=Ol%C3%A1%2C%20Z%C3%A9%21")
        );
        assert_eq!(
            link_whatsapp("+55 31 99999-8888", "x").as_deref(),
            Some("https://wa.me/5531999998888?text=x")
        );
        assert!(link_whatsapp("1234", "x").is_none());
    }

    #[test]
    fn telefone_formatado_com_ddd() {
        assert_eq!(formatar("31988887777"), "(31) 98888-7777");
        assert_eq!(formatar("+55 31 3333-4444"), "(31) 3333-4444");
        assert_eq!(formatar("ramal 12"), "ramal 12");
    }
}
