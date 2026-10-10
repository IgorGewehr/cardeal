//! Leitura de extrato bancário: OFX (o que todo banco brasileiro exporta) e CSV simples
//! (`data;descrição;valor[;documento]`). Domínio puro — recebe o texto do arquivo.
//!
//! Cada linha ganha um `fitid`, a identidade dela no banco, para a mesma linha nunca entrar duas
//! vezes se o extrato for importado de novo (períodos que se sobrepõem são o normal).

use cardeal_kernel::{Data, Dinheiro};
use serde::{Deserialize, Serialize};

/// O formato do arquivo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FormatoExtrato {
    /// OFX (SGML 1.x ou XML 2.x).
    Ofx,
    /// CSV: `data;descrição;valor[;documento]`, com ou sem cabeçalho.
    Csv,
}

impl FormatoExtrato {
    /// O melhor palpite pelo nome do arquivo (`.ofx` → OFX, o resto → CSV).
    #[must_use]
    pub fn do_arquivo(nome: &str) -> Self {
        if nome.to_lowercase().ends_with(".ofx") {
            Self::Ofx
        } else {
            Self::Csv
        }
    }
}

/// Uma linha do extrato, já interpretada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinhaDeExtrato {
    /// A identidade da linha no banco (única por conta).
    pub fitid: String,
    /// A data do lançamento no banco.
    pub data: Data,
    /// Positivo = crédito (entrou), negativo = débito (saiu).
    pub valor: Dinheiro,
    /// O texto do banco.
    pub descricao: String,
    /// Número do documento/cheque, quando o banco informa.
    pub documento: Option<String>,
}

/// Lê o conteúdo no formato indicado.
///
/// # Errors
/// Mensagem para o usuário dizendo o que não deu para entender.
pub fn ler(formato: FormatoExtrato, texto: &str) -> Result<Vec<LinhaDeExtrato>, String> {
    let linhas = match formato {
        FormatoExtrato::Ofx => ler_ofx(texto)?,
        FormatoExtrato::Csv => ler_csv(texto)?,
    };
    if linhas.is_empty() {
        return Err("O arquivo não tem nenhum lançamento.".to_owned());
    }
    Ok(linhas)
}

/// "1.234,56", "1234.56", "-29,90", "R$ 10,00" → centavos.
fn parse_valor(t: &str) -> Option<Dinheiro> {
    let limpo: String = t
        .trim()
        .chars()
        .filter(|c| c.is_ascii_digit() || matches!(c, ',' | '.' | '-' | '+'))
        .collect();
    if !limpo.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    let negativo = limpo.starts_with('-');
    let mut corpo = limpo.trim_start_matches(['-', '+']).to_owned();
    if corpo.contains(',') {
        // Vírgula decimal: os pontos são milhar.
        corpo = corpo.replace('.', "").replace(',', ".");
    } else if corpo.matches('.').count() > 1 {
        // Vários pontos, sem vírgula: todos milhar.
        corpo = corpo.replace('.', "");
    }
    let (inteiro, frac) = corpo.split_once('.').unwrap_or((corpo.as_str(), ""));
    let inteiro: i64 = if inteiro.is_empty() {
        0
    } else {
        inteiro.parse().ok()?
    };
    let frac: String = format!("{frac:0<2}").chars().take(2).collect();
    let centavos = inteiro
        .checked_mul(100)?
        .checked_add(frac.parse::<i64>().ok()?)?;
    Some(Dinheiro::centavos(if negativo {
        -centavos
    } else {
        centavos
    }))
}

fn tag(bloco: &str, nome: &str) -> Option<String> {
    let alvo = format!("<{}>", nome.to_uppercase());
    let maiusculo = bloco.to_uppercase();
    let ini = maiusculo.find(&alvo)? + alvo.len();
    let resto = &bloco[ini..];
    let fim = resto.find(['<', '\n', '\r']).unwrap_or(resto.len());
    let v = resto[..fim].trim();
    (!v.is_empty()).then(|| v.to_owned())
}

fn data_ofx(t: &str) -> Option<Data> {
    let d: String = t.chars().filter(char::is_ascii_digit).take(8).collect();
    if d.len() != 8 {
        return None;
    }
    Data::de_ymd(
        d[..4].parse().ok()?,
        d[4..6].parse().ok()?,
        d[6..8].parse().ok()?,
    )
    .ok()
}

fn ler_ofx(texto: &str) -> Result<Vec<LinhaDeExtrato>, String> {
    let maiusculo = texto.to_uppercase();
    if !maiusculo.contains("<STMTTRN>") {
        return Err(
            "Não encontrei lançamentos neste OFX (faltam as marcações STMTTRN).".to_owned(),
        );
    }
    let mut saida = Vec::new();
    let mut pos = 0;
    while let Some(i) = maiusculo[pos..].find("<STMTTRN>") {
        let ini = pos + i + "<STMTTRN>".len();
        let fim = maiusculo[ini..].find("</STMTTRN>").map_or_else(
            || {
                maiusculo[ini..]
                    .find("<STMTTRN>")
                    .map_or(texto.len(), |j| ini + j)
            },
            |j| ini + j,
        );
        let bloco = &texto[ini..fim];
        pos = fim;
        let data = tag(bloco, "DTPOSTED")
            .and_then(|d| data_ofx(&d))
            .ok_or("Um lançamento do OFX está sem data (DTPOSTED).")?;
        let valor = tag(bloco, "TRNAMT")
            .and_then(|v| parse_valor(&v))
            .ok_or("Um lançamento do OFX está sem valor legível (TRNAMT).")?;
        let memo = tag(bloco, "MEMO")
            .or_else(|| tag(bloco, "NAME"))
            .unwrap_or_default();
        let documento = tag(bloco, "CHECKNUM").or_else(|| tag(bloco, "REFNUM"));
        let fitid = tag(bloco, "FITID").unwrap_or_else(|| {
            format!("ofx:{}:{}:{memo}", data.formatar_iso(), valor.em_centavos())
        });
        saida.push(LinhaDeExtrato {
            fitid,
            data,
            valor,
            descricao: memo,
            documento,
        });
    }
    Ok(saida)
}

fn data_csv(t: &str) -> Option<Data> {
    let t = t.trim();
    let partes: Vec<&str> = t.split(['/', '-']).collect();
    if partes.len() != 3 {
        return None;
    }
    let n = |s: &str| s.trim().parse::<i32>().ok();
    let (a, b, c) = (n(partes[0])?, n(partes[1])?, n(partes[2])?);
    let (ano, mes, dia) = if partes[0].trim().len() == 4 {
        (a, b, c)
    } else {
        (c, b, a)
    };
    let ano = if ano < 100 { ano + 2000 } else { ano };
    Data::de_ymd(ano, u32::try_from(mes).ok()?, u32::try_from(dia).ok()?).ok()
}

fn ler_csv(texto: &str) -> Result<Vec<LinhaDeExtrato>, String> {
    let separador = if texto.lines().take(3).any(|l| l.contains(';')) {
        ';'
    } else {
        ','
    };
    let mut saida: Vec<LinhaDeExtrato> = Vec::new();
    for (n, linha) in texto.lines().enumerate() {
        let linha = linha.trim();
        if linha.is_empty() {
            continue;
        }
        let campos: Vec<&str> = linha
            .split(separador)
            .map(|c| c.trim().trim_matches('"'))
            .collect();
        // Cabeçalho: a primeira coluna não é uma data.
        let Some(data) = campos.first().and_then(|c| data_csv(c)) else {
            if saida.is_empty() && n == 0 {
                continue;
            }
            return Err(format!(
                "Linha {}: a primeira coluna precisa ser a data (dd/mm/aaaa).",
                n + 1
            ));
        };
        let valor = campos
            .get(2)
            .and_then(|v| parse_valor(v))
            .ok_or_else(|| format!("Linha {}: valor ilegível na terceira coluna.", n + 1))?;
        let descricao = campos.get(1).copied().unwrap_or_default().to_owned();
        let documento = campos
            .get(3)
            .filter(|d| !d.is_empty())
            .map(|d| (*d).to_owned());
        // O mesmo lançamento repetido no mesmo arquivo é legítimo (duas tarifas iguais): conta a ocorrência.
        let base = format!(
            "csv:{}:{}:{descricao}",
            data.formatar_iso(),
            valor.em_centavos()
        );
        let ocorrencia = saida.iter().filter(|l| l.fitid.starts_with(&base)).count();
        saida.push(LinhaDeExtrato {
            fitid: format!("{base}:{ocorrencia}"),
            data,
            valor,
            descricao,
            documento,
        });
    }
    Ok(saida)
}

#[cfg(test)]
mod testes {
    use super::*;

    const OFX: &str = "OFXHEADER:100\nDATA:OFXSGML\n\n<OFX><BANKMSGSRSV1><STMTTRNRS><STMTRS><BANKTRANLIST>\n\
<STMTTRN>\n<TRNTYPE>CREDIT\n<DTPOSTED>20261003120000[-3:BRT]\n<TRNAMT>340.00\n<FITID>A001\n<MEMO>PIX RECEBIDO JOAO SILVA\n</STMTTRN>\n\
<STMTTRN>\n<TRNTYPE>DEBIT\n<DTPOSTED>20261003\n<TRNAMT>-29,90\n<FITID>A002\n<CHECKNUM>0042\n<MEMO>TARIFA PACOTE SERVICOS\n</STMTTRN>\n\
</BANKTRANLIST></STMTRS></STMTTRNRS></BANKMSGSRSV1></OFX>";

    #[test]
    fn ofx_sgml_le_data_valor_memo_e_documento() {
        let l = ler(FormatoExtrato::Ofx, OFX).unwrap();
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].fitid, "A001");
        assert_eq!(l[0].data, Data::de_ymd(2026, 10, 3).unwrap());
        assert_eq!(l[0].valor, Dinheiro::centavos(34_000));
        assert_eq!(l[0].descricao, "PIX RECEBIDO JOAO SILVA");
        assert_eq!(l[1].valor, Dinheiro::centavos(-2_990));
        assert_eq!(l[1].documento.as_deref(), Some("0042"));
    }

    #[test]
    fn ofx_xml_com_tags_fechadas_tambem_vale() {
        let xml = "<OFX><STMTTRN><DTPOSTED>20260105</DTPOSTED><TRNAMT>-1.234,50</TRNAMT><FITID>X9</FITID><MEMO>Aluguel</MEMO></STMTTRN></OFX>";
        let l = ler(FormatoExtrato::Ofx, xml).unwrap();
        assert_eq!(l[0].valor, Dinheiro::centavos(-123_450));
        assert_eq!(l[0].fitid, "X9");
    }

    #[test]
    fn ofx_sem_lancamentos_ou_sem_data_e_recusado_com_motivo() {
        assert!(ler(FormatoExtrato::Ofx, "<OFX></OFX>")
            .unwrap_err()
            .contains("STMTTRN"));
        let sem_data = "<OFX><STMTTRN><TRNAMT>10.00</TRNAMT></STMTTRN></OFX>";
        assert!(ler(FormatoExtrato::Ofx, sem_data)
            .unwrap_err()
            .contains("data"));
    }

    #[test]
    fn csv_com_cabecalho_ponto_e_virgula_e_datas_nos_dois_estilos() {
        let csv = "Data;Descrição;Valor;Documento\n03/10/2026;PIX RECEBIDO;340,00;\n2026-10-04;TARIFA;-29,90;77\n";
        let l = ler(FormatoExtrato::Csv, csv).unwrap();
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].data, Data::de_ymd(2026, 10, 3).unwrap());
        assert_eq!(l[1].data, Data::de_ymd(2026, 10, 4).unwrap());
        assert_eq!(l[1].documento.as_deref(), Some("77"));
    }

    #[test]
    fn csv_repetido_no_mesmo_arquivo_ganha_ocorrencia_e_erro_aponta_a_linha() {
        let csv = "01/10/2026;TARIFA;-5,00\n01/10/2026;TARIFA;-5,00\n";
        let l = ler(FormatoExtrato::Csv, csv).unwrap();
        assert_ne!(l[0].fitid, l[1].fitid);
        let e = ler(FormatoExtrato::Csv, "01/10/2026;X;abc").unwrap_err();
        assert!(e.contains("Linha 1"), "{e}");
        assert!(ler(FormatoExtrato::Csv, "").is_err());
    }

    #[test]
    fn parse_valor_cobre_os_estilos_de_banco() {
        for (t, c) in [
            ("1.234,56", 123_456),
            ("1234.56", 123_456),
            ("-29,90", -2_990),
            ("R$ 10,00", 1_000),
            ("5", 500),
            ("1,5", 150),
            ("1.234.567", 123_456_700),
        ] {
            assert_eq!(parse_valor(t), Some(Dinheiro::centavos(c)), "{t}");
        }
        assert_eq!(parse_valor("abc"), None);
        assert_eq!(
            FormatoExtrato::do_arquivo("EXTRATO.OFX"),
            FormatoExtrato::Ofx
        );
        assert_eq!(FormatoExtrato::do_arquivo("x.csv"), FormatoExtrato::Csv);
    }
}
