//! O casamento sugerido entre uma linha do extrato e o que o sistema já sabe: uma parcela em
//! aberto (o banco mostra um pagamento que ainda não foi baixado) ou uma baixa já feita (o
//! dinheiro já foi registrado, falta confirmar que o banco o viu). Domínio puro.
//!
//! O valor tem que ser **exatamente** o da parcela ou da baixa — extrato é prova, não palpite.
//! A data e o número do documento só aumentam a confiança.

use cardeal_kernel::{Data, Dinheiro, Id};
use serde::{Deserialize, Serialize};

/// Com o que a linha do extrato pode ser casada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlvoDaConciliacao {
    /// Uma parcela em aberto: conciliar dá a baixa nela, na data do banco.
    Parcela(Id),
    /// Uma baixa que já existe no sistema: conciliar só confirma.
    Baixa(Id),
}

/// Um candidato ao casamento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidato {
    /// O alvo.
    pub alvo: AlvoDaConciliacao,
    /// O valor que ele representa (saldo da parcela ou valor da baixa).
    pub valor: Dinheiro,
    /// O vencimento da parcela ou a data da baixa.
    pub data: Data,
    /// Descrição/observação do título, para procurar o número do documento.
    pub texto: String,
}

/// O que o banco mostra.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaDoBanco {
    /// A data.
    pub data: Data,
    /// O valor sem sinal.
    pub valor: Dinheiro,
    /// O documento/cheque, se houver.
    pub documento: Option<String>,
}

/// Uma sugestão pontuada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sugestao {
    /// O alvo.
    pub alvo: AlvoDaConciliacao,
    /// De 60 a 100.
    pub confianca: u8,
    /// O valor.
    pub valor: Dinheiro,
    /// O vencimento ou a data da baixa.
    pub data: Data,
    /// O texto do título, para a tela mostrar o que é.
    pub texto: String,
}

/// A confiança do casamento; `None` quando o valor não bate ou a confiança fica abaixo de 60.
#[must_use]
pub fn pontuar(linha: &LinhaDoBanco, c: &Candidato) -> Option<u8> {
    if linha.valor != c.valor {
        return None;
    }
    let base: i32 = match c.alvo {
        AlvoDaConciliacao::Baixa(_) => 80,
        AlvoDaConciliacao::Parcela(_) => 65,
    };
    let dias = linha.data.dias_ate(c.data).abs();
    let data: i32 = match dias {
        0 => 20,
        1..=2 => 15,
        3..=7 => 8,
        8..=30 => 3,
        _ => 0,
    };
    let documento: i32 = linha
        .documento
        .as_deref()
        .filter(|d| d.len() >= 3 && c.texto.contains(*d))
        .map_or(0, |_| 10);
    let total = (base + data + documento).min(100);
    // Uma baixa a mais de uma semana de distância já é outro pagamento.
    if matches!(c.alvo, AlvoDaConciliacao::Baixa(_)) && dias > 7 {
        return None;
    }
    (total >= 60).then(|| u8::try_from(total).unwrap_or(100))
}

/// As até três melhores sugestões, da mais confiável para a menos.
#[must_use]
pub fn sugerir(linha: &LinhaDoBanco, candidatos: &[Candidato]) -> Vec<Sugestao> {
    let mut v: Vec<Sugestao> = candidatos
        .iter()
        .filter_map(|c| {
            pontuar(linha, c).map(|confianca| Sugestao {
                alvo: c.alvo,
                confianca,
                valor: c.valor,
                data: c.data,
                texto: c.texto.clone(),
            })
        })
        .collect();
    v.sort_by_key(|s| std::cmp::Reverse(s.confianca));
    v.truncate(3);
    v
}

#[cfg(test)]
mod testes {
    use super::*;

    fn dia(d: u32) -> Data {
        Data::de_ymd(2026, 10, d).unwrap()
    }

    fn linha(d: u32, valor: i64) -> LinhaDoBanco {
        LinhaDoBanco {
            data: dia(d),
            valor: Dinheiro::reais(valor),
            documento: None,
        }
    }

    fn cand(alvo: AlvoDaConciliacao, d: u32, valor: i64, texto: &str) -> Candidato {
        Candidato {
            alvo,
            valor: Dinheiro::reais(valor),
            data: dia(d),
            texto: texto.into(),
        }
    }

    #[test]
    fn valor_diferente_nunca_casa_e_data_so_ajusta_a_confianca() {
        let p = AlvoDaConciliacao::Parcela(Id::novo());
        assert_eq!(pontuar(&linha(3, 340), &cand(p, 3, 341, "")), None);
        assert_eq!(pontuar(&linha(3, 340), &cand(p, 3, 340, "")), Some(85));
        assert_eq!(pontuar(&linha(3, 340), &cand(p, 5, 340, "")), Some(80));
        assert_eq!(pontuar(&linha(3, 340), &cand(p, 20, 340, "")), Some(68));
        // Parcela com muito atraso: valor igual ainda é sugerido, com a confiança mínima.
        assert_eq!(pontuar(&linha(3, 340), &cand(p, 1, 340, "")), Some(80));
    }

    #[test]
    fn baixa_ja_feita_pesa_mais_mas_so_perto_da_data() {
        let b = AlvoDaConciliacao::Baixa(Id::novo());
        assert_eq!(pontuar(&linha(3, 100), &cand(b, 3, 100, "")), Some(100));
        assert_eq!(pontuar(&linha(3, 100), &cand(b, 8, 100, "")), Some(88));
        assert_eq!(pontuar(&linha(3, 100), &cand(b, 25, 100, "")), None);
    }

    #[test]
    fn documento_no_texto_do_titulo_soma_confianca() {
        let p = AlvoDaConciliacao::Parcela(Id::novo());
        let mut l = linha(3, 50);
        l.documento = Some("004211".into());
        assert_eq!(
            pontuar(&l, &cand(p, 30, 50, "NF 004211 do fornecedor")),
            Some(78)
        );
        assert_eq!(pontuar(&l, &cand(p, 30, 50, "outra nota")), Some(68));
    }

    #[test]
    fn sugerir_ordena_e_limita_a_tres() {
        let cs: Vec<Candidato> = [10, 3, 4, 5, 6]
            .iter()
            .map(|d| cand(AlvoDaConciliacao::Parcela(Id::novo()), *d, 100, ""))
            .collect();
        let s = sugerir(&linha(3, 100), &cs);
        assert_eq!(s.len(), 3);
        assert!(s[0].confianca >= s[1].confianca && s[1].confianca >= s[2].confianca);
        assert_eq!(s[0].data, dia(3));
    }
}
