//! O DANFCE — o cupom impresso da NFC-e, numa bobina de 80 ou 58 mm (altura do tamanho do
//! cupom).
//!
//! Segue o leiaute do Manual de Especificações Técnicas do DANFE NFC-e: identificação do
//! emitente, itens, totais, formas de pagamento, a chave de acesso com o endereço de consulta,
//! o consumidor, número/série/emissão e protocolo, e o QR Code. Em homologação, o aviso de "sem
//! valor fiscal" aparece no topo e no rodapé.

use cardeal_kernel::{Dinheiro, Fuso, Instante, Preco, Quantidade};
use qrcodegen::{QrCode, QrCodeEcc};

use crate::bobina::{imprimir, Bobina, Escrita};
use crate::fonte::Fonte;
use crate::ErroPdf;

/// Um item do cupom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemDanfce {
    /// Código do produto.
    pub codigo: String,
    /// Descrição.
    pub descricao: String,
    /// Quantidade.
    pub quantidade: Quantidade,
    /// Unidade.
    pub unidade: String,
    /// Valor unitário.
    pub valor_unitario: Preco,
    /// Valor total do item (bruto).
    pub valor_total: Dinheiro,
}

/// Tudo o que vai impresso no DANFCE.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DanfcePdf {
    /// Razão social do emitente.
    pub razao_social: String,
    /// CNPJ, formatado.
    pub cnpj: String,
    /// Inscrição estadual.
    pub inscricao_estadual: String,
    /// Endereço numa linha.
    pub endereco: String,
    /// Os itens.
    pub itens: Vec<ItemDanfce>,
    /// Soma dos itens (bruto).
    pub valor_total: Dinheiro,
    /// Descontos.
    pub desconto: Dinheiro,
    /// Valor a pagar.
    pub valor_a_pagar: Dinheiro,
    /// Formas de pagamento e valores.
    pub pagamentos: Vec<(String, Dinheiro)>,
    /// Troco.
    pub troco: Dinheiro,
    /// O consumidor ("CPF 123.456.789-09"), ou `None` (não identificado).
    pub consumidor: Option<String>,
    /// Número.
    pub numero: u64,
    /// Série.
    pub serie: u32,
    /// Emissão.
    pub emissao: Instante,
    /// Chave de acesso (44 dígitos).
    pub chave: String,
    /// Protocolo de autorização.
    pub protocolo: Option<String>,
    /// Quando foi autorizada.
    pub autorizacao: Option<Instante>,
    /// O endereço de consulta pela chave (o `urlChave` do XML).
    pub url_consulta: String,
    /// O conteúdo do QR Code (o `qrCode` do XML).
    pub qr_code: Option<String>,
    /// Emitida em homologação (sem valor fiscal).
    pub homologacao: bool,
    /// Emitida em contingência off-line: o aviso vai no topo e, até a autorização, o
    /// "pendente de autorização" no lugar do protocolo.
    pub contingencia: bool,
    /// O fuso das datas impressas.
    pub fuso: Fuso,
}

/// A chave em 11 grupos de 4 dígitos, como o manual pede.
#[must_use]
pub fn chave_em_grupos(chave: &str) -> String {
    chave
        .as_bytes()
        .chunks(4)
        .map(|g| String::from_utf8_lossy(g).into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

fn reais(d: Dinheiro) -> String {
    d.formatar()
}

/// Os itens, os totais e as formas de pagamento.
fn itens_e_totais(c: &mut impl Escrita, d: &DanfcePdf) {
    let corpo = c.bobina().corpo();
    c.par("Código  Descrição", "Valor", Fonte::Negrito);
    for (i, item) in d.itens.iter().enumerate() {
        c.esquerda(
            &format!("{:03} {} {}", i + 1, item.codigo, item.descricao),
            Fonte::Regular,
            corpo,
        );
        c.par(
            &format!(
                "   {} {} x {}",
                item.quantidade.formatar(0),
                item.unidade,
                item.valor_unitario.formatar()
            ),
            &reais(item.valor_total),
            Fonte::Regular,
        );
    }
    c.tracejado();

    c.par(
        "Qtd. total de itens",
        &d.itens.len().to_string(),
        Fonte::Regular,
    );
    c.par("Valor total R$", &reais(d.valor_total), Fonte::Regular);
    if !d.desconto.e_zero() {
        c.par("Descontos R$", &reais(d.desconto), Fonte::Regular);
    }
    c.par("Valor a pagar R$", &reais(d.valor_a_pagar), Fonte::Negrito);
    c.espaco(1.0);
    c.par("FORMA DE PAGAMENTO", "VALOR PAGO R$", Fonte::Negrito);
    for (forma, valor) in &d.pagamentos {
        c.par(forma, &reais(*valor), Fonte::Regular);
    }
    if !d.troco.e_zero() {
        c.par("Troco R$", &reais(d.troco), Fonte::Regular);
    }
    c.tracejado();
}

pub(crate) fn compor(c: &mut impl Escrita, d: &DanfcePdf, qr: Option<&QrCode>) {
    let homologacao = "EMITIDA EM AMBIENTE DE HOMOLOGAÇÃO - SEM VALOR FISCAL";
    let pequeno = c.bobina().pequeno();

    let titulo = c.bobina().titulo();
    c.centro(&d.razao_social, Fonte::Negrito, titulo);
    c.centro(
        &format!("CNPJ {}  IE {}", d.cnpj, d.inscricao_estadual),
        Fonte::Regular,
        pequeno,
    );
    c.centro(&d.endereco, Fonte::Regular, pequeno);
    c.espaco(1.5);
    c.centro(
        "Documento Auxiliar da Nota Fiscal de Consumidor Eletrônica",
        Fonte::Negrito,
        pequeno,
    );
    if d.homologacao {
        c.espaco(1.0);
        c.centro(homologacao, Fonte::Negrito, pequeno);
    }
    if d.contingencia {
        c.espaco(1.0);
        c.centro("EMITIDA EM CONTINGÊNCIA", Fonte::Negrito, pequeno);
    }
    c.tracejado();

    itens_e_totais(c, d);

    c.centro("Consulte pela Chave de Acesso em", Fonte::Negrito, pequeno);
    c.centro(&d.url_consulta, Fonte::Regular, pequeno);
    c.centro(&chave_em_grupos(&d.chave), Fonte::Regular, pequeno);
    c.tracejado();

    c.centro(
        d.consumidor
            .as_deref()
            .map_or("CONSUMIDOR NÃO IDENTIFICADO".to_string(), |x| {
                format!("CONSUMIDOR - {x}")
            })
            .as_str(),
        Fonte::Negrito,
        pequeno,
    );
    c.centro(
        &format!(
            "NFC-e nº {:09} Série {:03} {} {}",
            d.numero,
            d.serie,
            d.emissao.data(d.fuso).formatar(),
            d.emissao.hora(d.fuso).formatar_com_segundos()
        ),
        Fonte::Regular,
        pequeno,
    );
    if let Some(p) = &d.protocolo {
        c.centro(
            &format!("Protocolo de autorização: {p}"),
            Fonte::Regular,
            pequeno,
        );
    } else if d.contingencia {
        c.centro("EMITIDA EM CONTINGÊNCIA", Fonte::Negrito, pequeno);
        c.centro("Pendente de autorização", Fonte::Negrito, pequeno);
    }
    if let Some(a) = d.autorizacao {
        c.centro(
            &format!(
                "Data de autorização: {} {}",
                a.data(d.fuso).formatar(),
                a.hora(d.fuso).formatar_com_segundos()
            ),
            Fonte::Regular,
            pequeno,
        );
    }
    if let Some(qr) = qr {
        c.qr(qr);
    }
    if d.homologacao {
        c.centro(homologacao, Fonte::Negrito, pequeno);
    }
}

/// Gera o PDF do DANFCE na bobina dada.
///
/// # Errors
/// [`ErroPdf::QrCode`] se o conteúdo do QR Code não couber num QR Code.
pub fn gerar_danfce(d: &DanfcePdf, bobina: Bobina) -> Result<Vec<u8>, ErroPdf> {
    let qr = d
        .qr_code
        .as_deref()
        .map(|t| QrCode::encode_text(t, QrCodeEcc::Medium))
        .transpose()
        .map_err(|e| ErroPdf::QrCode(format!("{e:?}")))?;
    Ok(imprimir(bobina, |c| compor(c, d, qr.as_ref())))
}

#[cfg(test)]
mod testes {
    use super::*;

    /// `(largura, altura)` da página, em pontos.
    fn pagina(pdf: &[u8]) -> (f32, f32) {
        let texto = String::from_utf8_lossy(pdf);
        let i = texto.find("/MediaBox [0 0 ").unwrap() + "/MediaBox [0 0 ".len();
        let dims: Vec<f32> = texto[i..]
            .split(']')
            .next()
            .unwrap()
            .split_whitespace()
            .map(|n| n.parse().unwrap())
            .collect();
        (dims[0], dims[1])
    }

    fn exemplo() -> DanfcePdf {
        DanfcePdf {
            razao_social: "TENSORROOT TECH LTDA".into(),
            cnpj: "65.609.793/0001-96".into(),
            inscricao_estadual: "0960000000".into(),
            endereco: "Rua A, 1 - Centro, Porto Alegre/RS".into(),
            itens: vec![
                ItemDanfce {
                    codigo: "A1B2C3D4E5F6".into(),
                    descricao: "Cabo USB-C trançado de 2 metros com carga rápida".into(),
                    quantidade: Quantidade::unidades(2),
                    unidade: "UN".into(),
                    valor_unitario: Preco::centavos(2_990),
                    valor_total: Dinheiro::centavos(5_980),
                },
                ItemDanfce {
                    codigo: "0F0E0D0C0B0A".into(),
                    descricao: "Película".into(),
                    quantidade: Quantidade::unidades(1),
                    unidade: "UN".into(),
                    valor_unitario: Preco::centavos(1_500),
                    valor_total: Dinheiro::centavos(1_500),
                },
            ],
            valor_total: Dinheiro::centavos(7_480),
            desconto: Dinheiro::centavos(580),
            valor_a_pagar: Dinheiro::centavos(6_900),
            pagamentos: vec![("Pix".into(), Dinheiro::centavos(4_000)), ("Dinheiro".into(), Dinheiro::reais(30))],
            troco: Dinheiro::centavos(100),
            consumidor: Some("CPF 529.982.247-25".into()),
            numero: 42,
            serie: 1,
            emissao: Instante::agora(),
            chave: "43261065609793000196650010000000421100000017".into(),
            protocolo: Some("143260000000042".into()),
            autorizacao: Some(Instante::agora()),
            url_consulta: "www.sefaz.rs.gov.br/nfce/consulta".into(),
            qr_code: Some(
                "https://www.sefaz.rs.gov.br/NFCE/NFCE-COM.aspx?p=43261065609793000196650010000000421100000017|3|2"
                    .into(),
            ),
            homologacao: true,
            contingencia: false,
            fuso: Fuso::BRASILIA,
        }
    }

    #[test]
    fn gera_bobina_de_80mm_do_tamanho_do_cupom() {
        let pdf = gerar_danfce(&exemplo(), Bobina::Mm80).unwrap();
        // Para ver o cupom: CARDEAL_DANFCE_SAIDA=caminho.pdf cargo test -p cardeal-pdf danfce
        if let Ok(destino) = std::env::var("CARDEAL_DANFCE_SAIDA") {
            std::fs::write(destino, &pdf).unwrap();
        }
        let texto = String::from_utf8_lossy(&pdf);
        assert!(texto.starts_with("%PDF-1.7"));
        // 80 mm de largura; a altura é a do conteúdo (bem menos que um A4).
        let (largura, altura) = pagina(&pdf);
        assert!((largura - 226.772).abs() < 0.01, "{largura}");
        assert!(altura > 300.0 && altura < 800.0, "{altura}");
        assert!(texto.contains("HOMOLOGA"));
        assert!(texto.contains("4326 1065 6097"));
        assert!(texto.contains("Protocolo de autoriza"));
        // O QR Code são retângulos pretos.
        assert!(texto.matches(" re f").count() > 50);
    }

    #[test]
    fn em_contingencia_avisa_e_diz_que_falta_a_autorizacao() {
        let mut d = exemplo();
        d.contingencia = true;
        d.protocolo = None;
        d.autorizacao = None;
        let pdf = String::from_utf8_lossy(&gerar_danfce(&d, Bobina::Mm80).unwrap()).into_owned();
        assert!(pdf.contains("EMITIDA EM CONTING"));
        assert!(pdf.contains("Pendente de autoriza"));
        assert!(!pdf.contains("Protocolo de autoriza"));
        // Autorizada depois: o protocolo no lugar do "pendente"; o aviso do topo fica.
        d.protocolo = Some("143260000000042".into());
        let pdf = String::from_utf8_lossy(&gerar_danfce(&d, Bobina::Mm80).unwrap()).into_owned();
        assert!(pdf.contains("EMITIDA EM CONTING"));
        assert!(!pdf.contains("Pendente de autoriza"));
    }

    #[test]
    fn na_termica_sai_o_mesmo_cupom_do_pdf() {
        let bytes =
            crate::danfce_escpos(&exemplo(), Bobina::Mm80, crate::OpcoesEscPos::default()).unwrap();
        let texto = String::from_utf8_lossy(&bytes);
        for trecho in [
            "Documento Auxiliar da Nota Fiscal de Consumidor",
            "Protocolo de autorizacao",
            "4326 1065 6097",
            "EMITIDA EM AMBIENTE DE HOMOLOGACAO",
            "Valor a pagar R$",
        ] {
            assert!(texto.contains(trecho), "falta {trecho:?}");
        }
        // O QR Code foi como imagem.
        assert!(bytes.windows(3).any(|w| w == [0x1D, b'v', b'0']));
    }

    #[test]
    fn sem_qr_e_sem_consumidor_tambem_sai() {
        let mut d = exemplo();
        d.qr_code = None;
        d.consumidor = None;
        d.homologacao = false;
        let pdf = String::from_utf8_lossy(&gerar_danfce(&d, Bobina::Mm80).unwrap()).into_owned();
        assert!(pdf.contains("CONSUMIDOR N"));
        assert!(!pdf.contains("HOMOLOGA"));
        assert!(!pdf.contains(" re f"));
    }

    #[test]
    fn na_bobina_de_58mm_fica_mais_estreito_e_mais_comprido() {
        let (l80, a80) = pagina(&gerar_danfce(&exemplo(), Bobina::Mm80).unwrap());
        let (l58, a58) = pagina(&gerar_danfce(&exemplo(), Bobina::Mm58).unwrap());
        assert!((l58 - 164.409).abs() < 0.01, "{l58}");
        assert!(l58 < l80);
        assert!(
            a58 > a80 * 0.8,
            "{a58} × {a80}: o texto quebra em mais linhas"
        );
    }

    #[test]
    fn quebra_linha_longa_e_url_sem_espaco() {
        let linhas = crate::bobina::quebrar(
            "https://www.sefaz.rs.gov.br/NFCE/NFCE-COM.aspx?p=43261065609793000196650010000000421100000017|3|2",
            Fonte::Regular,
            Bobina::Mm80.pequeno(),
            Bobina::Mm80.util(),
        );
        assert!(linhas.len() >= 2, "{linhas:?}");
        assert_eq!(chave_em_grupos("12345678"), "1234 5678");
    }
}
