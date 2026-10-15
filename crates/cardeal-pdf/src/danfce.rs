//! O DANFCE — o cupom impresso da NFC-e, numa bobina de 80 mm (altura do tamanho do cupom).
//!
//! Segue o leiaute do Manual de Especificações Técnicas do DANFE NFC-e: identificação do
//! emitente, itens, totais, formas de pagamento, a chave de acesso com o endereço de consulta,
//! o consumidor, número/série/emissão e protocolo, e o QR Code. Em homologação, o aviso de "sem
//! valor fiscal" aparece no topo e no rodapé.

use cardeal_kernel::{Dinheiro, Fuso, Instante, Preco, Quantidade};
use qrcodegen::{QrCode, QrCodeEcc};

use crate::documento::{Canvas, Cor, Documento};
use crate::fonte::{largura_texto, Fonte};
use crate::ErroPdf;

const LARGURA: f32 = 80.0;
const MARGEM: f32 = 4.0;
const UTIL: f32 = LARGURA - 2.0 * MARGEM;
const PRETO: Cor = Cor(0.0, 0.0, 0.0);
const CORPO: f32 = 7.0;
const PEQUENO: f32 = 6.0;
const QR_MM: f32 = 32.0;

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

/// Quebra `texto` em linhas que cabem em `largura_mm`.
fn quebrar(texto: &str, fonte: Fonte, tam: f32, largura_mm: f32) -> Vec<String> {
    let cabe = |s: &str| largura_texto(s, fonte, tam) / crate::documento::MM <= largura_mm;
    let mut linhas = Vec::new();
    let mut atual = String::new();
    for palavra in texto.split_whitespace() {
        let tentativa = if atual.is_empty() {
            palavra.to_string()
        } else {
            format!("{atual} {palavra}")
        };
        if cabe(&tentativa) {
            atual = tentativa;
            continue;
        }
        if !atual.is_empty() {
            linhas.push(std::mem::take(&mut atual));
        }
        // Uma palavra maior que a linha (uma URL): corta por caractere.
        let mut pedaco = String::new();
        for ch in palavra.chars() {
            pedaco.push(ch);
            if !cabe(&pedaco) {
                pedaco.pop();
                linhas.push(std::mem::take(&mut pedaco));
                pedaco.push(ch);
            }
        }
        atual = pedaco;
    }
    if !atual.is_empty() {
        linhas.push(atual);
    }
    linhas
}

/// O cursor de composição: escreve no canvas e desce. A primeira passada mede (canvas
/// descartado), a segunda desenha numa página da altura medida.
struct Cursor {
    canvas: Canvas,
    y: f32,
}

impl Cursor {
    fn centro(&mut self, texto: &str, fonte: Fonte, tam: f32) {
        for linha in quebrar(texto, fonte, tam, UTIL) {
            let largura = largura_texto(&linha, fonte, tam) / crate::documento::MM;
            self.y += tam * 0.42;
            self.canvas
                .texto((LARGURA - largura) / 2.0, self.y, fonte, tam, PRETO, &linha);
        }
    }

    fn esquerda(&mut self, texto: &str, fonte: Fonte, tam: f32) {
        for linha in quebrar(texto, fonte, tam, UTIL) {
            self.y += tam * 0.42;
            self.canvas.texto(MARGEM, self.y, fonte, tam, PRETO, &linha);
        }
    }

    /// Rótulo à esquerda, valor à direita, na mesma linha.
    fn par(&mut self, rotulo: &str, valor: &str, fonte: Fonte) {
        self.y += CORPO * 0.42;
        self.canvas
            .texto(MARGEM, self.y, fonte, CORPO, PRETO, rotulo);
        let largura = largura_texto(valor, fonte, CORPO) / crate::documento::MM;
        self.canvas.texto(
            LARGURA - MARGEM - largura,
            self.y,
            fonte,
            CORPO,
            PRETO,
            valor,
        );
    }

    fn espaco(&mut self, mm: f32) {
        self.y += mm;
    }

    fn tracejado(&mut self) {
        self.y += 1.2;
        let mut x = MARGEM;
        while x < LARGURA - MARGEM {
            self.canvas.linha(
                x,
                self.y,
                (x + 1.2).min(LARGURA - MARGEM),
                self.y,
                0.4,
                PRETO,
            );
            x += 2.0;
        }
        self.y += 0.8;
    }

    fn qr(&mut self, qr: &QrCode) {
        let n = qr.size();
        let modulo = QR_MM / f32::from(u16::try_from(n).unwrap_or(1));
        let x0 = (LARGURA - QR_MM) / 2.0;
        self.y += 1.0;
        for linha in 0..n {
            let mut coluna = 0;
            while coluna < n {
                if qr.get_module(coluna, linha) {
                    // Junta os módulos pretos vizinhos da linha num retângulo só.
                    let inicio = coluna;
                    while coluna < n && qr.get_module(coluna, linha) {
                        coluna += 1;
                    }
                    let largura = (coluna - inicio) as f32 * modulo;
                    self.canvas.retangulo(
                        x0 + inicio as f32 * modulo,
                        self.y + linha as f32 * modulo,
                        largura,
                        modulo,
                        PRETO,
                    );
                } else {
                    coluna += 1;
                }
            }
        }
        self.y += QR_MM + 1.0;
    }
}

fn reais(d: Dinheiro) -> String {
    d.formatar()
}

/// Os itens, os totais e as formas de pagamento.
fn itens_e_totais(c: &mut Cursor, d: &DanfcePdf) {
    c.par("Código  Descrição", "Valor", Fonte::Negrito);
    for (i, item) in d.itens.iter().enumerate() {
        c.esquerda(
            &format!("{:03} {} {}", i + 1, item.codigo, item.descricao),
            Fonte::Regular,
            CORPO,
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

fn compor(d: &DanfcePdf, qr: Option<&QrCode>, canvas: Canvas) -> Cursor {
    let mut c = Cursor { canvas, y: MARGEM };
    let homologacao = "EMITIDA EM AMBIENTE DE HOMOLOGAÇÃO - SEM VALOR FISCAL";

    c.centro(&d.razao_social, Fonte::Negrito, 8.0);
    c.centro(
        &format!("CNPJ {}  IE {}", d.cnpj, d.inscricao_estadual),
        Fonte::Regular,
        PEQUENO,
    );
    c.centro(&d.endereco, Fonte::Regular, PEQUENO);
    c.espaco(1.5);
    c.centro(
        "Documento Auxiliar da Nota Fiscal de Consumidor Eletrônica",
        Fonte::Negrito,
        PEQUENO,
    );
    if d.homologacao {
        c.espaco(1.0);
        c.centro(homologacao, Fonte::Negrito, PEQUENO);
    }
    c.tracejado();

    itens_e_totais(&mut c, d);

    c.centro("Consulte pela Chave de Acesso em", Fonte::Negrito, PEQUENO);
    c.centro(&d.url_consulta, Fonte::Regular, PEQUENO);
    c.centro(&chave_em_grupos(&d.chave), Fonte::Regular, PEQUENO);
    c.tracejado();

    c.centro(
        d.consumidor
            .as_deref()
            .map_or("CONSUMIDOR NÃO IDENTIFICADO".to_string(), |x| {
                format!("CONSUMIDOR - {x}")
            })
            .as_str(),
        Fonte::Negrito,
        PEQUENO,
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
        PEQUENO,
    );
    if let Some(p) = &d.protocolo {
        c.centro(
            &format!("Protocolo de autorização: {p}"),
            Fonte::Regular,
            PEQUENO,
        );
    }
    if let Some(a) = d.autorizacao {
        c.centro(
            &format!(
                "Data de autorização: {} {}",
                a.data(d.fuso).formatar(),
                a.hora(d.fuso).formatar_com_segundos()
            ),
            Fonte::Regular,
            PEQUENO,
        );
    }
    if let Some(qr) = qr {
        c.qr(qr);
    }
    if d.homologacao {
        c.centro(homologacao, Fonte::Negrito, PEQUENO);
    }
    c
}

/// Gera o PDF do DANFCE.
///
/// # Errors
/// [`ErroPdf::QrCode`] se o conteúdo do QR Code não couber num QR Code.
pub fn gerar_danfce(d: &DanfcePdf) -> Result<Vec<u8>, ErroPdf> {
    let qr = d
        .qr_code
        .as_deref()
        .map(|t| QrCode::encode_text(t, QrCodeEcc::Medium))
        .transpose()
        .map_err(|e| ErroPdf::QrCode(format!("{e:?}")))?;
    // Mede numa página alta e descartada; desenha na altura certa.
    let altura = compor(d, qr.as_ref(), Canvas::nova(LARGURA, 2000.0)).y + MARGEM;
    let cursor = compor(d, qr.as_ref(), Canvas::nova(LARGURA, altura));
    let mut doc = Documento::novo();
    doc.pagina(cursor.canvas);
    Ok(doc.finalizar())
}

#[cfg(test)]
mod testes {
    use super::*;

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
            fuso: Fuso::BRASILIA,
        }
    }

    #[test]
    fn gera_bobina_de_80mm_do_tamanho_do_cupom() {
        let pdf = gerar_danfce(&exemplo()).unwrap();
        // Para ver o cupom: CARDEAL_DANFCE_SAIDA=caminho.pdf cargo test -p cardeal-pdf danfce
        if let Ok(destino) = std::env::var("CARDEAL_DANFCE_SAIDA") {
            std::fs::write(destino, &pdf).unwrap();
        }
        let texto = String::from_utf8_lossy(&pdf);
        assert!(texto.starts_with("%PDF-1.7"));
        // 80 mm de largura; a altura é a do conteúdo (bem menos que um A4).
        let i = texto.find("/MediaBox [0 0 ").unwrap() + "/MediaBox [0 0 ".len();
        let dims: Vec<f32> = texto[i..]
            .split(']')
            .next()
            .unwrap()
            .split_whitespace()
            .map(|n| n.parse().unwrap())
            .collect();
        assert!((dims[0] - 226.772).abs() < 0.01, "{dims:?}");
        assert!(dims[1] > 300.0 && dims[1] < 800.0, "{dims:?}");
        assert!(texto.contains("HOMOLOGA"));
        assert!(texto.contains("4326 1065 6097"));
        assert!(texto.contains("Protocolo de autoriza"));
        // O QR Code são retângulos pretos.
        assert!(texto.matches(" re f").count() > 50);
    }

    #[test]
    fn sem_qr_e_sem_consumidor_tambem_sai() {
        let mut d = exemplo();
        d.qr_code = None;
        d.consumidor = None;
        d.homologacao = false;
        let pdf = String::from_utf8_lossy(&gerar_danfce(&d).unwrap()).into_owned();
        assert!(pdf.contains("CONSUMIDOR N"));
        assert!(!pdf.contains("HOMOLOGA"));
        assert!(!pdf.contains(" re f"));
    }

    #[test]
    fn quebra_linha_longa_e_url_sem_espaco() {
        let linhas = quebrar(
            "https://www.sefaz.rs.gov.br/NFCE/NFCE-COM.aspx?p=43261065609793000196650010000000421100000017|3|2",
            Fonte::Regular,
            PEQUENO,
            UTIL,
        );
        assert!(linhas.len() >= 2, "{linhas:?}");
        assert_eq!(chave_em_grupos("12345678"), "1234 5678");
    }
}
