//! O código da SEFAZ (`cStat`) em português de balcão: o que aconteceu e onde corrigir
//! (`saas-erp`, "Convenções de mensagem de erro fiscal": campo, documento, onde corrigir).

/// A frase para o usuário a partir do código e do motivo cru da SEFAZ (ou do provedor).
#[must_use]
pub fn explicar(codigo: &str, motivo: &str) -> String {
    let motivo = motivo.trim();
    let dica = match codigo {
        "100" | "150" => return "Autorizada".to_string(),
        "101" | "151" | "155" => return "Cancelada na SEFAZ".to_string(),
        "217" => "a SEFAZ não tem esta nota — ela vai ser emitida de novo",
        "108" | "109" => "a SEFAZ está fora do ar — a nota fica na fila e é enviada sozinha",
        "204" | "539" => "este número já tem nota na SEFAZ — conferindo qual",
        "225" | "215" | "216" => "a nota saiu fora do formato que a SEFAZ aceita — fale com o suporte",
        "209" | "230" | "231" => {
            "a inscrição estadual da empresa não confere com o cadastro da SEFAZ — confira em Configurações › Fiscal"
        }
        "232" | "233" | "234" => "a inscrição estadual do cliente não confere com o cadastro da SEFAZ",
        "245" | "301" | "303" => "a SEFAZ aponta irregularidade no cadastro da empresa — fale com o contador",
        "302" => "a SEFAZ aponta irregularidade no cadastro do cliente",
        "778" => "o NCM de um produto não existe na tabela da Receita — corrija no cadastro do produto e reenvie",
        "806" => "falta o CEST de um produto com substituição tributária — corrija no cadastro do produto",
        "464" | "465" => "o QR Code não confere — confira o CSC em Configurações › Fiscal",
        "704" => "a nota saiu muito depois da venda — emita uma nova",
        "999" => "a SEFAZ teve um erro interno — tente de novo em alguns minutos",
        "TIMEOUT" | "ERR" => "o resultado não chegou — a nota vai ser conferida na SEFAZ antes de qualquer reenvio",
        "400" => "o emissor recusou os dados da nota — corrija o que está apontado e reenvie",
        _ => "",
    };
    match (dica.is_empty(), motivo.is_empty()) {
        (true, true) => format!("SEFAZ respondeu {codigo}"),
        (true, false) => format!("SEFAZ ({codigo}): {motivo}"),
        (false, true) => format!("{dica} ({codigo})"),
        (false, false) => format!("{dica} — SEFAZ ({codigo}): {motivo}"),
    }
}

#[cfg(test)]
mod testes {
    use super::explicar;

    #[test]
    fn traduz_os_codigos_comuns_e_mantem_o_motivo() {
        assert_eq!(explicar("100", "Autorizado o uso da NF-e"), "Autorizada");
        let ncm = explicar("778", "Rejeição: Informado NCM inexistente");
        assert!(
            ncm.starts_with("o NCM de um produto") && ncm.contains("(778)"),
            "{ncm}"
        );
        assert!(explicar("230", "IE do emitente não cadastrada").contains("Configurações › Fiscal"));
        assert!(explicar("232", "IE do destinatário não informada").contains("do cliente"));
        assert_eq!(explicar("612", "Rejeição: x"), "SEFAZ (612): Rejeição: x");
        assert_eq!(explicar("612", ""), "SEFAZ respondeu 612");
    }
}
