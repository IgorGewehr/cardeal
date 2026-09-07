//! Gera um `amostra-orcamento.pdf` no diretório atual para conferência visual do layout.
//!
//! `cargo run -p cardeal-pdf --example amostra`

use cardeal_kernel::{Data, Dinheiro, Instante, Percentual, Preco, Quantidade};
use cardeal_pdf::{gerar_orcamento, IdentidadeEmpresa, ItemPdf, OrcamentoPdf};

fn main() {
    let empresa = IdentidadeEmpresa {
        nome_fantasia: "Oficina Rubro".to_owned(),
        razao_social: "Rubro Serviços Técnicos LTDA".to_owned(),
        cnpj: "11.222.333/0001-81".to_owned(),
        endereco: "Rua das Oficinas, 100 — Centro, Belo Horizonte/MG".to_owned(),
        telefone: "(31) 3333-4444".to_owned(),
        email: "contato@rubro.example".to_owned(),
        site: "rubro.example".to_owned(),
        logo_png: logo_quadrada(),
    };

    let itens = vec![
        item("Diagnóstico completo e laudo técnico", 1, "un", 180_00, 0),
        item("Rebobinamento do estator (fio de cobre esmaltado 1,2mm)", 1, "un", 1_450_00, 5),
        item("Rolamento blindado 6206-2RS", 2, "un", 62_00, 0),
        item("Rolamento blindado 6204-2RS", 2, "un", 41_00, 0),
        item("Verniz isolante classe H — impregnação a vácuo", 1, "un", 210_00, 0),
        item("Balanceamento dinâmico do rotor", 1, "un", 160_00, 0),
        item("Mão de obra de montagem e teste em bancada", 4, "h", 90_00, 0),
    ];
    let subtotal: Dinheiro = itens
        .iter()
        .map(|i| Dinheiro::de_total(i.quantidade, i.preco_unitario, cardeal_kernel::Arredondamento::MeioAcima))
        .sum();
    let liquido: Dinheiro = itens.iter().map(|i| i.total).sum();

    let orc = OrcamentoPdf {
        numero: 128,
        cliente_nome: "Metalúrgica São Jorge LTDA".to_owned(),
        cliente_documento: "CNPJ 05.914.650/0001-72".to_owned(),
        cliente_contato: "Sr. Antônio — (31) 98888-1234".to_owned(),
        assunto: "Reforma completa do motor elétrico trifásico WEG 15 cv, 4 polos, carcaça 132M"
            .to_owned(),
        descricao: "Escopo: desmontagem, laudo, substituição de rolamentos, rebobinamento total do \
                    estator, impregnação, balanceamento e teste em bancada com relatório."
            .to_owned(),
        data_emissao: Data::de_ymd(2026, 9, 7).unwrap(),
        validade: Data::de_ymd(2026, 9, 22).unwrap(),
        condicoes_pagamento: "50% na aprovação e 50% na retirada — Pix, boleto ou cartão em até 3x."
            .to_owned(),
        prazo_entrega: "7 dias úteis após a aprovação e o adiantamento.".to_owned(),
        observacoes: "Garantia de 90 dias sobre o serviço executado. Não inclui transporte do \
                      equipamento."
            .to_owned(),
        responsavel: "Eng. Marina Alves".to_owned(),
        itens,
        subtotal,
        desconto: subtotal - liquido,
        total: liquido,
    };

    let bytes = gerar_orcamento(&empresa, &orc, Instante::agora()).expect("gerar PDF");
    std::fs::write("amostra-orcamento.pdf", &bytes).expect("escrever arquivo");
    println!("amostra-orcamento.pdf ({} bytes)", bytes.len());
}

fn item(desc: &str, qtd: i64, un: &str, preco_cent: i64, desc_pct: i64) -> ItemPdf {
    let quantidade = Quantidade::unidades(qtd);
    let preco_unitario = Preco::centavos(preco_cent);
    let bruto =
        Dinheiro::de_total(quantidade, preco_unitario, cardeal_kernel::Arredondamento::MeioAcima);
    let desconto_pct = Percentual::pontos(desc_pct);
    let total = bruto - bruto.aplicar(desconto_pct, cardeal_kernel::Arredondamento::MeioAcima);
    ItemPdf {
        descricao: desc.to_owned(),
        quantidade,
        unidade: un.to_owned(),
        preco_unitario,
        desconto_pct,
        total,
    }
}

/// Um PNG 96×96 com um "R" branco em fundo rubro — logo de exemplo, montado à mão.
fn logo_quadrada() -> Option<Vec<u8>> {
    const L: u32 = 96;
    let mut raw = Vec::with_capacity((L * (L * 3 + 1)) as usize);
    for y in 0..L {
        raw.push(0); // filtro None
        for x in 0..L {
            let borda = x < 6 || y < 6 || x >= L - 6 || y >= L - 6;
            let haste = (18..30).contains(&x) && (18..78).contains(&y);
            let topo = (18..64).contains(&x) && (18..30).contains(&y);
            let meio = (18..64).contains(&x) && (44..56).contains(&y);
            let curva_dir = (52..64).contains(&x) && (18..56).contains(&y);
            let perna = {
                let t = (y as i32 - 44).max(0);
                (x as i32 >= 30 + t / 2) && (x as i32 <= 42 + t / 2) && (44..78).contains(&y)
            };
            let branco = !borda && (haste || topo || meio || curva_dir || perna);
            if branco {
                raw.extend_from_slice(&[255, 255, 255]);
            } else {
                raw.extend_from_slice(&[0xEF, 0x44, 0x3B]);
            }
        }
    }
    Some(png_rgb(L, L, &raw))
}

fn png_rgb(w: u32, h: u32, scanlines_com_filtro: &[u8]) -> Vec<u8> {
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    chunk(&mut out, b"IHDR", &{
        let mut v = Vec::new();
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&h.to_be_bytes());
        v.extend_from_slice(&[8, 2, 0, 0, 0]);
        v
    });
    chunk(&mut out, b"IDAT", &zlib_stored(scanlines_com_filtro));
    chunk(&mut out, b"IEND", &[]);
    out
}

fn chunk(out: &mut Vec<u8>, tipo: &[u8; 4], dados: &[u8]) {
    out.extend_from_slice(&u32::try_from(dados.len()).unwrap().to_be_bytes());
    let mut crc_src = tipo.to_vec();
    crc_src.extend_from_slice(dados);
    out.extend_from_slice(tipo);
    out.extend_from_slice(dados);
    out.extend_from_slice(&crc32(&crc_src).to_be_bytes());
}

fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    let mut resto = data;
    while !resto.is_empty() {
        let n = resto.len().min(0xFFFF);
        let (bloco, r) = resto.split_at(n);
        resto = r;
        out.push(u8::from(resto.is_empty()));
        let len = u16::try_from(n).unwrap();
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(bloco);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    out.extend_from_slice(&((b << 16) | a).to_be_bytes());
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}
