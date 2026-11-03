//! Um escritor de ZIP mínimo, só com arquivos **armazenados** (sem compressão) — o bastante para
//! o pacote `.xlsx`, que é um ZIP de XMLs. CRC-32 próprio (polinômio do ZIP, 0xEDB88320).

/// O CRC-32 do ZIP.
#[must_use]
pub fn crc32(dados: &[u8]) -> u32 {
    // `i` vai só até 255: o `as u32` não trunca.
    #[allow(clippy::cast_possible_truncation)]
    const fn tabela() -> [u32; 256] {
        let mut t = [0_u32; 256];
        let mut i = 0;
        while i < 256 {
            let mut c = i as u32;
            let mut k = 0;
            while k < 8 {
                c = if c & 1 == 1 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
                k += 1;
            }
            t[i] = c;
            i += 1;
        }
        t
    }
    const TABELA: [u32; 256] = tabela();
    let mut c = 0xFFFF_FFFF_u32;
    for b in dados {
        c = TABELA[((c ^ u32::from(*b)) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

fn u16_le(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_le_bytes());
}

fn u32_le(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
}

/// 1980-01-01, a menor data do ZIP — o conteúdo não precisa de data.
const DATA_DOS: u16 = (1 << 5) | 1;
/// Nomes em UTF-8.
const FLAG_UTF8: u16 = 0x0800;

/// O ZIP sem ZIP64 não comporta o pacote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroZip {
    /// Mais de 65.535 arquivos.
    ArquivosDemais(usize),
    /// Um arquivo ou o pacote passou de 4 GiB.
    GrandeDemais,
}

impl std::fmt::Display for ErroZip {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ArquivosDemais(n) => {
                write!(f, "{n} arquivos não cabem num ZIP (o máximo é 65.535)")
            }
            Self::GrandeDemais => f.write_str("o pacote passou de 4 GiB, o máximo de um ZIP"),
        }
    }
}

impl std::error::Error for ErroZip {}

/// Empacota os arquivos (caminho, conteúdo), na ordem dada, **armazenados**.
///
/// # Panics
/// Se um arquivo ou o pacote passar de 4 GiB (o ZIP sem ZIP64 não comporta) — impossível
/// para uma planilha de relatório.
#[must_use]
pub fn empacotar(arquivos: &[(&str, Vec<u8>)]) -> Vec<u8> {
    escrever(arquivos, false).expect("planilha grande demais para um ZIP")
}

/// Como [`empacotar`], com cada arquivo **comprimido** (deflate) — os XMLs das notas do mês,
/// que encolhem umas dez vezes (o pacote vai por e-mail). O arquivo que não encolhe vai
/// armazenado. Erro em vez de pânico: quem decide o tamanho é o movimento do mês.
///
/// # Errors
/// [`ErroZip`] quando o pacote não cabe num ZIP sem ZIP64.
pub fn empacotar_comprimido<N: AsRef<str>>(arquivos: &[(N, Vec<u8>)]) -> Result<Vec<u8>, ErroZip> {
    escrever(arquivos, true)
}

/// Armazenado (0) ou deflate (8).
const ARMAZENADO: u16 = 0;
const DEFLATE: u16 = 8;

fn escrever<N: AsRef<str>>(arquivos: &[(N, Vec<u8>)], comprimir: bool) -> Result<Vec<u8>, ErroZip> {
    let tamanho = |n: usize| u32::try_from(n).map_err(|_| ErroZip::GrandeDemais);
    let qtd = u16::try_from(arquivos.len()).map_err(|_| ErroZip::ArquivosDemais(arquivos.len()))?;
    let mut saida = Vec::new();
    let mut central = Vec::new();
    for (nome, dados) in arquivos {
        let deslocamento = tamanho(saida.len())?;
        let crc = crc32(dados);
        let comprimido = comprimir
            .then(|| miniz_oxide::deflate::compress_to_vec(dados, 6))
            .filter(|c| c.len() < dados.len());
        let (metodo, gravado) = match &comprimido {
            Some(c) => (DEFLATE, c.as_slice()),
            None => (ARMAZENADO, dados.as_slice()),
        };
        let nome_b = nome.as_ref().as_bytes();
        let nome_len = u16::try_from(nome_b.len()).map_err(|_| ErroZip::GrandeDemais)?;
        let (gravado_len, dados_len) = (tamanho(gravado.len())?, tamanho(dados.len())?);
        // Cabeçalho local.
        u32_le(&mut saida, 0x0403_4b50);
        u16_le(&mut saida, 20);
        u16_le(&mut saida, FLAG_UTF8);
        u16_le(&mut saida, metodo);
        u16_le(&mut saida, 0); // hora
        u16_le(&mut saida, DATA_DOS);
        u32_le(&mut saida, crc);
        u32_le(&mut saida, gravado_len);
        u32_le(&mut saida, dados_len);
        u16_le(&mut saida, nome_len);
        u16_le(&mut saida, 0);
        saida.extend_from_slice(nome_b);
        saida.extend_from_slice(gravado);
        // Entrada do diretório central.
        u32_le(&mut central, 0x0201_4b50);
        u16_le(&mut central, 20);
        u16_le(&mut central, 20);
        u16_le(&mut central, FLAG_UTF8);
        u16_le(&mut central, metodo);
        u16_le(&mut central, 0);
        u16_le(&mut central, DATA_DOS);
        u32_le(&mut central, crc);
        u32_le(&mut central, gravado_len);
        u32_le(&mut central, dados_len);
        u16_le(&mut central, nome_len);
        u16_le(&mut central, 0); // extra
        u16_le(&mut central, 0); // comentário
        u16_le(&mut central, 0); // disco
        u16_le(&mut central, 0); // atributos internos
        u32_le(&mut central, 0); // atributos externos
        u32_le(&mut central, deslocamento);
        central.extend_from_slice(nome_b);
    }
    let inicio_central = tamanho(saida.len())?;
    let central_len = tamanho(central.len())?;
    saida.extend_from_slice(&central);
    // Fim do diretório central.
    u32_le(&mut saida, 0x0605_4b50);
    u16_le(&mut saida, 0);
    u16_le(&mut saida, 0);
    u16_le(&mut saida, qtd);
    u16_le(&mut saida, qtd);
    u32_le(&mut saida, central_len);
    u32_le(&mut saida, inicio_central);
    u16_le(&mut saida, 0);
    tamanho(saida.len())?;
    Ok(saida)
}

/// Lê de volta um ZIP deste módulo pelo diretório central, conferindo o CRC de cada arquivo —
/// para os testes (deste crate e de quem grava pacotes).
///
/// # Panics
/// Se o ZIP não for um destes (sem diretório central, CRC errado, deflate inválido).
#[doc(hidden)]
#[must_use]
pub fn ler(zip: &[u8]) -> Vec<(String, Vec<u8>)> {
    let u16_em = |i: usize| u16::from_le_bytes([zip[i], zip[i + 1]]);
    let u32_em = |i: usize| u32::from_le_bytes([zip[i], zip[i + 1], zip[i + 2], zip[i + 3]]);
    let fim = zip.len() - 22;
    assert_eq!(u32_em(fim), 0x0605_4b50, "fim do diretório central");
    let qtd = usize::from(u16_em(fim + 10));
    let mut pos = u32_em(fim + 16) as usize;
    let mut saida = Vec::new();
    for _ in 0..qtd {
        assert_eq!(u32_em(pos), 0x0201_4b50);
        let metodo = u16_em(pos + 10);
        let crc = u32_em(pos + 16);
        let gravado = u32_em(pos + 20) as usize;
        let nome_len = usize::from(u16_em(pos + 28));
        let local = u32_em(pos + 42) as usize;
        let nome = String::from_utf8(zip[pos + 46..pos + 46 + nome_len].to_vec()).unwrap();
        assert_eq!(u32_em(local), 0x0403_4b50);
        let ini = local + 30 + usize::from(u16_em(local + 26)) + usize::from(u16_em(local + 28));
        let bruto = &zip[ini..ini + gravado];
        let dados = match metodo {
            ARMAZENADO => bruto.to_vec(),
            DEFLATE => miniz_oxide::inflate::decompress_to_vec(bruto).expect("deflate válido"),
            m => panic!("método {m} desconhecido"),
        };
        assert_eq!(dados.len(), u32_em(pos + 24) as usize, "tamanho de {nome}");
        assert_eq!(crc32(&dados), crc, "CRC de {nome}");
        saida.push((nome, dados));
        pos += 46 + nome_len;
    }
    saida
}

#[cfg(test)]
pub(crate) mod testes {
    use super::*;

    #[test]
    fn crc32_conhecido() {
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn comprime_o_que_encolhe_e_le_de_volta() {
        let xml = "<nfeProc><NFe><infNFe>".to_owned() + &"<det><prod>x</prod></det>".repeat(200);
        let arquivos = vec![
            ("Saidas/a.xml".to_owned(), xml.clone().into_bytes()),
            ("curto.txt".to_owned(), b"ab".to_vec()),
        ];
        let zip = empacotar_comprimido(&arquivos).unwrap();
        assert!(
            zip.len() < xml.len() / 5,
            "comprimiu: {} de {}",
            zip.len(),
            xml.len()
        );
        let lidos = ler(&zip);
        assert_eq!(lidos[0], ("Saidas/a.xml".to_owned(), xml.into_bytes()));
        // O que não encolhe vai armazenado.
        assert_eq!(lidos[1], ("curto.txt".to_owned(), b"ab".to_vec()));
    }

    #[test]
    fn arquivos_demais_e_erro_nao_panico() {
        let muitos: Vec<(String, Vec<u8>)> = (0..=usize::from(u16::MAX))
            .map(|i| (i.to_string(), Vec::new()))
            .collect();
        assert_eq!(
            empacotar_comprimido(&muitos),
            Err(ErroZip::ArquivosDemais(65_536))
        );
    }

    #[test]
    fn empacota_e_le_de_volta() {
        let zip = empacotar(&[
            ("a.txt", b"ola".to_vec()),
            ("pasta/b.xml", b"<x/>".to_vec()),
        ]);
        let lidos = ler(&zip);
        assert_eq!(lidos.len(), 2);
        assert_eq!(lidos[0], ("a.txt".to_owned(), b"ola".to_vec()));
        assert_eq!(lidos[1].0, "pasta/b.xml");
    }
}
