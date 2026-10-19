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

/// Empacota os arquivos (caminho, conteúdo), na ordem dada.
///
/// # Panics
/// Se um arquivo ou o pacote passar de 4 GiB (o ZIP sem ZIP64 não comporta) — impossível
/// para uma planilha de relatório.
#[must_use]
pub fn empacotar(arquivos: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let tamanho = |n: usize| u32::try_from(n).expect("arquivo de mais de 4 GiB");
    let mut saida = Vec::new();
    let mut central = Vec::new();
    for (nome, dados) in arquivos {
        let deslocamento = tamanho(saida.len());
        let crc = crc32(dados);
        let nome_b = nome.as_bytes();
        let nome_len = u16::try_from(nome_b.len()).expect("nome curto");
        // Cabeçalho local.
        u32_le(&mut saida, 0x0403_4b50);
        u16_le(&mut saida, 20);
        u16_le(&mut saida, FLAG_UTF8);
        u16_le(&mut saida, 0); // armazenado
        u16_le(&mut saida, 0); // hora
        u16_le(&mut saida, DATA_DOS);
        u32_le(&mut saida, crc);
        u32_le(&mut saida, tamanho(dados.len()));
        u32_le(&mut saida, tamanho(dados.len()));
        u16_le(&mut saida, nome_len);
        u16_le(&mut saida, 0);
        saida.extend_from_slice(nome_b);
        saida.extend_from_slice(dados);
        // Entrada do diretório central.
        u32_le(&mut central, 0x0201_4b50);
        u16_le(&mut central, 20);
        u16_le(&mut central, 20);
        u16_le(&mut central, FLAG_UTF8);
        u16_le(&mut central, 0);
        u16_le(&mut central, 0);
        u16_le(&mut central, DATA_DOS);
        u32_le(&mut central, crc);
        u32_le(&mut central, tamanho(dados.len()));
        u32_le(&mut central, tamanho(dados.len()));
        u16_le(&mut central, nome_len);
        u16_le(&mut central, 0); // extra
        u16_le(&mut central, 0); // comentário
        u16_le(&mut central, 0); // disco
        u16_le(&mut central, 0); // atributos internos
        u32_le(&mut central, 0); // atributos externos
        u32_le(&mut central, deslocamento);
        central.extend_from_slice(nome_b);
    }
    let inicio_central = tamanho(saida.len());
    let qtd = u16::try_from(arquivos.len()).expect("menos de 65 mil arquivos");
    saida.extend_from_slice(&central);
    // Fim do diretório central.
    u32_le(&mut saida, 0x0605_4b50);
    u16_le(&mut saida, 0);
    u16_le(&mut saida, 0);
    u16_le(&mut saida, qtd);
    u16_le(&mut saida, qtd);
    u32_le(&mut saida, tamanho(central.len()));
    u32_le(&mut saida, inicio_central);
    u16_le(&mut saida, 0);
    saida
}

#[cfg(test)]
pub(crate) mod testes {
    use super::*;

    /// Lê de volta um ZIP armazenado pelo diretório central, conferindo o CRC de cada arquivo.
    pub(crate) fn ler(zip: &[u8]) -> Vec<(String, Vec<u8>)> {
        let u16_em = |i: usize| u16::from_le_bytes([zip[i], zip[i + 1]]);
        let u32_em = |i: usize| u32::from_le_bytes([zip[i], zip[i + 1], zip[i + 2], zip[i + 3]]);
        let fim = zip.len() - 22;
        assert_eq!(u32_em(fim), 0x0605_4b50, "fim do diretório central");
        let qtd = usize::from(u16_em(fim + 10));
        let mut pos = u32_em(fim + 16) as usize;
        let mut saida = Vec::new();
        for _ in 0..qtd {
            assert_eq!(u32_em(pos), 0x0201_4b50);
            let crc = u32_em(pos + 16);
            let tam = u32_em(pos + 20) as usize;
            let nome_len = usize::from(u16_em(pos + 28));
            let local = u32_em(pos + 42) as usize;
            let nome = String::from_utf8(zip[pos + 46..pos + 46 + nome_len].to_vec()).unwrap();
            assert_eq!(u32_em(local), 0x0403_4b50);
            let ini =
                local + 30 + usize::from(u16_em(local + 26)) + usize::from(u16_em(local + 28));
            let dados = zip[ini..ini + tam].to_vec();
            assert_eq!(crc32(&dados), crc, "CRC de {nome}");
            saida.push((nome, dados));
            pos += 46 + nome_len;
        }
        saida
    }

    #[test]
    fn crc32_conhecido() {
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
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
