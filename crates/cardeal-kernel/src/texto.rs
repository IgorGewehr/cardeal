//! Utilidades de texto para busca, deduplicação e proteção de dados pessoais.
//!
//! Nada aqui é "genérico": cada função existe porque um problema real do ERP a exigiu.
//! A busca de produto no PDV precisa achar "acucar" quando o cadastro diz "Açúcar";
//! a entrada de nota precisa casar "COCA COLA 2L" com "Coca-Cola 2 Litros"; e o logger
//! precisa nunca gravar um CPF.

use std::collections::HashSet;

/// Remove acentos e cedilha, mantendo a letra base.
///
/// Cobre o conjunto que aparece em português; não é um normalizador Unicode completo
/// (que exigiria uma tabela de dezenas de milhares de pontos de código, contra o Pilar I).
#[must_use]
pub fn sem_acentos(texto: &str) -> String {
    texto
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'ã' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'õ' | 'ô' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            'Á' | 'À' | 'Ã' | 'Â' | 'Ä' => 'A',
            'É' | 'È' | 'Ê' | 'Ë' => 'E',
            'Í' | 'Ì' | 'Î' | 'Ï' => 'I',
            'Ó' | 'Ò' | 'Õ' | 'Ô' | 'Ö' => 'O',
            'Ú' | 'Ù' | 'Û' | 'Ü' => 'U',
            'Ç' => 'C',
            'Ñ' => 'N',
            outro => outro,
        })
        .collect()
}

/// Forma canônica para busca e comparação: sem acento, minúscula, sem pontuação,
/// com espaços colapsados.
///
/// ```
/// # use cardeal_kernel::texto::chave_busca;
/// assert_eq!(chave_busca("  Açúcar   Cristal 1kg "), "acucar cristal 1kg");
/// assert_eq!(chave_busca("Coca-Cola 2L"), "coca cola 2l");
/// ```
#[must_use]
pub fn chave_busca(texto: &str) -> String {
    let sem = sem_acentos(texto).to_lowercase();
    let mut saida = String::with_capacity(sem.len());
    let mut espaco_pendente = false;
    for c in sem.chars() {
        if c.is_alphanumeric() {
            if espaco_pendente && !saida.is_empty() {
                saida.push(' ');
            }
            espaco_pendente = false;
            saida.push(c);
        } else {
            espaco_pendente = true;
        }
    }
    saida
}

/// Os trigramas de um texto já normalizado, com marcação de início e fim.
fn trigramas(texto: &str) -> HashSet<[char; 3]> {
    let marcado: Vec<char> = std::iter::once(' ')
        .chain(std::iter::once(' '))
        .chain(texto.chars())
        .chain(std::iter::once(' '))
        .collect();
    marcado.windows(3).map(|j| [j[0], j[1], j[2]]).collect()
}

/// Similaridade entre dois textos, de 0 a 1, pelo índice de Jaccard sobre trigramas.
///
/// É o algoritmo usado no casamento de produto na entrada de nota fiscal
/// (`docs/10-modulo-fiscal.md` §5). O limiar adotado é **0,82**: acima disso o sistema
/// sugere o vínculo; abaixo, pergunta ao usuário e aprende com a resposta.
///
/// ```
/// # use cardeal_kernel::texto::similaridade;
/// assert!(similaridade("Coca-Cola 2L", "COCA COLA 2 L") > 0.5);
/// assert!(similaridade("Arroz Tipo 1", "Feijão Preto") < 0.2);
/// ```
#[must_use]
pub fn similaridade(a: &str, b: &str) -> f32 {
    let na = chave_busca(a);
    let nb = chave_busca(b);
    if na == nb {
        return 1.0;
    }
    if na.is_empty() || nb.is_empty() {
        return 0.0;
    }
    let ta = trigramas(&na);
    let tb = trigramas(&nb);
    let intersecao = ta.intersection(&tb).count();
    let uniao = ta.union(&tb).count();
    if uniao == 0 {
        0.0
    } else {
        #[allow(clippy::cast_precision_loss)]
        {
            intersecao as f32 / uniao as f32
        }
    }
}

/// Verdadeiro se `agulha` aparece em `palheiro`, ignorando acento e caixa.
/// É o casamento usado nas caixas de busca da interface.
#[must_use]
pub fn contem_normalizado(palheiro: &str, agulha: &str) -> bool {
    if agulha.is_empty() {
        return true;
    }
    chave_busca(palheiro).contains(&chave_busca(agulha))
}

/// Verdadeiro se todas as palavras de `termo` aparecem em `texto`, em qualquer ordem.
///
/// É o comportamento que o usuário espera: digitar "cristal acucar" acha "Açúcar Cristal".
#[must_use]
pub fn casa_por_palavras(texto: &str, termo: &str) -> bool {
    let alvo = chave_busca(texto);
    chave_busca(termo)
        .split_whitespace()
        .all(|p| alvo.contains(p))
}

/// Trunca preservando limites de caractere e acrescenta reticências se cortou.
#[must_use]
pub fn truncar(texto: &str, maximo: usize) -> String {
    if texto.chars().count() <= maximo {
        return texto.to_string();
    }
    let corte = maximo.saturating_sub(1);
    let mut s: String = texto.chars().take(corte).collect();
    s.push('…');
    s
}

/// Só os dígitos de um texto. Usado em código de barras, documento e telefone.
#[must_use]
pub fn somente_digitos(texto: &str) -> String {
    texto.chars().filter(char::is_ascii_digit).collect()
}

/// Substitui prováveis dados pessoais por marcadores.
///
/// Usado pela camada de log (`docs/08-seguranca-permissoes.md` §7 e `docs/13-observabilidade.md`
/// §1). É deliberadamente agressivo: preferimos redigir um número de pedido a vazar um CPF.
///
/// ```
/// # use cardeal_kernel::texto::redigir_dados_pessoais;
/// let s = redigir_dados_pessoais("cliente 529.982.247-25 fone (11) 98765-4321");
/// assert!(!s.contains("529.982.247-25"));
/// assert!(s.contains("[cpf]"));
/// ```
#[must_use]
pub fn redigir_dados_pessoais(texto: &str) -> String {
    let mut saida = String::with_capacity(texto.len());
    let chars: Vec<char> = texto.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // E-mail: sequência com '@' e um '.' depois.
        if chars[i].is_alphanumeric() {
            let inicio = i;
            let mut j = i;
            let mut tem_arroba = false;
            let mut ponto_apos_arroba = false;
            while j < chars.len()
                && (chars[j].is_alphanumeric() || matches!(chars[j], '@' | '.' | '_' | '-' | '+'))
            {
                if chars[j] == '@' {
                    tem_arroba = true;
                } else if chars[j] == '.' && tem_arroba {
                    ponto_apos_arroba = true;
                }
                j += 1;
            }
            if tem_arroba && ponto_apos_arroba && j - inicio >= 6 {
                saida.push_str("[email]");
                i = j;
                continue;
            }

            // Sequência com muitos dígitos: candidata a documento ou telefone.
            let trecho: String = chars[inicio..j].iter().collect();
            let n_digitos = trecho.chars().filter(char::is_ascii_digit).count();
            if n_digitos >= 8 && !trecho.chars().any(char::is_alphabetic) {
                saida.push_str(marcador_por_tamanho(n_digitos));
                i = j;
                continue;
            }
        }

        // Sequências pontuadas típicas: 529.982.247-25, (11) 98765-4321, 12.345.678/0001-95
        if chars[i].is_ascii_digit() || chars[i] == '(' {
            let inicio = i;
            let mut j = i;
            while j < chars.len()
                && (chars[j].is_ascii_digit()
                    || matches!(chars[j], '.' | '-' | '/' | '(' | ')' | ' '))
            {
                j += 1;
            }
            let trecho: String = chars[inicio..j].iter().collect();
            let n = trecho.chars().filter(char::is_ascii_digit).count();
            if n >= 8 {
                // Preserva espaço final para não colar palavras.
                let sufixo = if trecho.ends_with(' ') { " " } else { "" };
                saida.push_str(marcador_por_tamanho(n));
                saida.push_str(sufixo);
                i = j;
                continue;
            }
        }

        saida.push(chars[i]);
        i += 1;
    }

    saida
}

const fn marcador_por_tamanho(n_digitos: usize) -> &'static str {
    match n_digitos {
        11 => "[cpf]",
        14 => "[cnpj]",
        44 => "[chave-nfe]",
        8 | 9 | 10 | 12 | 13 => "[telefone]",
        _ => "[numero]",
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn normalizacao() {
        assert_eq!(sem_acentos("Açúcar Cristal"), "Acucar Cristal");
        assert_eq!(chave_busca("  Açúcar   Cristal 1kg "), "acucar cristal 1kg");
        assert_eq!(chave_busca("Coca-Cola 2L"), "coca cola 2l");
        assert_eq!(chave_busca("São João"), "sao joao");
        assert_eq!(chave_busca(""), "");
    }

    #[test]
    fn busca_por_palavras_em_qualquer_ordem() {
        assert!(casa_por_palavras(
            "Açúcar Cristal União 1kg",
            "cristal acucar"
        ));
        assert!(casa_por_palavras("Açúcar Cristal União 1kg", "uniao"));
        assert!(!casa_por_palavras("Açúcar Cristal União 1kg", "refinado"));
        assert!(contem_normalizado("Açúcar", "acu"));
    }

    #[test]
    fn similaridade_entre_descricoes() {
        assert!((similaridade("Coca-Cola 2L", "coca cola 2 l") - 1.0).abs() > f32::EPSILON);
        assert!(similaridade("Coca-Cola 2L", "COCA COLA 2L") > 0.9);
        assert!(similaridade("Arroz Tipo 1 5kg", "Arroz Tipo 1 - 5 kg") > 0.6);
        assert!(similaridade("Arroz Tipo 1", "Feijão Carioca") < 0.2);
        assert!((similaridade("igual", "igual") - 1.0).abs() < f32::EPSILON);
        assert!(similaridade("", "algo") < f32::EPSILON);
    }

    #[test]
    fn truncamento_respeita_caracteres() {
        assert_eq!(truncar("Açúcar", 10), "Açúcar");
        assert_eq!(truncar("Açúcar Cristal", 8), "Açúcar …");
        assert_eq!(truncar("Açúcar Cristal", 8).chars().count(), 8);
    }

    #[test]
    fn redacao_de_dados_pessoais() {
        let entrada = "cliente 529.982.247-25 fone (11) 98765-4321 email joao@exemplo.com.br";
        let saida = redigir_dados_pessoais(entrada);
        assert!(!saida.contains("529.982.247-25"), "vazou CPF: {saida}");
        assert!(!saida.contains("98765-4321"), "vazou telefone: {saida}");
        assert!(
            !saida.contains("joao@exemplo.com.br"),
            "vazou email: {saida}"
        );
        assert!(saida.contains("cliente"));

        let cnpj = redigir_dados_pessoais("CNPJ 11.222.333/0001-81 da empresa");
        assert!(!cnpj.contains("11.222.333"), "vazou CNPJ: {cnpj}");
        assert!(cnpj.contains("empresa"));

        // Texto sem dados pessoais passa intacto.
        assert_eq!(
            redigir_dados_pessoais("venda 42 finalizada"),
            "venda 42 finalizada"
        );
    }

    #[test]
    fn somente_digitos_extrai() {
        assert_eq!(somente_digitos("(11) 98765-4321"), "11987654321");
        assert_eq!(somente_digitos("abc"), "");
    }
}
