//! Mandar o cupom à impressora térmica, sem abrir o PDF: os bytes ESC/POS (`cardeal_pdf::escpos`)
//! vão à impressora **instalada** no sistema (no Windows, pelo spooler em modo cru — o driver da
//! térmica passa os comandos adiante; fora dele, `lp -o raw`) ou à impressora **de rede** (TCP,
//! porta 9100, o padrão das térmicas com placa de rede).

use std::io::Write as _;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use crate::maquina::Destino;

/// A porta padrão das térmicas de rede (o "raw" / JetDirect).
pub const PORTA_PADRAO: u16 = 9100;

/// Para onde foi o que se mandou imprimir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Saida {
    /// O PDF, gravado e aberto no visualizador.
    Pdf(std::path::PathBuf),
    /// Direto à impressora (o nome ou o endereço dela).
    Impressora(String),
}

impl Saida {
    /// A frase para o aviso: "Cupom fiscal aberto para imprimir" / "… enviado à impressora X".
    #[must_use]
    pub fn mensagem(&self, oque: &str) -> String {
        match self {
            Self::Pdf(_) => format!("{oque} aberto para imprimir"),
            Self::Impressora(onde) => format!("{oque} enviado à impressora {onde}"),
        }
    }

    /// O PDF gravado, quando foi PDF.
    #[cfg(test)]
    #[must_use]
    pub fn caminho(&self) -> Option<&std::path::Path> {
        match self {
            Self::Pdf(c) => Some(c),
            Self::Impressora(_) => None,
        }
    }
}

/// As opções do ESC/POS para a impressora desta máquina.
#[must_use]
pub fn opcoes(i: &crate::maquina::ImpressoraDaMaquina) -> cardeal_pdf::OpcoesEscPos {
    cardeal_pdf::OpcoesEscPos { acentos: i.acentos }
}

/// Manda os bytes ao destino. Devolve para onde foi ("a Elgin i9", "192.168.0.50"), para a
/// mensagem da tela.
///
/// # Errors
/// O destino é o PDF (não é impressora), ou a impressora não aceitou: a mensagem diz por quê.
pub fn enviar(destino: &Destino, bytes: &[u8]) -> Result<String, String> {
    match destino {
        Destino::Pdf => Err("Nenhuma impressora configurada neste computador".into()),
        Destino::Rede(endereco) => rede(endereco, bytes).map(|()| endereco.clone()),
        Destino::Instalada(nome) => instalada(nome, bytes).map(|()| nome.clone()),
    }
}

/// O endereço com a porta (a 9100 quando não vem).
fn com_porta(endereco: &str) -> String {
    let e = endereco.trim();
    if e.rsplit_once(':')
        .is_some_and(|(_, p)| p.parse::<u16>().is_ok())
    {
        e.to_owned()
    } else {
        format!("{e}:{PORTA_PADRAO}")
    }
}

fn rede(endereco: &str, bytes: &[u8]) -> Result<(), String> {
    let alvo = com_porta(endereco);
    let addr = alvo
        .to_socket_addrs()
        .map_err(|e| format!("Endereço da impressora inválido ({alvo}): {e}"))?
        .next()
        .ok_or_else(|| format!("Endereço da impressora inválido ({alvo})"))?;
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(3))
        .map_err(|e| format!("A impressora {alvo} não respondeu: {e}"))?;
    s.set_write_timeout(Some(Duration::from_secs(10)))
        .map_err(|e| e.to_string())?;
    s.write_all(bytes)
        .and_then(|()| s.flush())
        .map_err(|e| format!("A impressora {alvo} parou de receber: {e}"))
}

#[cfg(windows)]
fn largo(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Imprime cru pelo spooler do Windows: abre a impressora, começa um documento "RAW" e escreve.
#[cfg(windows)]
fn instalada(nome: &str, bytes: &[u8]) -> Result<(), String> {
    use windows_sys::Win32::Graphics::Printing::{
        ClosePrinter, EndDocPrinter, EndPagePrinter, OpenPrinterW, StartDocPrinterW,
        StartPagePrinter, WritePrinter, DOC_INFO_1W, PRINTER_HANDLE,
    };
    let nome_w = largo(nome);
    let mut documento = largo("Cardeal - cupom");
    let mut tipo = largo("RAW");
    let mut h = PRINTER_HANDLE::default();
    let tamanho = u32::try_from(bytes.len()).map_err(|_| "cupom grande demais".to_owned())?;
    // SAFETY: as cadeias são UTF-16 terminadas em zero e vivem até o fim da função; o `h` só é
    // usado depois de `OpenPrinterW` devolver sucesso e é fechado em todos os caminhos.
    unsafe {
        if OpenPrinterW(nome_w.as_ptr(), &raw mut h, std::ptr::null()) == 0 {
            return Err(format!(
                "Não deu para abrir a impressora \"{nome}\" (ela está instalada neste computador?)"
            ));
        }
        let info = DOC_INFO_1W {
            pDocName: documento.as_mut_ptr(),
            pOutputFile: std::ptr::null_mut(),
            pDatatype: tipo.as_mut_ptr(),
        };
        let resultado = if StartDocPrinterW(h, 1, &raw const info) == 0 {
            Err(format!("A impressora \"{nome}\" recusou o documento"))
        } else {
            let mut escritos = 0u32;
            let ok = StartPagePrinter(h) != 0
                && WritePrinter(h, bytes.as_ptr().cast(), tamanho, &raw mut escritos) != 0
                && escritos == tamanho;
            EndPagePrinter(h);
            EndDocPrinter(h);
            if ok {
                Ok(())
            } else {
                Err(format!(
                    "A impressora \"{nome}\" não recebeu o cupom inteiro"
                ))
            }
        };
        ClosePrinter(h);
        resultado
    }
}

#[cfg(not(windows))]
fn instalada(nome: &str, bytes: &[u8]) -> Result<(), String> {
    use std::process::{Command, Stdio};
    let mut lp = Command::new("lp")
        .args(["-d", nome, "-o", "raw"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .map_err(|e| format!("Não deu para chamar o lp: {e}"))?;
    lp.stdin
        .take()
        .ok_or_else(|| "lp sem entrada".to_owned())?
        .write_all(bytes)
        .map_err(|e| e.to_string())?;
    let status = lp.wait().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "A impressora \"{nome}\" recusou o cupom (lp: {status})"
        ))
    }
}

/// As impressoras instaladas neste computador (locais e conectadas), pelo nome.
#[cfg(windows)]
#[must_use]
pub fn instaladas() -> Vec<String> {
    use windows_sys::Win32::Graphics::Printing::{
        EnumPrintersW, PRINTER_ENUM_CONNECTIONS, PRINTER_ENUM_LOCAL, PRINTER_INFO_4W,
    };
    let flags = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS;
    let (mut precisa, mut quantas) = (0u32, 0u32);
    // SAFETY: a primeira chamada só mede (buffer nulo, tamanho zero); a segunda recebe um buffer
    // do tamanho pedido, alinhado para `PRINTER_INFO_4W`, e só se leem as `quantas` entradas
    // que ela preencheu; cada nome é uma cadeia UTF-16 terminada em zero dentro do buffer.
    unsafe {
        EnumPrintersW(
            flags,
            std::ptr::null(),
            4,
            std::ptr::null_mut(),
            0,
            &raw mut precisa,
            &raw mut quantas,
        );
        if precisa == 0 {
            return Vec::new();
        }
        let n = (precisa as usize).div_ceil(std::mem::size_of::<PRINTER_INFO_4W>());
        let mut buffer: Vec<PRINTER_INFO_4W> = Vec::with_capacity(n);
        if EnumPrintersW(
            flags,
            std::ptr::null(),
            4,
            buffer.as_mut_ptr().cast(),
            precisa,
            &raw mut precisa,
            &raw mut quantas,
        ) == 0
        {
            return Vec::new();
        }
        let infos = std::slice::from_raw_parts(buffer.as_ptr(), quantas as usize);
        let mut nomes: Vec<String> = infos
            .iter()
            .filter(|i| !i.pPrinterName.is_null())
            .map(|i| {
                let mut fim = 0;
                while *i.pPrinterName.add(fim) != 0 {
                    fim += 1;
                }
                String::from_utf16_lossy(std::slice::from_raw_parts(i.pPrinterName, fim))
            })
            .collect();
        nomes.sort();
        nomes
    }
}

/// As impressoras instaladas neste computador (pelo `lpstat`).
#[cfg(not(windows))]
#[must_use]
pub fn instaladas() -> Vec<String> {
    std::process::Command::new("lpstat")
        .arg("-e")
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
pub(crate) mod testes {
    use std::io::Read as _;
    use std::net::TcpListener;

    use super::*;

    /// Uma "impressora de rede" local: devolve o endereço e o que ela recebeu.
    pub(crate) fn impressora_de_rede() -> (String, std::thread::JoinHandle<Vec<u8>>) {
        let ouvinte = TcpListener::bind("127.0.0.1:0").unwrap();
        let endereco = ouvinte.local_addr().unwrap().to_string();
        let tarefa = std::thread::spawn(move || {
            let (mut conexao, _) = ouvinte.accept().unwrap();
            let mut recebido = Vec::new();
            conexao.read_to_end(&mut recebido).unwrap();
            recebido
        });
        (endereco, tarefa)
    }

    #[test]
    fn manda_os_bytes_a_impressora_de_rede() {
        let (endereco, tarefa) = impressora_de_rede();
        let onde = enviar(&Destino::Rede(endereco.clone()), b"\x1b@ola").unwrap();
        assert_eq!(onde, endereco);
        assert_eq!(tarefa.join().unwrap(), b"\x1b@ola");
    }

    #[test]
    fn a_porta_padrao_e_os_erros_que_se_entendem() {
        assert_eq!(com_porta("192.168.0.50"), "192.168.0.50:9100");
        assert_eq!(com_porta(" 10.0.0.9:9101 "), "10.0.0.9:9101");
        assert!(enviar(&Destino::Pdf, b"x")
            .unwrap_err()
            .contains("Nenhuma impressora"));
        // Uma porta fechada: a mensagem diz que a impressora não respondeu.
        let fechada = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        let erro = enviar(&Destino::Rede(fechada.to_string()), b"x").unwrap_err();
        assert!(erro.contains("não respondeu"), "{erro}");
        let erro = enviar(
            &Destino::Instalada("Impressora que não existe 123".into()),
            b"x",
        );
        assert!(erro.is_err());
    }
}
