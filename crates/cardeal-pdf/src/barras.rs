//! O código de barras **Code 128, conjunto C** — o da chave de acesso no DANFE (44 dígitos em 22
//! pares). Devolve as larguras dos módulos (barra, espaço, barra…), para quem desenha.

/// Os padrões dos 107 símbolos do Code 128: a largura de cada barra e espaço, em módulos (barra
/// primeiro). Cada um soma 11 módulos; o de parada (106), 13.
const PADROES: [&str; 107] = [
    "212222", "222122", "222221", "121223", "121322", "131222", "122213", "122312", "132212",
    "221213", "221312", "231212", "112232", "122132", "122231", "113222", "123122", "123221",
    "223211", "221132", "221231", "213212", "223112", "312131", "311222", "321122", "321221",
    "312212", "322112", "322211", "212123", "212321", "232121", "111323", "131123", "131321",
    "112313", "132113", "132311", "211313", "231113", "231311", "112133", "112331", "132131",
    "113123", "113321", "133121", "313121", "211331", "231131", "213113", "213311", "213131",
    "311123", "311321", "331121", "312113", "312311", "332111", "314111", "221411", "431111",
    "111224", "111422", "121124", "121421", "141122", "141221", "112214", "112412", "122114",
    "122411", "142112", "142211", "241211", "221114", "413111", "241112", "134111", "111242",
    "121142", "121241", "114212", "124112", "124211", "411212", "421112", "421211", "212141",
    "214121", "412121", "111143", "111341", "131141", "114113", "114311", "411113", "411311",
    "113141", "114131", "311141", "411131", "211412", "211214", "211232", "2331112",
];

const INICIO_C: usize = 105;
const PARADA: usize = 106;

/// As larguras dos módulos de `digitos` em Code 128C (início C, os pares, o dígito verificador,
/// a parada) — `None` se não for uma quantidade par de dígitos.
#[must_use]
pub(crate) fn code128c(digitos: &str) -> Option<Vec<u8>> {
    if digitos.is_empty() || digitos.len() % 2 != 0 || !digitos.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let pares: Vec<usize> = digitos
        .as_bytes()
        .chunks(2)
        .map(|p| usize::from(p[0] - b'0') * 10 + usize::from(p[1] - b'0'))
        .collect();
    let soma = pares
        .iter()
        .enumerate()
        .fold(INICIO_C, |acc, (i, v)| acc + (i + 1) * v);
    let mut simbolos = Vec::with_capacity(pares.len() + 3);
    simbolos.push(INICIO_C);
    simbolos.extend(pares);
    simbolos.push(soma % 103);
    simbolos.push(PARADA);
    Some(
        simbolos
            .into_iter()
            .flat_map(|s| PADROES[s].bytes().map(|b| b - b'0'))
            .collect(),
    )
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn os_padroes_sao_validos_e_distintos() {
        for (i, p) in PADROES.iter().enumerate() {
            let soma: u32 = p.bytes().map(|b| u32::from(b - b'0')).sum();
            assert_eq!(soma, if i == PARADA { 13 } else { 11 }, "símbolo {i}");
            assert!(p.bytes().all(|b| (b'1'..=b'4').contains(&b)), "símbolo {i}");
        }
        let mut unicos = PADROES.to_vec();
        unicos.sort_unstable();
        unicos.dedup();
        assert_eq!(unicos.len(), PADROES.len());
    }

    #[test]
    fn codifica_a_chave_com_o_verificador() {
        // "1234": 105 + 1×12 + 2×34 = 185 → verificador 82.
        let m = code128c("1234").unwrap();
        let simbolo = |i: usize| -> Vec<u8> { m[i * 6..i * 6 + 6].to_vec() };
        let padrao = |s: usize| -> Vec<u8> { PADROES[s].bytes().map(|b| b - b'0').collect() };
        assert_eq!(simbolo(0), padrao(105));
        assert_eq!(simbolo(1), padrao(12));
        assert_eq!(simbolo(2), padrao(34));
        assert_eq!(simbolo(3), padrao(82));
        assert_eq!(&m[24..], padrao(106).as_slice());
        // A chave: 22 pares + início + verificador + parada = 25 símbolos, 24 × 11 + 13 módulos.
        let chave = code128c(&"4".repeat(44)).unwrap();
        let modulos: u32 = chave.iter().map(|&w| u32::from(w)).sum();
        assert_eq!(modulos, 24 * 11 + 13);
        assert_eq!(code128c("123"), None);
        assert_eq!(code128c("12a4"), None);
    }
}
