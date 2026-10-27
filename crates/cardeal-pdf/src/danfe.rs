//! O **DANFE** em A4 — o Documento Auxiliar da NF-e, no leiaute do Manual de Orientação do
//! Contribuinte: o canhoto, o emitente, o quadro "DANFE" (entrada/saída, número, série, folha), o
//! código de barras da chave (Code 128C), a natureza e o protocolo, o destinatário, as
//! duplicatas, o cálculo do imposto, o transportador, os produtos e os dados adicionais. Os
//! produtos que não cabem na primeira folha seguem nas próximas, com o cabeçalho repetido e
//! "FOLHA n/total".

// Desenho de folha: `x`, `y`, `w`, `h` e o canvas `c` são o vocabulário da geometria aqui.
#![allow(clippy::many_single_char_names)]

use cardeal_kernel::{Data, Dinheiro, Fuso, Instante, Percentual, Preco, Quantidade};

use crate::barras::code128c;
use crate::bobina::quebrar;
use crate::danfce::chave_em_grupos;
use crate::documento::{Canvas, Cor, Documento, MM};
use crate::fonte::{largura_texto, Fonte};

const PRETO: Cor = Cor(0.0, 0.0, 0.0);
const CINZA: Cor = Cor(0.6, 0.6, 0.6);
const X0: f32 = 5.0;
const LARGURA: f32 = 200.0;
const ALTURA: f32 = 297.0;
/// Onde terminam os produtos na primeira folha (abaixo vêm os dados adicionais) e nas outras.
const FIM_PRODUTOS_1: f32 = 262.0;
const FIM_PRODUTOS_N: f32 = 292.0;
const ROTULO: f32 = 5.0;
const VALOR: f32 = 7.5;
const ITEM: f32 = 6.0;
const LINHA_ITEM: f32 = 2.6;

/// O destinatário, como vai impresso.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DestinatarioDanfe {
    /// Nome ou razão social.
    pub nome: String,
    /// CNPJ ou CPF, formatado.
    pub documento: String,
    /// Logradouro, número e complemento.
    pub endereco: String,
    /// Bairro.
    pub bairro: String,
    /// CEP.
    pub cep: String,
    /// Município.
    pub municipio: String,
    /// UF.
    pub uf: String,
    /// Telefone.
    pub fone: String,
    /// Inscrição estadual.
    pub inscricao_estadual: String,
}

/// Uma duplicata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicataDanfe {
    /// O número.
    pub numero: String,
    /// O vencimento.
    pub vencimento: Data,
    /// O valor.
    pub valor: Dinheiro,
}

/// Os totais do cálculo do imposto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TotaisDanfe {
    /// Base de cálculo do ICMS.
    pub base_icms: Dinheiro,
    /// Valor do ICMS.
    pub valor_icms: Dinheiro,
    /// Base do ICMS ST.
    pub base_icms_st: Dinheiro,
    /// Valor do ICMS ST.
    pub valor_icms_st: Dinheiro,
    /// Valor total dos produtos.
    pub produtos: Dinheiro,
    /// Frete.
    pub frete: Dinheiro,
    /// Seguro.
    pub seguro: Dinheiro,
    /// Desconto.
    pub desconto: Dinheiro,
    /// Outras despesas.
    pub outras: Dinheiro,
    /// IPI.
    pub ipi: Dinheiro,
    /// Valor total da nota.
    pub total: Dinheiro,
}

/// Um produto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemDanfe {
    /// Código.
    pub codigo: String,
    /// Descrição.
    pub descricao: String,
    /// NCM.
    pub ncm: String,
    /// CST ou CSOSN, com a origem na frente ("0102").
    pub cst: String,
    /// CFOP.
    pub cfop: String,
    /// Unidade.
    pub unidade: String,
    /// Quantidade.
    pub quantidade: Quantidade,
    /// Valor unitário.
    pub valor_unitario: Preco,
    /// Valor total.
    pub valor_total: Dinheiro,
    /// Base do ICMS.
    pub base_icms: Dinheiro,
    /// Valor do ICMS.
    pub valor_icms: Dinheiro,
    /// Alíquota do ICMS.
    pub aliquota_icms: Option<Percentual>,
}

/// Tudo o que vai no DANFE.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DanfePdf {
    /// Razão social do emitente.
    pub razao_social: String,
    /// O endereço do emitente, em linhas.
    pub endereco_emitente: Vec<String>,
    /// CNPJ, formatado.
    pub cnpj: String,
    /// Inscrição estadual.
    pub inscricao_estadual: String,
    /// Saída (`1`) ou entrada (`0`).
    pub saida: bool,
    /// Número.
    pub numero: u64,
    /// Série.
    pub serie: u32,
    /// Chave de acesso (44 dígitos).
    pub chave: String,
    /// Protocolo de autorização.
    pub protocolo: Option<String>,
    /// Quando foi autorizada.
    pub autorizacao: Option<Instante>,
    /// Natureza da operação.
    pub natureza: String,
    /// Emissão.
    pub emissao: Instante,
    /// O destinatário.
    pub destinatario: DestinatarioDanfe,
    /// As duplicatas.
    pub duplicatas: Vec<DuplicataDanfe>,
    /// O cálculo do imposto.
    pub totais: TotaisDanfe,
    /// Por conta de quem vai o frete ("9 - Sem Ocorrência de Transporte").
    pub frete: String,
    /// Os produtos.
    pub itens: Vec<ItemDanfe>,
    /// Informações complementares.
    pub informacoes: Option<String>,
    /// Emitida em homologação (sem valor fiscal).
    pub homologacao: bool,
    /// O fuso das datas.
    pub fuso: Fuso,
}

/// O número com os pontos do DANFE: `000.000.051`.
fn numero_formatado(n: u64) -> String {
    let s = format!("{n:09}");
    format!("{}.{}.{}", &s[0..3], &s[3..6], &s[6..9])
}

/// O texto cortado para caber em `largura_mm`.
fn cortar(texto: &str, fonte: Fonte, tam: f32, largura_mm: f32) -> String {
    let mut t: String = texto.to_owned();
    while !t.is_empty() && largura_texto(&t, fonte, tam) / MM > largura_mm {
        t.pop();
    }
    t
}

fn caixa(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32) {
    c.linha(x, y, x + w, y, 0.5, PRETO);
    c.linha(x, y + h, x + w, y + h, 0.5, PRETO);
    c.linha(x, y, x, y + h, 0.5, PRETO);
    c.linha(x + w, y, x + w, y + h, 0.5, PRETO);
}

/// Um campo: a caixa, o rótulo miúdo em cima e o valor embaixo (à direita, se `direita`).
#[allow(clippy::too_many_arguments)]
fn campo(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32, rotulo: &str, valor: &str, direita: bool) {
    caixa(c, x, y, w, h);
    c.texto(
        x + 1.0,
        y + 2.3,
        Fonte::Regular,
        ROTULO,
        PRETO,
        &cortar(rotulo, Fonte::Regular, ROTULO, w - 2.0),
    );
    let v = cortar(valor, Fonte::Negrito, VALOR, w - 2.0);
    let vx = if direita {
        x + w - 1.0 - largura_texto(&v, Fonte::Negrito, VALOR) / MM
    } else {
        x + 1.0
    };
    c.texto(vx, y + h - 1.6, Fonte::Negrito, VALOR, PRETO, &v);
}

fn titulo_secao(c: &mut Canvas, y: f32, texto: &str) {
    c.texto(X0, y, Fonte::Negrito, 6.5, PRETO, texto);
}

fn tracejado(c: &mut Canvas, y: f32) {
    let mut x = X0;
    while x < X0 + LARGURA {
        c.linha(x, y, (x + 1.5).min(X0 + LARGURA), y, 0.4, PRETO);
        x += 3.0;
    }
}

/// As colunas da tabela de produtos: (rótulo, largura, à direita).
const COLUNAS: [(&str, f32, bool); 12] = [
    ("CÓDIGO", 15.0, false),
    ("DESCRIÇÃO DO PRODUTO / SERVIÇO", 59.0, false),
    ("NCM/SH", 13.0, false),
    ("CST", 8.0, false),
    ("CFOP", 8.0, false),
    ("UN", 8.0, false),
    ("QUANT.", 14.0, true),
    ("VALOR UNIT.", 16.0, true),
    ("VALOR TOTAL", 16.0, true),
    ("BC ICMS", 15.0, true),
    ("VALOR ICMS", 15.0, true),
    ("ALÍQ.", 13.0, true),
];

/// As linhas da descrição de um item (quebrada na coluna).
fn linhas_do_item(item: &ItemDanfe) -> Vec<String> {
    let l = quebrar(&item.descricao, Fonte::Regular, ITEM, COLUNAS[1].1 - 1.5);
    if l.is_empty() {
        vec![String::new()]
    } else {
        l
    }
}

fn altura_do_item(item: &ItemDanfe) -> f32 {
    let n = linhas_do_item(item).len().max(1);
    #[allow(clippy::cast_precision_loss)]
    let n = n as f32;
    n * LINHA_ITEM + 1.4
}

/// Distribui os itens pelas folhas: os índices de cada folha.
fn folhas(itens: &[ItemDanfe]) -> Vec<Vec<usize>> {
    let mut folhas = vec![Vec::new()];
    let mut y = 160.0;
    for (i, item) in itens.iter().enumerate() {
        let h = altura_do_item(item);
        let fim = if folhas.len() == 1 {
            FIM_PRODUTOS_1
        } else {
            FIM_PRODUTOS_N
        };
        if y + h > fim && !folhas.last().is_some_and(Vec::is_empty) {
            folhas.push(Vec::new());
            y = 49.0;
        }
        if let Some(f) = folhas.last_mut() {
            f.push(i);
        }
        y += h;
    }
    folhas
}

/// Uma fileira de campos lado a lado, a partir da margem: `(rótulo, largura, valor, à direita)`.
fn fileira(c: &mut Canvas, y: f32, campos: &[(&str, f32, &str, bool)]) {
    let mut x = X0;
    for &(rotulo, w, valor, direita) in campos {
        campo(c, x, y, w, 8.0, rotulo, valor, direita);
        x += w;
    }
}

/// Um texto centrado na caixa que começa em `x` e tem largura `w`.
fn centrado(c: &mut Canvas, x: f32, w: f32, y: f32, fonte: Fonte, tam: f32, texto: &str) {
    let largura = largura_texto(texto, fonte, tam) / MM;
    c.texto(x + (w - largura) / 2.0, y, fonte, tam, PRETO, texto);
}

/// O quadro do emitente.
fn quadro_emitente(c: &mut Canvas, d: &DanfePdf, y: f32) {
    caixa(c, X0, y, 80.0, 34.0);
    let mut ly = y + 6.0;
    for linha in quebrar(&d.razao_social, Fonte::Negrito, 9.0, 76.0)
        .into_iter()
        .take(2)
    {
        c.texto(X0 + 2.0, ly, Fonte::Negrito, 9.0, PRETO, &linha);
        ly += 4.0;
    }
    ly += 1.0;
    for linha in &d.endereco_emitente {
        for l in quebrar(linha, Fonte::Regular, 6.5, 76.0) {
            if ly > y + 33.0 {
                return;
            }
            c.texto(X0 + 2.0, ly, Fonte::Regular, 6.5, PRETO, &l);
            ly += 3.0;
        }
    }
}

/// O quadro "DANFE": entrada ou saída, número, série e folha.
fn quadro_danfe(c: &mut Canvas, d: &DanfePdf, y: f32, folha: usize, total: usize) {
    let x = X0 + 80.0;
    caixa(c, x, y, 38.0, 34.0);
    centrado(c, x, 38.0, y + 6.0, Fonte::Negrito, 12.0, "DANFE");
    centrado(
        c,
        x,
        38.0,
        y + 9.5,
        Fonte::Regular,
        6.0,
        "Documento Auxiliar da",
    );
    centrado(
        c,
        x,
        38.0,
        y + 12.0,
        Fonte::Regular,
        6.0,
        "Nota Fiscal Eletrônica",
    );
    c.texto(x + 3.0, y + 16.5, Fonte::Regular, 6.0, PRETO, "0 - ENTRADA");
    c.texto(x + 3.0, y + 19.5, Fonte::Regular, 6.0, PRETO, "1 - SAÍDA");
    caixa(c, x + 27.0, y + 14.5, 6.0, 6.0);
    let tipo = if d.saida { "1" } else { "0" };
    c.texto(x + 28.6, y + 19.3, Fonte::Negrito, 9.0, PRETO, tipo);
    let numero = format!("Nº {}", numero_formatado(d.numero));
    centrado(c, x, 38.0, y + 25.0, Fonte::Negrito, 8.0, &numero);
    let serie = format!("SÉRIE {:03}", d.serie);
    centrado(c, x, 38.0, y + 28.5, Fonte::Negrito, 8.0, &serie);
    let folhas = format!("FOLHA {folha}/{total}");
    centrado(c, x, 38.0, y + 32.0, Fonte::Regular, 6.5, &folhas);
}

/// O quadro da chave: o código de barras (Code 128C), a chave em grupos e onde consultar.
fn quadro_chave(c: &mut Canvas, d: &DanfePdf, y: f32) {
    let x = X0 + 118.0;
    let w = X0 + LARGURA - x;
    caixa(c, x, y, w, 34.0);
    if let Some(modulos) = code128c(&d.chave) {
        let total: u32 = modulos.iter().map(|&m| u32::from(m)).sum();
        #[allow(clippy::cast_precision_loss)]
        let modulo = (w - 6.0) / total as f32;
        let mut bx = x + 3.0;
        for (i, &m) in modulos.iter().enumerate() {
            let largura = f32::from(m) * modulo;
            if i % 2 == 0 {
                c.retangulo(bx, y + 2.0, largura, 11.0, PRETO);
            }
            bx += largura;
        }
    }
    c.linha(x, y + 15.0, x + w, y + 15.0, 0.5, PRETO);
    c.texto(
        x + 1.0,
        y + 17.5,
        Fonte::Regular,
        ROTULO,
        PRETO,
        "CHAVE DE ACESSO",
    );
    let grupos = chave_em_grupos(&d.chave);
    c.texto(x + 2.0, y + 21.5, Fonte::Negrito, 7.5, PRETO, &grupos);
    c.linha(x, y + 23.5, x + w, y + 23.5, 0.5, PRETO);
    let consulta = "Consulta de autenticidade no portal nacional da NF-e";
    c.texto(x + 2.0, y + 27.5, Fonte::Regular, 6.0, PRETO, consulta);
    let site = "www.nfe.fazenda.gov.br/portal ou no site da Sefaz Autorizadora";
    c.texto(x + 2.0, y + 30.5, Fonte::Regular, 6.0, PRETO, site);
}

/// O cabeçalho: emitente, o quadro DANFE e a chave (com o código de barras).
fn cabecalho(c: &mut Canvas, d: &DanfePdf, y: f32, folha: usize, total: usize) {
    quadro_emitente(c, d, y);
    quadro_danfe(c, d, y, folha, total);
    quadro_chave(c, d, y);
}

fn canhoto(c: &mut Canvas, d: &DanfePdf) {
    let y = 5.0;
    caixa(c, X0, y, 160.0, 18.0);
    let texto = format!(
        "RECEBEMOS DE {} OS PRODUTOS E/OU SERVIÇOS CONSTANTES DA NOTA FISCAL ELETRÔNICA INDICADA AO \
         LADO. EMISSÃO: {} VALOR TOTAL: R$ {} DESTINATÁRIO: {}",
        d.razao_social,
        d.emissao.data(d.fuso).formatar(),
        d.totais.total.formatar(),
        d.destinatario.nome
    );
    let mut ly = y + 3.0;
    for l in quebrar(&texto, Fonte::Regular, 5.5, 156.0)
        .into_iter()
        .take(3)
    {
        c.texto(X0 + 2.0, ly, Fonte::Regular, 5.5, PRETO, &l);
        ly += 2.5;
    }
    fileira(
        c,
        y + 10.0,
        &[
            ("DATA DE RECEBIMENTO", 40.0, "", false),
            ("IDENTIFICAÇÃO E ASSINATURA DO RECEBEDOR", 120.0, "", false),
        ],
    );
    let xn = X0 + 160.0;
    caixa(c, xn, y, 40.0, 18.0);
    c.texto(xn + 15.0, y + 6.0, Fonte::Negrito, 10.0, PRETO, "NF-e");
    let numero = format!("Nº {}", numero_formatado(d.numero));
    c.texto(xn + 4.0, y + 11.0, Fonte::Negrito, 8.0, PRETO, &numero);
    let serie = format!("SÉRIE {:03}", d.serie);
    c.texto(xn + 4.0, y + 15.0, Fonte::Negrito, 8.0, PRETO, &serie);
    tracejado(c, y + 20.5);
}

/// A natureza, o protocolo e as inscrições do emitente.
fn natureza_e_inscricoes(c: &mut Canvas, d: &DanfePdf) {
    let protocolo = match (&d.protocolo, d.autorizacao) {
        (Some(p), Some(a)) => format!(
            "{p} - {} {}",
            a.data(d.fuso).formatar(),
            a.hora(d.fuso).formatar_com_segundos()
        ),
        (Some(p), None) => p.clone(),
        _ => "Pendente de autorização".to_owned(),
    };
    fileira(
        c,
        61.0,
        &[
            ("NATUREZA DA OPERAÇÃO", 115.0, &d.natureza, false),
            ("PROTOCOLO DE AUTORIZAÇÃO DE USO", 85.0, &protocolo, false),
        ],
    );
    fileira(
        c,
        69.0,
        &[
            ("INSCRIÇÃO ESTADUAL", 66.0, &d.inscricao_estadual, false),
            ("INSC. ESTADUAL DO SUBST. TRIBUT.", 67.0, "", false),
            ("CNPJ", 67.0, &d.cnpj, false),
        ],
    );
}

/// O destinatário.
fn destinatario(c: &mut Canvas, d: &DanfePdf) {
    let de = &d.destinatario;
    let emissao = d.emissao.data(d.fuso).formatar();
    let hora = d.emissao.hora(d.fuso).formatar();
    titulo_secao(c, 80.0, "DESTINATÁRIO / REMETENTE");
    fileira(
        c,
        81.0,
        &[
            ("NOME / RAZÃO SOCIAL", 120.0, &de.nome, false),
            ("CNPJ / CPF", 45.0, &de.documento, false),
            ("DATA DA EMISSÃO", 35.0, &emissao, false),
        ],
    );
    fileira(
        c,
        89.0,
        &[
            ("ENDEREÇO", 100.0, &de.endereco, false),
            ("BAIRRO / DISTRITO", 45.0, &de.bairro, false),
            ("CEP", 20.0, &de.cep, false),
            ("DATA DA SAÍDA/ENTRADA", 35.0, &emissao, false),
        ],
    );
    fileira(
        c,
        97.0,
        &[
            ("MUNICÍPIO", 75.0, &de.municipio, false),
            ("FONE / FAX", 35.0, &de.fone, false),
            ("UF", 10.0, &de.uf, false),
            ("INSCRIÇÃO ESTADUAL", 45.0, &de.inscricao_estadual, false),
            ("HORA DA SAÍDA/ENTRADA", 35.0, &hora, false),
        ],
    );
}

/// As duplicatas (até seis), ou "pagamento à vista".
fn duplicatas(c: &mut Canvas, d: &DanfePdf) {
    titulo_secao(c, 108.0, "FATURA / DUPLICATAS");
    if d.duplicatas.is_empty() {
        campo(c, X0, 109.0, LARGURA, 9.0, "", "PAGAMENTO À VISTA", false);
        return;
    }
    let w = LARGURA / 6.0;
    for (i, dup) in d.duplicatas.iter().take(6).enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let x = X0 + i as f32 * w;
        caixa(c, x, 109.0, w, 9.0);
        let numero = format!("Nº {}", dup.numero);
        c.texto(x + 1.0, 111.8, Fonte::Regular, 5.5, PRETO, &numero);
        let vencimento = format!("Venc. {}", dup.vencimento.formatar());
        c.texto(x + 1.0, 114.5, Fonte::Regular, 5.5, PRETO, &vencimento);
        let valor = format!("R$ {}", dup.valor.formatar());
        c.texto(x + 1.0, 117.2, Fonte::Negrito, 6.0, PRETO, &valor);
    }
}

/// O cálculo do imposto.
fn calculo_do_imposto(c: &mut Canvas, d: &DanfePdf) {
    let t = &d.totais;
    let w = LARGURA / 6.0;
    let (b, vi, bs, vs, pr) = (
        t.base_icms.formatar(),
        t.valor_icms.formatar(),
        t.base_icms_st.formatar(),
        t.valor_icms_st.formatar(),
        t.produtos.formatar(),
    );
    titulo_secao(c, 121.0, "CÁLCULO DO IMPOSTO");
    fileira(
        c,
        122.0,
        &[
            ("BASE DE CÁLC. DO ICMS", w, &b, true),
            ("VALOR DO ICMS", w, &vi, true),
            ("BASE DE CÁLC. ICMS S.T.", w, &bs, true),
            ("VALOR DO ICMS SUBST.", w, &vs, true),
            ("VALOR TOTAL DOS PRODUTOS", w * 2.0, &pr, true),
        ],
    );
    let (fr, se, de, ou, ip, to) = (
        t.frete.formatar(),
        t.seguro.formatar(),
        t.desconto.formatar(),
        t.outras.formatar(),
        t.ipi.formatar(),
        t.total.formatar(),
    );
    fileira(
        c,
        130.0,
        &[
            ("VALOR DO FRETE", w, &fr, true),
            ("VALOR DO SEGURO", w, &se, true),
            ("DESCONTO", w, &de, true),
            ("OUTRAS DESPESAS", w, &ou, true),
            ("VALOR TOTAL DO IPI", w, &ip, true),
            ("VALOR TOTAL DA NOTA", w, &to, true),
        ],
    );
}

/// Da natureza ao transportador (só na primeira folha).
fn quadros(c: &mut Canvas, d: &DanfePdf) {
    natureza_e_inscricoes(c, d);
    destinatario(c, d);
    duplicatas(c, d);
    calculo_do_imposto(c, d);
    titulo_secao(c, 141.0, "TRANSPORTADOR / VOLUMES TRANSPORTADOS");
    fileira(
        c,
        142.0,
        &[
            ("RAZÃO SOCIAL", 110.0, "", false),
            ("FRETE POR CONTA", 90.0, &d.frete, false),
        ],
    );
}

/// A tabela de produtos a partir de `y` (o cabeçalho e as linhas dos `itens`).
fn produtos(c: &mut Canvas, d: &DanfePdf, y: f32, itens: &[usize], fim: f32) {
    let mut x = X0;
    caixa(c, X0, y, LARGURA, fim - y);
    for (rotulo, w, _) in COLUNAS {
        c.linha(x, y, x, fim, 0.3, PRETO);
        c.texto(
            x + 0.6,
            y + 3.6,
            Fonte::Negrito,
            4.8,
            PRETO,
            &cortar(rotulo, Fonte::Negrito, 4.8, w - 1.0),
        );
        x += w;
    }
    c.linha(X0, y + 5.5, X0 + LARGURA, y + 5.5, 0.5, PRETO);
    let mut ly = y + 5.5;
    for &i in itens {
        let item = &d.itens[i];
        let linhas = linhas_do_item(item);
        let valores = [
            item.codigo.clone(),
            String::new(),
            item.ncm.clone(),
            item.cst.clone(),
            item.cfop.clone(),
            item.unidade.clone(),
            item.quantidade.formatar(0),
            item.valor_unitario.formatar(),
            item.valor_total.formatar(),
            item.base_icms.formatar(),
            item.valor_icms.formatar(),
            item.aliquota_icms
                .map(|a| a.formatar(2))
                .unwrap_or_default(),
        ];
        let base = ly + LINHA_ITEM + 0.2;
        let mut x = X0;
        for (j, ((_, w, direita), valor)) in COLUNAS.iter().zip(valores.iter()).enumerate() {
            if j == 1 {
                let mut yy = base;
                for l in &linhas {
                    c.texto(x + 0.6, yy, Fonte::Regular, ITEM, PRETO, l);
                    yy += LINHA_ITEM;
                }
            } else {
                let v = cortar(valor, Fonte::Regular, ITEM, w - 1.0);
                let vx = if *direita {
                    x + w - 0.6 - largura_texto(&v, Fonte::Regular, ITEM) / MM
                } else {
                    x + 0.6
                };
                c.texto(vx, base, Fonte::Regular, ITEM, PRETO, &v);
            }
            x += w;
        }
        ly += altura_do_item(item);
    }
}

fn dados_adicionais(c: &mut Canvas, d: &DanfePdf) {
    titulo_secao(c, 265.0, "DADOS ADICIONAIS");
    caixa(c, X0, 266.0, 130.0, 26.0);
    caixa(c, X0 + 130.0, 266.0, 70.0, 26.0);
    c.texto(
        X0 + 1.0,
        268.3,
        Fonte::Regular,
        ROTULO,
        PRETO,
        "INFORMAÇÕES COMPLEMENTARES",
    );
    c.texto(
        X0 + 131.0,
        268.3,
        Fonte::Regular,
        ROTULO,
        PRETO,
        "RESERVADO AO FISCO",
    );
    let mut texto = String::new();
    if d.homologacao {
        texto.push_str("NF-E EMITIDA EM AMBIENTE DE HOMOLOGAÇÃO - SEM VALOR FISCAL. ");
    }
    if let Some(i) = &d.informacoes {
        texto.push_str(i);
    }
    let mut ly = 271.5;
    for l in quebrar(&texto, Fonte::Regular, 6.0, 126.0)
        .into_iter()
        .take(7)
    {
        c.texto(X0 + 2.0, ly, Fonte::Regular, 6.0, PRETO, &l);
        ly += 2.8;
    }
}

/// Gera o PDF do DANFE.
#[must_use]
pub fn gerar_danfe(d: &DanfePdf) -> Vec<u8> {
    let paginas = folhas(&d.itens);
    let total = paginas.len();
    let mut doc = Documento::novo();
    for (n, itens) in paginas.iter().enumerate() {
        let mut c = Canvas::nova_a4();
        if n == 0 {
            canhoto(&mut c, d);
            cabecalho(&mut c, d, 27.0, 1, total);
            quadros(&mut c, d);
            titulo_secao(&mut c, 153.0, "DADOS DOS PRODUTOS / SERVIÇOS");
            produtos(&mut c, d, 154.5, itens, FIM_PRODUTOS_1);
            dados_adicionais(&mut c, d);
        } else {
            cabecalho(&mut c, d, 5.0, n + 1, total);
            titulo_secao(&mut c, 42.0, "DADOS DOS PRODUTOS / SERVIÇOS (CONTINUAÇÃO)");
            produtos(&mut c, d, 43.5, itens, FIM_PRODUTOS_N);
        }
        if d.homologacao {
            c.texto(30.0, 215.0, Fonte::Negrito, 30.0, CINZA, "SEM VALOR FISCAL");
        }
        doc.pagina(c);
    }
    let _ = ALTURA;
    doc.finalizar()
}

#[cfg(test)]
mod testes {
    use super::*;

    pub(crate) fn exemplo(itens: usize) -> DanfePdf {
        let item = |i: usize| ItemDanfe {
            codigo: format!("P{i:04}"),
            descricao: if i % 3 == 0 {
                "Capinha de silicone para celular com proteção de câmera e bordas reforçadas, \
                 várias cores"
                    .into()
            } else {
                "Película de vidro".into()
            },
            ncm: "39269090".into(),
            cst: "0102".into(),
            cfop: "5102".into(),
            unidade: "UN".into(),
            quantidade: Quantidade::unidades(2),
            valor_unitario: Preco::reais(25),
            valor_total: Dinheiro::reais(50),
            base_icms: Dinheiro::ZERO,
            valor_icms: Dinheiro::ZERO,
            aliquota_icms: None,
        };
        DanfePdf {
            razao_social: "EMPRESA TESTE LTDA".into(),
            endereco_emitente: vec![
                "Rua A, 1 - Centro".into(),
                "Porto Alegre/RS - CEP 90010-000".into(),
            ],
            cnpj: "11.222.333/0001-81".into(),
            inscricao_estadual: "0960000000".into(),
            saida: true,
            numero: 51,
            serie: 2,
            chave: "43261011222333000181550020000000511000000013".into(),
            protocolo: Some("143260000000051".into()),
            autorizacao: Some(Instante::de_micros(1_793_030_400_000_000)),
            natureza: "Venda de mercadoria".into(),
            emissao: Instante::de_micros(1_793_030_000_000_000),
            destinatario: DestinatarioDanfe {
                nome: "CLIENTE LTDA".into(),
                documento: "11.222.333/0001-81".into(),
                endereco: "Rua Sinimbu, 100".into(),
                bairro: "Centro".into(),
                cep: "95020-000".into(),
                municipio: "Caxias do Sul".into(),
                uf: "RS".into(),
                fone: String::new(),
                inscricao_estadual: String::new(),
            },
            duplicatas: vec![DuplicataDanfe {
                numero: "001".into(),
                vencimento: Data::de_ymd(2026, 11, 26).unwrap(),
                valor: Dinheiro::reais(50),
            }],
            totais: TotaisDanfe {
                produtos: Dinheiro::reais(50),
                total: Dinheiro::reais(50),
                ..TotaisDanfe::default()
            },
            frete: "9 - Sem Ocorrência de Transporte".into(),
            itens: (0..itens).map(item).collect(),
            informacoes: Some(
                "DOCUMENTO EMITIDO POR ME OU EPP OPTANTE PELO SIMPLES NACIONAL. NAO GERA DIREITO \
                 A CREDITO FISCAL DE IPI."
                    .into(),
            ),
            homologacao: true,
            fuso: Fuso::BRASILIA,
        }
    }

    fn paginas(pdf: &[u8]) -> usize {
        String::from_utf8_lossy(pdf).matches("/Type /Page ").count()
            + String::from_utf8_lossy(pdf)
                .matches("/Type /Page\n")
                .count()
            + String::from_utf8_lossy(pdf).matches("/Type /Page>").count()
    }

    #[test]
    fn a_folha_traz_o_que_o_manual_pede() {
        let pdf = gerar_danfe(&exemplo(3));
        // Para ver a folha: CARDEAL_DANFE_SAIDA=caminho.pdf cargo test -p cardeal-pdf danfe
        if let Ok(destino) = std::env::var("CARDEAL_DANFE_SAIDA") {
            std::fs::write(destino, &pdf).unwrap();
        }
        let texto = String::from_utf8_lossy(&pdf);
        for trecho in [
            "DANFE",
            // O texto vai em WinAnsi: os acentos não sobrevivem à leitura em UTF-8.
            "000.000.051",
            "RIE 002",
            "FOLHA 1/1",
            "4326 1011 2223 3300",
            "CLIENTE LTDA",
            "FATURA / DUPLICATAS",
            "Venc. 26/11/2026",
            "143260000000051",
            "SEM VALOR FISCAL",
            "OPTANTE",
            "SIMPLES",
        ] {
            assert!(texto.contains(trecho), "falta {trecho:?}");
        }
        // O código de barras são retângulos.
        assert!(texto.matches(" re f").count() > 50);
    }

    #[test]
    fn muitos_produtos_seguem_nas_proximas_folhas() {
        let pdf = gerar_danfe(&exemplo(120));
        let texto = String::from_utf8_lossy(&pdf);
        assert!(texto.contains("FOLHA 1/"), "folha 1");
        assert!(texto.contains("FOLHA 2/"), "folha 2");
        assert!(texto.contains("(CONTINUAÇÃO)") || texto.contains("CONTINUA"));
        let f = folhas(&exemplo(120).itens);
        assert!(f.len() >= 3, "{}", f.len());
        assert_eq!(
            f.iter().map(Vec::len).sum::<usize>(),
            120,
            "nenhum item some"
        );
        assert!(paginas(&pdf) >= f.len());
        assert_eq!(numero_formatado(51), "000.000.051");
    }
}
