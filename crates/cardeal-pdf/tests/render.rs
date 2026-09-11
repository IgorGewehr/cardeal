//! Verifica que o gerador produz um PDF estruturalmente válido — com e sem logo, em uma e
//! em várias páginas.

use cardeal_kernel::{Data, Dinheiro, Instante, Percentual, Preco, Quantidade};
use cardeal_pdf::{
    gerar_comprovante_os, gerar_orcamento, ComprovanteOsPdf, IdentidadeEmpresa, ItemPdf,
    OrcamentoPdf,
};

fn empresa(com_logo: bool) -> IdentidadeEmpresa {
    IdentidadeEmpresa {
        nome_fantasia: "Oficina Cardeal".to_owned(),
        razao_social: "Cardeal Serviços Técnicos LTDA".to_owned(),
        cnpj: "11.222.333/0001-81".to_owned(),
        endereco: "Rua das Oficinas, 100 — Centro, Belo Horizonte/MG".to_owned(),
        telefone: "(31) 3333-4444".to_owned(),
        email: "contato@cardeal.example".to_owned(),
        site: "cardeal.example".to_owned(),
        logo_png: com_logo.then(logo_png),
    }
}

/// Um PNG 4x2 vermelho, montado à mão (assinatura + IHDR + IDAT + IEND).
fn logo_png() -> Vec<u8> {
    // 2x2 RGBA vermelho, sem compressão real — só precisa decodificar.
    use std::io::Write;
    let mut bytes = Vec::new();
    // assinatura
    bytes.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    let chunk = |bytes: &mut Vec<u8>, tipo: &[u8; 4], dados: &[u8]| {
        bytes
            .write_all(&(dados.len() as u32).to_be_bytes())
            .unwrap();
        let mut crc_data = tipo.to_vec();
        crc_data.extend_from_slice(dados);
        bytes.extend_from_slice(tipo);
        bytes.extend_from_slice(dados);
        bytes.write_all(&crc32(&crc_data).to_be_bytes()).unwrap();
    };
    // IHDR: 2x2, bit depth 8, color type 2 (RGB), no interlace
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&2u32.to_be_bytes());
    ihdr.extend_from_slice(&2u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut bytes, b"IHDR", &ihdr);
    // IDAT: zlib stored block com scanlines (filter 0 + 3 bytes por pixel)
    let raw: Vec<u8> = vec![
        0, 220, 40, 40, 220, 40, 40, // linha 0: filtro + 2 px
        0, 220, 40, 40, 220, 40, 40, // linha 1
    ];
    chunk(&mut bytes, b"IDAT", &zlib_stored(&raw));
    chunk(&mut bytes, b"IEND", &[]);
    bytes
}

fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01]; // zlib header
    let len = data.len() as u16;
    out.push(0x01); // BFINAL=1, BTYPE=00 (stored)
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&(!len).to_le_bytes());
    out.extend_from_slice(data);
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in data {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    out.extend_from_slice(&((b << 16) | a).to_be_bytes());
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn orcamento(n_itens: usize) -> OrcamentoPdf {
    let itens: Vec<ItemPdf> = (0..n_itens)
        .map(|i| ItemPdf {
            descricao: format!(
                "Item {i} — serviço técnico com descrição razoavelmente longa para exercitar a \
                 quebra de linha dentro da célula da tabela"
            ),
            quantidade: Quantidade::unidades(2),
            unidade: "un".to_owned(),
            preco_unitario: Preco::reais(150),
            desconto_pct: if i % 3 == 0 {
                Percentual::pontos(10)
            } else {
                Percentual::ZERO
            },
            total: Dinheiro::reais(300),
        })
        .collect();
    let subtotal = Dinheiro::reais(300) * i64::try_from(n_itens).unwrap();
    OrcamentoPdf {
        numero: 42,
        cliente_nome: "João da Silva".to_owned(),
        cliente_documento: "CPF 529.982.247-25".to_owned(),
        cliente_contato: "(31) 99999-0000".to_owned(),
        assunto: "Reforma completa do motor elétrico trifásico de 15 cv".to_owned(),
        descricao: "Escopo: desmontagem, laudo, substituição de rolamentos e rebobinamento \
                    parcial, montagem e teste em bancada."
            .to_owned(),
        data_emissao: Data::de_dias(20_000),
        validade: Data::de_dias(20_015),
        condicoes_pagamento: "50% na aprovação, 50% na entrega — Pix ou cartão".to_owned(),
        prazo_entrega: "7 dias úteis após a aprovação".to_owned(),
        observacoes: "Garantia de 90 dias sobre o serviço executado.".to_owned(),
        responsavel: "Ana Técnica".to_owned(),
        itens,
        subtotal,
        desconto: Dinheiro::reais(30),
        total: subtotal - Dinheiro::reais(30),
    }
}

#[test]
fn gera_pdf_de_uma_pagina_sem_logo() {
    let bytes = gerar_orcamento(&empresa(false), &orcamento(4), Instante::EPOCA).unwrap();
    assert!(bytes.starts_with(b"%PDF-1."), "cabeçalho PDF ausente");
    assert!(bytes.ends_with(b"%%EOF\n"));
    assert!(bytes.len() > 800, "PDF pequeno demais: {}", bytes.len());
}

/// Regressão: `Canvas::texto` chegou a passar a literal WinAnsi (bytes crus, ver `fonte.rs`)
/// por `String::from_utf8_lossy` antes de escrevê-la — todo acento virava o caractere de
/// substituição UTF-8 (`EF BF BD`), que um leitor de PDF (que espera WinAnsiEncoding
/// byte-a-byte) exibia como "ï¿½". Nenhum teste estrutural (cabeçalho/tamanho/páginas) pegava
/// isso porque o PDF continuava válido — só o texto saía ilegível. Este teste verifica os
/// BYTES CP1252 esperados de "Não" (0xC3 tem código próprio em WinAnsi: 'ã'=0xE3) diretamente
/// no conteúdo do PDF, e garante que a sequência de substituição não aparece.
#[test]
fn acentos_sao_codificados_em_winansi_nunca_em_utf8_lossy() {
    let mut orc = orcamento(1);
    orc.observacoes = "Não inclui transporte — garantia sobre peça e mão de obra.".to_owned();
    let bytes = gerar_orcamento(&empresa(false), &orc, Instante::EPOCA).unwrap();

    // "Não" em WinAnsi: N=0x4E, ã=0xE3, o=0x6F.
    assert!(
        bytes.windows(3).any(|w| w == [0x4E, 0xE3, 0x6F]),
        "esperava a literal WinAnsi de 'Não' (4E E3 6F) nos operadores de conteúdo"
    );
    // A sequência de substituição UTF-8 (o sintoma do bug) nunca deve aparecer.
    assert!(
        !bytes.windows(3).any(|w| w == [0xEF, 0xBF, 0xBD]),
        "achou o caractere de substituição UTF-8 (EF BF BD) — acento corrompido"
    );
}

#[test]
fn gera_pdf_multipagina_com_logo() {
    let bytes = gerar_orcamento(&empresa(true), &orcamento(80), Instante::EPOCA).unwrap();
    assert!(bytes.starts_with(b"%PDF-1."));
    let texto = String::from_utf8_lossy(&bytes);
    let paginas = texto.matches("/Type /Page ").count();
    assert!(paginas >= 2, "esperava múltiplas páginas, achei {paginas}");
    assert!(texto.contains("/Subtype /Image"), "logo não embutida");
}

#[test]
fn logo_invalida_vira_erro() {
    let mut e = empresa(true);
    e.logo_png = Some(vec![1, 2, 3, 4]);
    assert!(gerar_orcamento(&e, &orcamento(2), Instante::EPOCA).is_err());
}

fn comprovante_os(com_itens: bool, aprovado: bool) -> ComprovanteOsPdf {
    let itens = if com_itens {
        vec![
            ItemPdf {
                descricao: "Peça — Fonte ADP-400DR".to_owned(),
                quantidade: Quantidade::unidades(1),
                unidade: "un".to_owned(),
                preco_unitario: Preco::reais(390),
                desconto_pct: Percentual::ZERO,
                total: Dinheiro::reais(390),
            },
            ItemPdf {
                descricao: "Mão de obra — Diagnóstico e substituição da fonte".to_owned(),
                quantidade: Quantidade::unidades(1),
                unidade: "srv".to_owned(),
                preco_unitario: Preco::reais(120),
                desconto_pct: Percentual::ZERO,
                total: Dinheiro::reais(120),
            },
        ]
    } else {
        Vec::new()
    };
    let subtotal = itens.iter().fold(Dinheiro::ZERO, |acc, i| acc + i.total);
    ComprovanteOsPdf {
        numero: 7,
        situacao: "Concluída e faturada".to_owned(),
        cliente_nome: "Maria Oliveira".to_owned(),
        cliente_documento: "CPF 111.222.333-44".to_owned(),
        cliente_contato: "(31) 98888-1234".to_owned(),
        equipamento: "Console PlayStation 5 (CFI-1214A)".to_owned(),
        data_abertura: Data::de_dias(20_000),
        defeito_relatado: "Desliga sozinho após 10 minutos de uso.".to_owned(),
        diagnostico: "Pasta térmica ressecada e capacitor da fonte estufado; fonte substituída."
            .to_owned(),
        itens,
        subtotal,
        total: subtotal,
        garantia_dias: 90,
        aprovado_por: if aprovado {
            "Maria Oliveira - CPF 111.222.333-44".to_owned()
        } else {
            String::new()
        },
        tecnico_responsavel: "Igor Gewehr".to_owned(),
    }
}

#[test]
fn gera_pdf_de_comprovante_os_completo() {
    let bytes = gerar_comprovante_os(
        &empresa(false),
        &comprovante_os(true, true),
        Instante::EPOCA,
    )
    .unwrap();
    assert!(bytes.starts_with(b"%PDF-1."), "cabeçalho PDF ausente");
    assert!(bytes.ends_with(b"%%EOF\n"));
    assert!(bytes.len() > 800, "PDF pequeno demais: {}", bytes.len());
}

#[test]
fn gera_pdf_de_comprovante_os_sem_itens_nem_aprovacao() {
    // OS recém-aberta, ainda sem laudo/orçamento — o comprovante de entrada precisa gerar
    // mesmo assim, só sem a tabela de itens e sem o bloco de aprovação.
    let mut os = comprovante_os(false, false);
    os.defeito_relatado = "Não liga.".to_owned();
    os.diagnostico = String::new();
    let bytes = gerar_comprovante_os(&empresa(false), &os, Instante::EPOCA).unwrap();
    assert!(bytes.starts_with(b"%PDF-1."));
    let texto = String::from_utf8_lossy(&bytes);
    assert!(!texto.contains("ORÇAMENTO APROVADO POR"));
}

#[test]
fn comprovante_os_com_logo_invalida_vira_erro() {
    let mut e = empresa(true);
    e.logo_png = Some(vec![9, 9, 9]);
    assert!(gerar_comprovante_os(&e, &comprovante_os(true, true), Instante::EPOCA).is_err());
}
