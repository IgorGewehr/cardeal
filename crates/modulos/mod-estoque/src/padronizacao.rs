//! Padronização do nome de um produto a partir da descrição do fornecedor (o `xProd` da NF-e)
//! — a versão **determinística** do "template de nomenclatura + dicionário de abreviações" do
//! lunno (§5.6). Domínio puro, sem IA:
//!
//! - cada palavra vira maiúscula só na primeira letra (`COCA-COLA` → `Coca-Cola`), e as
//!   preposições curtas ficam em minúsculo no meio (`de`, `com`, `sem`…);
//! - o **dicionário DE/PARA** da empresa troca palavras (`REFRIG` → `Refrigerante`,
//!   `PO` → `Pó`); um PARA vazio **remove** a palavra (`SABOR` → nada);
//! - o **conteúdo** (`35G`, `2000ML`, `5KG`, `500 G`) é normalizado (`35gr`, `2Lt`, `5Kg`,
//!   `500gr`) e vai para o fim.
//!
//! [`aprender`] é o outro lado: quando o usuário corrige o nome sugerido, os pares que dá para
//! explicar viram termos do dicionário — só quando a palavra nova **começa** com a palavra da
//! nota (abreviação expandida, `FERM` → `Fermento`) ou é a mesma sem acento/caixa (`PO` →
//! `Pó`). Uma palavra inserida, removida ou trocada por outra não ensina nada: na dúvida, não
//! aprende (o dicionário também é editável à mão).

use std::collections::HashMap;

use cardeal_kernel::texto::chave_busca;
use serde::{Deserialize, Serialize};

/// Um termo do dicionário: a palavra como vem na nota (normalizada) e como deve ficar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TermoDicionario {
    /// A palavra da nota, sem caixa nem acento (`refrig`).
    pub de: String,
    /// Como ela vira no nome (`Refrigerante`); vazio = some do nome.
    pub para: String,
}

/// O dicionário DE/PARA da empresa.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Dicionario(HashMap<String, String>);

impl Dicionario {
    /// Monta o dicionário a partir dos termos.
    #[must_use]
    pub fn de_termos(termos: &[TermoDicionario]) -> Self {
        Self(
            termos
                .iter()
                .map(|t| (chave(&t.de), t.para.clone()))
                .collect(),
        )
    }

    fn para(&self, palavra: &str) -> Option<&str> {
        self.0.get(&chave(palavra)).map(String::as_str)
    }
}

/// A chave de uma palavra no dicionário: sem caixa, acento, espaço nem pontuação.
#[must_use]
pub fn chave(palavra: &str) -> String {
    chave_busca(palavra)
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect()
}

const MINUSCULAS: &[&str] = &[
    "de", "da", "do", "das", "dos", "e", "com", "sem", "para", "em",
];

/// `COCA-COLA` → `Coca-Cola`; `TIPO1` → `Tipo1`; preposição no meio fica minúscula.
fn capitalizar(palavra: &str, primeira: bool) -> String {
    let minuscula = palavra.to_lowercase();
    if !primeira && MINUSCULAS.contains(&minuscula.as_str()) {
        return minuscula;
    }
    minuscula
        .split('-')
        .map(|parte| {
            let mut c = parte.chars();
            c.next().map_or_else(String::new, |p| {
                p.to_uppercase().collect::<String>() + c.as_str()
            })
        })
        .collect::<Vec<_>>()
        .join("-")
}

/// A unidade de conteúdo normalizada, ou `None` se não é uma.
fn unidade(sufixo: &str) -> Option<&'static str> {
    Some(match sufixo.to_uppercase().as_str() {
        "G" | "GR" | "GRS" => "gr",
        "KG" | "KGS" => "Kg",
        "ML" => "ml",
        "L" | "LT" | "LTS" => "Lt",
        "M" => "m",
        "CM" => "cm",
        "MM" => "mm",
        _ => return None,
    })
}

/// `2000` ml → `2` (L); `1500` → `1,5`; `350` → `None` (fica em ml).
fn em_milhares(numero: &str) -> Option<String> {
    let n: u64 = numero.parse().ok()?;
    if n < 1000 {
        return None;
    }
    if n % 1000 == 0 {
        return Some((n / 1000).to_string());
    }
    if n % 100 == 0 {
        return Some(format!("{},{}", n / 1000, (n % 1000) / 100));
    }
    None
}

/// `35G` → `35gr`; `2000ML` → `2Lt`; `1000G` → `1Kg`; `12X350ML` → `12x350ml`.
fn conteudo(numero: &str, sufixo: &str) -> Option<String> {
    let un = unidade(sufixo)?;
    let numero = numero.replace('.', ",");
    Some(match (un, em_milhares(&numero)) {
        ("ml", Some(l)) => format!("{l}Lt"),
        ("gr", Some(kg)) => format!("{kg}Kg"),
        (un, _) => format!("{numero}{un}"),
    })
}

/// Separa `35G` em (`35`, `G`); `1,5L` em (`1,5`, `L`).
fn numero_e_sufixo(palavra: &str) -> Option<(&str, &str)> {
    let fim_numero = palavra
        .char_indices()
        .find(|(_, c)| !(c.is_ascii_digit() || *c == ',' || *c == '.'))
        .map_or(palavra.len(), |(i, _)| i);
    let (n, s) = palavra.split_at(fim_numero);
    (!n.is_empty() && n.starts_with(|c: char| c.is_ascii_digit())).then_some((n, s))
}

/// O conteúdo de uma palavra (ou da palavra seguida da próxima, `500 G`): o texto normalizado
/// e quantas palavras ele consumiu.
fn conteudo_em(palavras: &[&str], i: usize) -> Option<(String, usize)> {
    let p = palavras[i];
    // Embalagem: 12X350ML.
    if let Some((qtd, resto)) = p.to_uppercase().split_once('X') {
        if !qtd.is_empty() && qtd.chars().all(|c| c.is_ascii_digit()) {
            let (n, s) = numero_e_sufixo(resto)?;
            return conteudo(n, s).map(|c| (format!("{qtd}x{c}"), 1));
        }
    }
    let (n, s) = numero_e_sufixo(p)?;
    if s.is_empty() {
        let proxima = palavras.get(i + 1)?;
        return conteudo(n, proxima).map(|c| (c, 2));
    }
    conteudo(n, s).map(|c| (c, 1))
}

/// As palavras da descrição e o conteúdo, se houver. Com mais de um, o último é o conteúdo e
/// os anteriores ficam no meio do nome (já normalizados).
fn separar(descricao: &str) -> (Vec<String>, Option<String>) {
    let palavras: Vec<&str> = descricao.split_whitespace().collect();
    let mut resto: Vec<String> = Vec::with_capacity(palavras.len());
    let mut achado: Option<String> = None;
    let mut i = 0;
    while i < palavras.len() {
        if let Some((c, n)) = conteudo_em(&palavras, i) {
            if let Some(anterior) = achado.replace(c) {
                resto.push(anterior);
            }
            i += n;
        } else {
            resto.push(palavras[i].to_owned());
            i += 1;
        }
    }
    (resto, achado)
}

/// O nome sugerido para a descrição do fornecedor.
#[must_use]
pub fn sugerir_nome(descricao: &str, dicionario: &Dicionario) -> String {
    let (palavras, conteudo) = separar(descricao);
    let mut saida: Vec<String> = Vec::with_capacity(palavras.len() + 1);
    for p in &palavras {
        let primeira = saida.is_empty();
        match dicionario.para(p) {
            Some("") => {}
            Some(para) => saida.push(para.to_owned()),
            None => saida.push(capitalizar(p, primeira)),
        }
    }
    if let Some(c) = conteudo {
        saida.push(c);
    }
    saida.join(" ")
}

/// Os termos que a correção do usuário ensina: para cada palavra da nota, a palavra
/// correspondente do nome escolhido, quando ela começa com a da nota (abreviação expandida)
/// ou é a mesma sem caixa/acento — e só se for diferente do que o sistema sugeriria.
#[must_use]
pub fn aprender(descricao: &str, escolhido: &str, dicionario: &Dicionario) -> Vec<TermoDicionario> {
    let (originais, _) = separar(descricao);
    let (novas, _) = separar(escolhido);
    let mut aprendidos: Vec<TermoDicionario> = Vec::new();
    let mut j = 0;
    for (i, original) in originais.iter().enumerate() {
        let k = chave(original);
        if k.is_empty() {
            continue;
        }
        let Some(pos) = novas[j..].iter().position(|n| {
            let kn = chave(n);
            // Uma palavra só (Coca-Cola não é a expansão de COCA) que começa com a da nota.
            !chave_busca(n).contains(' ') && kn.starts_with(&k)
        }) else {
            continue;
        };
        let nova = &novas[j + pos];
        j += pos + 1;
        let sugerida = match dicionario.para(original) {
            Some(p) => p.to_owned(),
            None => capitalizar(original, i == 0),
        };
        if *nova != sugerida && !aprendidos.iter().any(|t| t.de == k) {
            aprendidos.push(TermoDicionario {
                de: k,
                para: nova.clone(),
            });
        }
    }
    aprendidos
}

#[cfg(test)]
mod testes {
    use super::*;

    fn dic(pares: &[(&str, &str)]) -> Dicionario {
        Dicionario::de_termos(
            &pares
                .iter()
                .map(|(de, para)| TermoDicionario {
                    de: (*de).to_owned(),
                    para: (*para).to_owned(),
                })
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn exemplos_do_spec_sem_dicionario() {
        let vazio = Dicionario::default();
        let casos = [
            (
                "FERM BIOLOGICO SECO FLEISCHMANN 10G",
                "Ferm Biologico Seco Fleischmann 10gr",
            ),
            (
                "FEIJAO CARIOCA TIPO1 CANTU 1KG",
                "Feijao Carioca Tipo1 Cantu 1Kg",
            ),
            (
                "ARROZ PARBOILIZADO KIARROZ 5KG",
                "Arroz Parboilizado Kiarroz 5Kg",
            ),
            ("COCA-COLA 2000ML", "Coca-Cola 2Lt"),
            ("GUARANA ANTARCTICA 2000ML", "Guarana Antarctica 2Lt"),
            (
                "FLEISCHMANN MISTURA PARA BOLO CHOC 500G",
                "Fleischmann Mistura para Bolo Choc 500gr",
            ),
        ];
        for (nf, esperado) in casos {
            assert_eq!(sugerir_nome(nf, &vazio), esperado, "{nf}");
        }
    }

    #[test]
    fn dicionario_expande_acentua_e_remove() {
        let d = dic(&[
            ("ferm", "Fermento"),
            ("po", "Pó"),
            ("sabor", ""),
            ("REFRIG", "Refrigerante"),
            ("feijao", "Feijão"),
        ]);
        assert_eq!(
            sugerir_nome("GELATINA PO SABOR MORANGO APTI 35G", &d),
            "Gelatina Pó Morango Apti 35gr"
        );
        assert_eq!(
            sugerir_nome("FERM BIOLOGICO SECO FLEISCHMANN 10G", &d),
            "Fermento Biologico Seco Fleischmann 10gr"
        );
        assert_eq!(
            sugerir_nome("REFRIG COLA PET 2L", &d),
            "Refrigerante Cola Pet 2Lt"
        );
        assert_eq!(sugerir_nome("feijão carioca 1kg", &d), "Feijão Carioca 1Kg");
    }

    #[test]
    fn conteudo_normalizado_e_no_fim() {
        let vazio = Dicionario::default();
        assert_eq!(
            sugerir_nome("LEITE 1000ML INTEGRAL", &vazio),
            "Leite Integral 1Lt"
        );
        assert_eq!(sugerir_nome("OLEO SOJA 900 ML", &vazio), "Oleo Soja 900ml");
        assert_eq!(sugerir_nome("SUCO 1500ML", &vazio), "Suco 1,5Lt");
        assert_eq!(sugerir_nome("ACUCAR 1000G", &vazio), "Acucar 1Kg");
        assert_eq!(sugerir_nome("AGUA 1,5L", &vazio), "Agua 1,5Lt");
        assert_eq!(sugerir_nome("CERVEJA 12X350ML", &vazio), "Cerveja 12x350ml");
        // Número sem unidade não é conteúdo.
        assert_eq!(
            sugerir_nome("PARAFUSO 3 MOD 10", &vazio),
            "Parafuso 3 Mod 10"
        );
    }

    #[test]
    fn aprende_so_o_que_da_para_explicar() {
        let vazio = Dicionario::default();
        let nf = "FERM BIOLOGICO SECO FLEISCHMANN 10G";
        let aprendidos = aprender(nf, "Fermento Biológico Seco Fleischmann 10gr", &vazio);
        assert_eq!(
            aprendidos,
            [
                TermoDicionario {
                    de: "ferm".into(),
                    para: "Fermento".into()
                },
                TermoDicionario {
                    de: "biologico".into(),
                    para: "Biológico".into()
                },
            ]
        );
        // Inserir "Refrigerante" e juntar "Coca-Cola" não ensina nada (não dá para explicar).
        assert!(aprender("COCA COLA 2000ML", "Refrigerante Coca-Cola 2Lt", &vazio).is_empty());
        // Aceitar a sugestão como veio também não.
        let sugerido = sugerir_nome(nf, &vazio);
        assert!(aprender(nf, &sugerido, &vazio).is_empty());
    }

    #[test]
    fn o_que_aprendeu_vale_na_proxima_sugestao() {
        let nf = "FEIJAO CARIOCA TIPO1 CANTU 1KG";
        let d = Dicionario::de_termos(&aprender(
            nf,
            "Feijão Carioca Tipo1 Cantu 1Kg",
            &Dicionario::default(),
        ));
        assert_eq!(
            sugerir_nome("FEIJAO PRETO CANTU 1KG", &d),
            "Feijão Preto Cantu 1Kg"
        );
    }
}
