//! Os NCM em que o PIS/COFINS é **monofásico** (recolhido pelo fabricante ou importador; quem
//! revende não paga): a sugestão que a nota de compra usa quando o fornecedor não informou o
//! CST 04. Lista por prefixo das leis — **parcial**, só os grupos mais comuns no varejo; o CST
//! que vem na nota vale mais que ela. Conferir com o contador.
//!
//! - Lei 10.147/2000: perfumaria, toucador e higiene pessoal (33.03 a 33.05, 33.07, 3401.11.90,
//!   3401.20.10, 9603.21.00) e medicamentos (30.01, 30.03, 30.04, 3002.10/20/90, 3005.10.10,
//!   3006.30, 3006.60);
//! - Lei 10.485/2002: pneus e câmaras (40.11, 40.13) e autopeças dos Anexos I e II (os capítulos
//!   87.08 e 84.07/84.08 mais comuns);
//! - Lei 13.097/2015: bebidas frias (22.01, 22.02, 22.03, 2106.90.10);
//! - Lei 9.718/1998 e 10.865/2004: combustíveis (27.10, 27.11, 2207.10, 2207.20).

const PREFIXOS: &[&str] = &[
    // higiene pessoal e perfumaria
    "3303", "3304", "3305", "3307", "34011190", "34012010", "96032100", // medicamentos
    "3001", "3003", "3004", "300210", "300220", "300290", "30051010", "300630", "300660",
    // pneus e autopeças
    "4011", "4013", "8708", "8407", "8408", // bebidas frias
    "2201", "2202", "2203", "21069010", // combustíveis
    "2710", "2711", "220710", "220720",
];

/// O NCM (8 dígitos, com ou sem pontos) está na lista de monofásicos.
#[must_use]
pub fn ncm_monofasico(ncm: &str) -> bool {
    let digitos: String = ncm.chars().filter(char::is_ascii_digit).collect();
    digitos.len() == 8 && PREFIXOS.iter().any(|p| digitos.starts_with(p))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn reconhece_os_grupos_mais_comuns() {
        assert!(ncm_monofasico("3305.10.00")); // xampu
        assert!(ncm_monofasico("22030000")); // cerveja
        assert!(ncm_monofasico("87089990")); // autopeça
        assert!(!ncm_monofasico("19053100")); // biscoito
        assert!(!ncm_monofasico("3305")); // NCM incompleto não conta
    }
}
