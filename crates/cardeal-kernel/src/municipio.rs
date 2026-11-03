//! O código IBGE do município — o `cMun` da nota fiscal — a partir da UF e do nome, sem depender
//! de rede: a tabela do IBGE vai embutida (`municipios_dados`). O nome é comparado sem acento,
//! caixa nem pontuação: "São Paulo", "sao paulo" e "SAO PAULO" dão o mesmo; "D'Oeste" e
//! "d oeste" também.

use crate::documento::Uf;
use crate::municipios_dados::MUNICIPIOS;
use crate::texto::chave_busca;

/// Os municípios de uma UF (os códigos começam pelo código IBGE dela).
fn da_uf(uf: Uf) -> &'static [(u32, &'static str)] {
    let inicio = u32::from(uf.codigo_ibge()) * 100_000;
    let fim = inicio + 100_000;
    let a = MUNICIPIOS.partition_point(|(c, _)| *c < inicio);
    let b = MUNICIPIOS.partition_point(|(c, _)| *c < fim);
    &MUNICIPIOS[a..b]
}

/// O código IBGE (7 dígitos) do município `nome` na `uf` — `None` se não há um com esse nome.
#[must_use]
pub fn codigo_do_municipio(uf: Uf, nome: &str) -> Option<u32> {
    let alvo = chave_busca(nome);
    if alvo.is_empty() {
        return None;
    }
    da_uf(uf)
        .iter()
        .find(|(_, n)| chave_busca(n) == alvo)
        .map(|(c, _)| *c)
}

/// A UF e o nome oficial do município de código `codigo`.
#[must_use]
pub fn municipio(codigo: u32) -> Option<(Uf, &'static str)> {
    let i = MUNICIPIOS.binary_search_by_key(&codigo, |(c, _)| *c).ok()?;
    let uf = Uf::de_codigo_ibge(u8::try_from(codigo / 100_000).ok()?).ok()?;
    Some((uf, MUNICIPIOS[i].1))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn acha_pelo_nome_sem_acento_nem_caixa() {
        assert_eq!(codigo_do_municipio(Uf::Sp, "São Paulo"), Some(3_550_308));
        assert_eq!(
            codigo_do_municipio(Uf::Sp, " sao   PAULO "),
            Some(3_550_308)
        );
        assert_eq!(codigo_do_municipio(Uf::Rs, "Porto Alegre"), Some(4_314_902));
        assert_eq!(
            codigo_do_municipio(Uf::Ro, "alta floresta d'oeste"),
            Some(1_100_015)
        );
        // O nome existe, mas noutra UF; e o que não existe.
        assert_eq!(codigo_do_municipio(Uf::Rs, "São Paulo"), None);
        assert_eq!(codigo_do_municipio(Uf::Sc, "Cidade Que Não Existe"), None);
        assert_eq!(codigo_do_municipio(Uf::Sc, ""), None);
    }

    #[test]
    fn a_tabela_e_inteira_e_ordenada() {
        assert!(MUNICIPIOS.len() > 5_500);
        assert!(MUNICIPIOS.windows(2).all(|w| w[0].0 < w[1].0));
        assert_eq!(municipio(4_314_902), Some((Uf::Rs, "Porto Alegre")));
        assert_eq!(municipio(5_300_108), Some((Uf::Df, "Brasília")));
        assert_eq!(municipio(1), None);
        // Toda UF tem municípios.
        for uf in [Uf::Ac, Uf::Df, Uf::Rr, Uf::Sp, Uf::To] {
            assert!(!da_uf(uf).is_empty(), "{uf:?}");
        }
    }
}
