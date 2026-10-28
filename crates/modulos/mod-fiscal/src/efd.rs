//! A **EFD ICMS/IPI** (o "SPED Fiscal") do mês, montada a partir dos XMLs: as notas que a
//! empresa emitiu (NFC-e e NF-e, autorizadas e canceladas) e as notas de compra que entraram no
//! mês. O `sefaz-api` não gera SPED; o arquivo nasce aqui e o contador o valida e transmite pelo
//! PVA da Receita.
//!
//! O que sai (leiaute de 2026, Guia Prático 3.2.x):
//! - bloco 0: abertura, dados complementares, contabilista, participantes, unidades e itens (só
//!   os que as entradas usam);
//! - bloco C: o C100 de cada nota, o C170 dos itens das notas de **terceiros** (a nota própria
//!   não leva C170) e o C190 analítico por CST, CFOP e alíquota; a cancelada só com a chave; a
//!   denegada e a inutilizada não entram (deixaram de ser escrituradas em 2023);
//! - bloco E: a apuração do ICMS (E110) com o saldo credor do mês anterior e, havendo imposto, a
//!   obrigação a recolher (E116);
//! - B, D, G, H e K sem movimento; o 1010 com tudo "N"; o bloco 9 com a contagem.
//!
//! Decisões (o contador confere):
//! - **Só regime normal.** O Simples Nacional é dispensado da EFD ICMS/IPI; a geração recusa.
//! - **Entrada pela ótica de quem compra:** o CFOP vira o de entrada (5102 → 1102, 5405 → 1403,
//!   6xxx → 2xxx; na indústria, 1101/1401), o CSOSN do fornecedor do Simples vira CST 090 (500 →
//!   060), e o crédito é o ICMS destacado (ou o `vCredICMSSN` do CSOSN 101). O item vai com o
//!   código, a unidade e o NCM do **cadastro da empresa** (o produto casado na entrada); o que não
//!   foi casado vai com o código do fornecedor e um aviso.
//! - O PIS/COFINS dos itens fica fora do C170 (é da EFD-Contribuições, que acaba em 2027 com a
//!   CBS); o IPI de quem não é industrial é só informado (CST 49), sem apuração (E500).
//! - O inventário (bloco H) não é gerado: o de 31/12 vai na EFD de fevereiro, e a geração avisa.

use std::collections::BTreeMap;

use cardeal_fiscal::{ler_para_escriturar, Crt, DocumentoXml, Emitente, ItemXml, ParticipanteXml};
#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
use cardeal_kernel::{Competencia, Data, Dinheiro, Percentual, Quantidade};
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::{Comando, Consulta, Risco};
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::ModeloFiscal;

/// Por que a empresa não gera a EFD ICMS/IPI: não é do regime normal.
pub const EFD_DISPENSADA: &str = "A empresa é do Simples Nacional (ou MEI), que é dispensado da \
                                  EFD ICMS/IPI — confirme com o contador.";

/// O perfil de apresentação (definido pela SEFAZ da UF).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PerfilEfd {
    /// Perfil A (o mais detalhado; o padrão).
    #[default]
    A,
    /// Perfil B.
    B,
    /// Perfil C.
    C,
}

impl PerfilEfd {
    /// A letra do `IND_PERFIL`.
    #[must_use]
    pub const fn letra(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::B => "B",
            Self::C => "C",
        }
    }
}

/// O contabilista responsável (registro 0100).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contabilista {
    /// Nome.
    pub nome: String,
    /// CPF.
    pub cpf: String,
    /// CRC (com a UF, como consta no registro do conselho).
    pub crc: String,
    /// CNPJ do escritório, se houver.
    pub cnpj: String,
    /// CEP.
    pub cep: String,
    /// Logradouro.
    pub endereco: String,
    /// Número.
    pub numero: String,
    /// Complemento.
    pub complemento: String,
    /// Bairro.
    pub bairro: String,
    /// Telefone.
    pub telefone: String,
    /// E-mail.
    pub email: String,
    /// Código IBGE do município do escritório.
    pub codigo_municipio: String,
}

/// Como a empresa escritura (o que a EFD pede e o cadastro não tem).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfiguracaoEfd {
    /// O perfil.
    pub perfil: PerfilEfd,
    /// Industrial ou equiparado (`IND_ATIV = 0`); senão, "outros" (comércio, serviço).
    pub industrial: bool,
    /// A classificação do estabelecimento industrial (registro 0002, `00` a `09`).
    pub classificacao_industrial: String,
    /// O contabilista.
    pub contabilista: Contabilista,
    /// O código de receita do ICMS próprio na UF (o `COD_REC` do E116).
    pub codigo_receita: String,
    /// O dia do vencimento do ICMS no mês seguinte (0 = não informado).
    pub dia_vencimento: u32,
}

fn digitos(s: &str) -> String {
    s.chars().filter(char::is_ascii_digit).collect()
}

impl ConfiguracaoEfd {
    /// O que falta para gerar o arquivo.
    #[must_use]
    pub fn pendencias(&self) -> Vec<String> {
        let c = &self.contabilista;
        let mut v = Vec::new();
        if c.nome.trim().is_empty() {
            v.push("o nome do contabilista".to_owned());
        }
        if cardeal_kernel::Cpf::novo(&c.cpf).is_err() {
            v.push("o CPF do contabilista".to_owned());
        }
        if c.crc.trim().is_empty() {
            v.push("o CRC do contabilista".to_owned());
        }
        if digitos(&c.codigo_municipio).len() != 7 {
            v.push("o código IBGE do município do contabilista (7 dígitos)".to_owned());
        }
        if self.industrial && digitos(&self.classificacao_industrial).len() != 2 {
            v.push("a classificação do estabelecimento industrial (00 a 09)".to_owned());
        }
        v
    }
}

/// Um item no cadastro da empresa (registro 0200).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemDoCadastro {
    /// O código (o mesmo que vai nas notas da empresa).
    pub codigo: String,
    /// A descrição.
    pub descricao: String,
    /// A sigla da unidade de estoque.
    pub unidade: String,
    /// O NCM.
    pub ncm: String,
    /// O GTIN.
    pub gtin: Option<String>,
    /// O CEST.
    pub cest: Option<String>,
}

/// Uma nota de terceiros que entrou no mês, com o produto do cadastro de cada item (alinhado com
/// os `det` do XML; `None` = item sem produto casado).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntradaEscriturada {
    /// O XML da nota (`nfeProc` ou `NFe`).
    pub xml: String,
    /// O dia em que a mercadoria entrou (a confirmação da entrada).
    pub data_entrada: Data,
    /// O produto de cada item.
    pub itens: Vec<Option<ItemDoCadastro>>,
}

/// Como a nota própria fechou.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SituacaoPropria {
    /// Autorizada.
    Autorizada,
    /// Cancelada.
    Cancelada,
}

/// Uma nota emitida pela empresa no mês.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotaPropria {
    /// Como fechou.
    pub situacao: SituacaoPropria,
    /// O modelo.
    pub modelo: ModeloFiscal,
    /// A série.
    pub serie: u32,
    /// O número.
    pub numero: u64,
    /// A chave.
    pub chave: Option<String>,
    /// O XML (`nfeProc`).
    pub xml: Option<String>,
}

/// Tudo o que entra no arquivo.
#[derive(Debug, Clone, Copy)]
pub struct DadosDaEfd<'a> {
    /// O mês.
    pub competencia: Competencia,
    /// Arquivo substituto (`COD_FIN = 1`) de um já entregue.
    pub substituta: bool,
    /// A empresa.
    pub emitente: &'a Emitente,
    /// O e-mail da empresa (0005).
    pub email: &'a str,
    /// A configuração.
    pub configuracao: &'a ConfiguracaoEfd,
    /// O saldo credor de ICMS que veio do mês anterior.
    pub saldo_credor_anterior: Dinheiro,
    /// As notas emitidas no mês.
    pub proprias: &'a [NotaPropria],
    /// As notas de compra que entraram no mês.
    pub entradas: &'a [EntradaEscriturada],
}

/// A apuração do ICMS próprio do mês (E110).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApuracaoIcms {
    /// Os débitos das saídas.
    pub debitos: Dinheiro,
    /// Os créditos das entradas.
    pub creditos: Dinheiro,
    /// O saldo credor do mês anterior.
    pub saldo_credor_anterior: Dinheiro,
    /// Débitos − créditos − saldo anterior, quando positivo.
    pub saldo_apurado: Dinheiro,
    /// O ICMS a recolher.
    pub a_recolher: Dinheiro,
    /// O saldo credor que vai para o mês seguinte.
    pub saldo_credor_a_transportar: Dinheiro,
}

/// O arquivo gerado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArquivoEfd {
    /// O nome do arquivo (`EFD_ICMS_IPI_<CNPJ>_<AAAAMM>.txt`).
    pub nome: String,
    /// O conteúdo (em UTF-8 aqui; [`ArquivoEfd::em_latin1`] para gravar).
    pub texto: String,
    /// A apuração.
    pub apuracao: ApuracaoIcms,
    /// Quantos documentos (C100) o arquivo tem.
    pub documentos: u32,
    /// O que o contador precisa olhar.
    pub avisos: Vec<String>,
}

impl ArquivoEfd {
    /// O arquivo como o PVA o lê: ISO-8859-1 (o que não cabe vira `?`).
    #[must_use]
    pub fn em_latin1(&self) -> Vec<u8> {
        self.texto
            .chars()
            .map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?'))
            .collect()
    }
}

/// O código da versão do leiaute pelo ano (2020 = 014 … 2026 = 020).
fn codigo_da_versao(ano: i32) -> String {
    format!("{:03}", (ano - 2006).clamp(14, 999))
}

fn valor(d: Dinheiro) -> String {
    let c = d.em_centavos();
    format!(
        "{}{},{:02}",
        if c < 0 { "-" } else { "" },
        (c / 100).abs(),
        (c % 100).abs()
    )
}

/// Alíquota com 2 casas (`Percentual` tem 1% = 1.000.000).
fn aliquota(p: Option<Percentual>) -> String {
    let u = p.map_or(0, Percentual::unidades_internas) / 10_000;
    format!("{},{:02}", u / 100, (u % 100).abs())
}

/// Quantidade com 5 casas (a interna tem 4).
fn quantidade(q: Quantidade) -> String {
    let u = q.unidades_internas();
    format!("{},{:04}0", u / 10_000, (u % 10_000).abs())
}

fn data(d: Data) -> String {
    format!("{:02}{:02}{:04}", d.dia(), d.mes(), d.ano())
}

/// Um campo de texto: sem `|`, sem quebra de linha, sem espaço nas pontas, até `max` caracteres.
fn texto(s: &str, max: usize) -> String {
    s.chars()
        .map(|c| if c == '|' || c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .chars()
        .take(max)
        .collect()
}

/// As linhas do arquivo, com a contagem por registro e por bloco.
#[derive(Default)]
struct Arquivo {
    texto: String,
    /// Registro → quantidade, na ordem em que apareceram.
    por_registro: Vec<(String, usize)>,
    total: usize,
    no_bloco: usize,
}

impl Arquivo {
    fn linha(&mut self, campos: &[String]) {
        let reg = campos[0].clone();
        self.texto.push('|');
        for c in campos {
            self.texto.push_str(c);
            self.texto.push('|');
        }
        self.texto.push_str("\r\n");
        match self.por_registro.iter_mut().find(|(r, _)| *r == reg) {
            Some((_, n)) => *n += 1,
            None => self.por_registro.push((reg, 1)),
        }
        self.total += 1;
        self.no_bloco += 1;
    }

    fn l(&mut self, campos: &[&str]) {
        self.linha(&campos.iter().map(|c| (*c).to_owned()).collect::<Vec<_>>());
    }

    fn abrir(&mut self, reg: &str, com_dados: bool) {
        self.no_bloco = 0;
        self.l(&[reg, if com_dados { "0" } else { "1" }]);
    }

    fn fechar(&mut self, reg: &str) {
        let n = self.no_bloco + 1;
        self.l(&[reg, &n.to_string()]);
    }

    /// Um bloco sem movimento.
    fn vazio(&mut self, bloco: &str) {
        self.abrir(&format!("{bloco}001"), false);
        self.fechar(&format!("{bloco}990"));
    }
}

/// Uma linha do C190 sendo somada.
#[derive(Default, Clone, Copy)]
struct Analitico {
    operacao: Dinheiro,
    base: Dinheiro,
    icms: Dinheiro,
    base_st: Dinheiro,
    st: Dinheiro,
    reducao: Dinheiro,
    ipi: Dinheiro,
}

/// (CST, CFOP, alíquota) → o analítico.
type Analiticos = BTreeMap<(String, String, i64), Analitico>;

/// Um item como a empresa o escritura (o CST, o CFOP e o crédito da ótica dela).
struct Escriturado {
    cst: String,
    cfop: String,
    aliquota: Option<Percentual>,
    base: Dinheiro,
    icms: Dinheiro,
}

fn somar(analiticos: &mut Analiticos, item: &ItemXml, e: &Escriturado) {
    let chave = (
        e.cst.clone(),
        e.cfop.clone(),
        e.aliquota.map_or(0, Percentual::unidades_internas),
    );
    let a = analiticos.entry(chave).or_default();
    a.operacao += item.valor_da_operacao();
    a.base += e.base;
    a.icms += e.icms;
    a.base_st += item.base_st;
    a.st += item.st;
    a.ipi += item.ipi;
    if (e.cst.ends_with("20") || e.cst.ends_with("70")) && e.base.e_positivo() {
        a.reducao += (item.valor - item.desconto + item.despesas - e.base).nao_negativo();
    }
}

/// O CFOP de quem compra, a partir do de quem vendeu.
#[must_use]
pub fn cfop_de_entrada(cfop_do_emitente: &str, industrial: bool) -> String {
    let c = digitos(cfop_do_emitente);
    if c.len() != 4 {
        return c;
    }
    let origem = match &c[..1] {
        "5" => "1",
        "6" => "2",
        "7" => "3",
        outro => return format!("{outro}{}", &c[1..]),
    };
    let resto = match &c[1..] {
        "101" | "102" | "103" | "104" | "105" | "106" | "109" | "110" | "111" | "112" | "113"
        | "114" | "115" | "116" | "117" | "118" | "119" | "120" | "122" | "123" => {
            if industrial {
                "101"
            } else {
                "102"
            }
        }
        "401" | "402" | "403" | "405" => {
            if industrial {
                "401"
            } else {
                "403"
            }
        }
        r => r,
    };
    format!("{origem}{resto}")
}

/// O CST do ICMS de um item, com a origem (3 dígitos): o do XML, ou o CSOSN convertido.
fn cst_do_item(item: &ItemXml) -> String {
    let cst = match (&item.cst_icms, &item.csosn) {
        (Some(c), _) => c.clone(),
        (None, Some(s)) if s == "500" => "60".to_owned(),
        _ => "90".to_owned(),
    };
    format!("{}{cst:0>2}", item.origem)
}

/// Os CSTs em que o ICMS próprio destacado vale como crédito.
fn cst_com_credito(cst: &str) -> bool {
    matches!(
        &cst[cst.len().saturating_sub(2)..],
        "00" | "10" | "20" | "51" | "70" | "90"
    )
}

fn escriturar_entrada(item: &ItemXml, industrial: bool) -> Escriturado {
    let cst = cst_do_item(item);
    let cfop = cfop_de_entrada(&item.cfop, industrial);
    let (aliquota, base, icms) = if item.cst_icms.is_some() {
        if cst_com_credito(&cst) && item.icms.e_positivo() {
            (item.aliquota_icms, item.base_icms, item.icms)
        } else {
            (None, Dinheiro::ZERO, Dinheiro::ZERO)
        }
    } else if item.credito_sn.e_positivo() {
        (
            item.aliquota_credito_sn,
            item.valor - item.desconto,
            item.credito_sn,
        )
    } else {
        (None, Dinheiro::ZERO, Dinheiro::ZERO)
    };
    Escriturado {
        cst,
        cfop,
        aliquota,
        base,
        icms,
    }
}

fn escriturar_propria(item: &ItemXml) -> Escriturado {
    Escriturado {
        cst: cst_do_item(item),
        cfop: digitos(&item.cfop),
        aliquota: item.aliquota_icms.filter(|_| item.icms.e_positivo()),
        base: item.base_icms,
        icms: item.icms,
    }
}

/// O que o bloco C acumula para o bloco 0 e o E.
#[derive(Default)]
struct Acumulado {
    participantes: BTreeMap<String, ParticipanteXml>,
    itens: BTreeMap<String, ItemDoCadastro>,
    debitos: Dinheiro,
    creditos: Dinheiro,
    documentos: u32,
    avisos: Vec<String>,
}

impl Acumulado {
    fn participante(&mut self, p: &ParticipanteXml) -> String {
        let codigo = digitos(&p.documento);
        if !codigo.is_empty() {
            self.participantes
                .entry(codigo.clone())
                .or_insert_with(|| p.clone());
        }
        codigo
    }

    /// Fecha o C190 de uma nota: as linhas, os débitos/créditos e os totais para o C100.
    fn analiticos(&mut self, analiticos: &Analiticos, linhas: &mut Vec<Vec<String>>) -> Analitico {
        let mut total = Analitico::default();
        for ((cst, cfop, aliq), a) in analiticos {
            linhas.push(vec![
                "C190".into(),
                cst.clone(),
                cfop.clone(),
                aliquota(Some(Percentual::unidades(*aliq))),
                valor(a.operacao),
                valor(a.base),
                valor(a.icms),
                valor(a.base_st),
                valor(a.st),
                valor(a.reducao),
                valor(a.ipi),
                String::new(),
            ]);
            match cfop.chars().next() {
                Some('5' | '6' | '7') => self.debitos += a.icms,
                Some('1' | '2' | '3') => self.creditos += a.icms,
                _ => {}
            }
            total.operacao += a.operacao;
            total.base += a.base;
            total.icms += a.icms;
            total.base_st += a.base_st;
            total.st += a.st;
            total.ipi += a.ipi;
        }
        total
    }
}

/// O C100 de uma nota com valores.
fn c100(
    doc: &DocumentoXml,
    entrada: bool,
    propria: bool,
    participante: &str,
    dt_e_s: Option<Data>,
    t: &Analitico,
) -> Vec<String> {
    let nfce = doc.modelo == "65";
    let vazio_na_nfce = |v: Dinheiro| if nfce { String::new() } else { valor(v) };
    vec![
        "C100".into(),
        if entrada { "0" } else { "1" }.into(),
        if propria { "0" } else { "1" }.into(),
        if nfce {
            String::new()
        } else {
            participante.to_owned()
        },
        doc.modelo.clone(),
        if doc.finalidade == cardeal_fiscal::FinalidadeNfe::Complementar {
            "06"
        } else {
            "00"
        }
        .into(),
        texto(&doc.serie, 3),
        doc.numero.clone(),
        doc.chave.clone(),
        data(doc.emissao),
        if nfce {
            String::new()
        } else {
            dt_e_s.map(data).unwrap_or_default()
        },
        valor(doc.totais.total),
        if doc.a_prazo { "1" } else { "0" }.into(),
        valor(doc.totais.desconto),
        String::new(),
        valor(doc.totais.produtos),
        texto(&doc.modalidade_frete, 1),
        valor(doc.totais.frete),
        valor(doc.totais.seguro),
        valor(doc.totais.outras),
        valor(t.base),
        valor(t.icms),
        vazio_na_nfce(t.base_st),
        vazio_na_nfce(t.st),
        vazio_na_nfce(t.ipi),
        vazio_na_nfce(doc.totais.pis),
        vazio_na_nfce(doc.totais.cofins),
        String::new(),
        String::new(),
    ]
}

/// O C170 de um item de terceiros.
fn c170(item: &ItemXml, cadastro: &ItemDoCadastro, e: &Escriturado) -> Vec<String> {
    let com_ipi = item.ipi.e_positivo();
    let ipi = |s: String| if com_ipi { s } else { String::new() };
    let mut v = vec![
        "C170".into(),
        item.numero.to_string(),
        texto(&cadastro.codigo, 60),
        String::new(),
        quantidade(item.quantidade),
        texto(&cadastro.unidade, 6),
        valor(item.valor),
        valor(item.desconto),
        "0".into(),
        e.cst.clone(),
        e.cfop.clone(),
        String::new(),
        valor(e.base),
        aliquota(e.aliquota),
        valor(e.icms),
        valor(item.base_st),
        aliquota(item.aliquota_st),
        valor(item.st),
        ipi("0".into()),
        ipi("49".into()),
        String::new(),
        ipi(valor(item.base_ipi)),
        ipi(aliquota(item.aliquota_ipi)),
        ipi(valor(item.ipi)),
    ];
    // PIS e COFINS (25 a 36), a conta contábil (37) e o abatimento (38): fora.
    v.extend(std::iter::repeat_n(String::new(), 14));
    v
}

/// O C100 da nota própria cancelada: só a identificação e a chave.
fn cancelada(
    nota: &NotaPropria,
    doc: Option<&DocumentoXml>,
    rotulo: &str,
    acc: &mut Acumulado,
) -> Vec<Vec<String>> {
    let Some(chave) = nota.chave.clone().or_else(|| doc.map(|d| d.chave.clone())) else {
        acc.avisos.push(format!(
            "{rotulo}: cancelada sem a chave guardada — ficou fora"
        ));
        return Vec::new();
    };
    let entrada = doc.is_some_and(|d| !d.saida);
    let mut l = vec![
        "C100".to_owned(),
        if entrada { "0" } else { "1" }.into(),
        "0".into(),
        String::new(),
        match nota.modelo {
            ModeloFiscal::Nfce => "65",
            ModeloFiscal::Nfe => "55",
        }
        .into(),
        "02".into(),
        nota.serie.to_string(),
        nota.numero.to_string(),
        chave,
    ];
    l.extend(std::iter::repeat_n(String::new(), 20));
    acc.documentos += 1;
    vec![l]
}

fn conferir_total(acc: &mut Acumulado, rotulo: &str, itens: Dinheiro, nota: Dinheiro) {
    if itens != nota {
        acc.avisos.push(format!(
            "{rotulo}: os itens somam {} e a nota, {} — confira",
            itens.formatar_com_simbolo(),
            nota.formatar_com_simbolo()
        ));
    }
}

/// Uma nota emitida pela empresa: o C100 e o C190 (a nota própria não leva C170).
fn propria(nota: &NotaPropria, acc: &mut Acumulado) -> Vec<Vec<String>> {
    let rotulo = format!(
        "{} série {} nº {}",
        match nota.modelo {
            ModeloFiscal::Nfce => "NFC-e",
            ModeloFiscal::Nfe => "NF-e",
        },
        nota.serie,
        nota.numero
    );
    let lido = nota.xml.as_deref().map(ler_para_escriturar);
    if nota.situacao == SituacaoPropria::Cancelada {
        let doc = lido.as_ref().and_then(|d| d.as_ref().ok());
        return cancelada(nota, doc, &rotulo, acc);
    }
    let doc = match lido {
        Some(Ok(doc)) => doc,
        Some(Err(e)) => {
            acc.avisos.push(format!(
                "{rotulo}: o XML guardado não pôde ser lido ({e}) — ficou fora"
            ));
            return Vec::new();
        }
        None => {
            acc.avisos.push(format!(
                "{rotulo}: autorizada sem o XML guardado — busque-o (Notas fiscais) e gere de novo; ficou fora"
            ));
            return Vec::new();
        }
    };
    let participante = doc
        .destinatario
        .as_ref()
        .filter(|_| doc.modelo != "65")
        .map(|p| acc.participante(p))
        .unwrap_or_default();
    let mut analiticos = Analiticos::new();
    for item in &doc.itens {
        somar(&mut analiticos, item, &escriturar_propria(item));
    }
    let mut filhos = Vec::new();
    let t = acc.analiticos(&analiticos, &mut filhos);
    conferir_total(acc, &rotulo, t.operacao, doc.totais.total);
    let dt_e_s = if doc.saida {
        doc.saida_ou_entrada
    } else {
        Some(doc.saida_ou_entrada.unwrap_or(doc.emissao))
    };
    let mut linhas = vec![c100(&doc, !doc.saida, true, &participante, dt_e_s, &t)];
    linhas.extend(filhos);
    acc.documentos += 1;
    linhas
}

/// O produto do cadastro de um item da compra — ou, sem casamento, o do fornecedor (com aviso).
fn cadastro_do_item(
    e: &EntradaEscriturada,
    i: usize,
    item: &ItemXml,
    participante: &str,
    rotulo: &str,
    acc: &mut Acumulado,
) -> ItemDoCadastro {
    e.itens.get(i).cloned().flatten().unwrap_or_else(|| {
        acc.avisos.push(format!(
            "{rotulo}, item {}: sem produto casado — foi com o código do fornecedor",
            item.numero
        ));
        ItemDoCadastro {
            codigo: texto(&format!("{participante}-{}", item.codigo), 60),
            descricao: item.descricao.clone(),
            unidade: item.unidade.clone(),
            ncm: item.ncm.clone(),
            gtin: item.gtin.clone(),
            cest: item.cest.clone(),
        }
    })
}

/// Uma nota de compra: o C100, um C170 por item e o C190.
fn compra(
    e: &EntradaEscriturada,
    cnpj: &str,
    industrial: bool,
    acc: &mut Acumulado,
) -> Vec<Vec<String>> {
    let doc = match ler_para_escriturar(&e.xml) {
        Ok(doc) => doc,
        Err(erro) => {
            acc.avisos.push(format!(
                "Uma nota de compra de {} não pôde ser lida ({erro}) — ficou fora",
                e.data_entrada.formatar()
            ));
            return Vec::new();
        }
    };
    let rotulo = format!(
        "Compra nº {} de {}",
        doc.numero,
        texto(&doc.emitente.nome, 40)
    );
    if digitos(&doc.emitente.documento) == cnpj {
        acc.avisos.push(format!(
            "{rotulo}: emitida pela própria empresa — vai pelas notas emitidas"
        ));
        return Vec::new();
    }
    let participante = acc.participante(&doc.emitente);
    let mut analiticos = Analiticos::new();
    let mut filhos = Vec::new();
    for (i, item) in doc.itens.iter().enumerate() {
        let cadastro = cadastro_do_item(e, i, item, &participante, &rotulo, acc);
        let esc = escriturar_entrada(item, industrial);
        filhos.push(c170(item, &cadastro, &esc));
        somar(&mut analiticos, item, &esc);
        acc.itens
            .entry(texto(&cadastro.codigo, 60))
            .or_insert(cadastro);
    }
    let mut c190 = Vec::new();
    let t = acc.analiticos(&analiticos, &mut c190);
    conferir_total(acc, &rotulo, t.operacao, doc.totais.total);
    let mut linhas = vec![c100(
        &doc,
        true,
        false,
        &participante,
        Some(e.data_entrada),
        &t,
    )];
    linhas.extend(filhos);
    linhas.extend(c190);
    acc.documentos += 1;
    linhas
}

/// O bloco C: as notas próprias e as entradas.
fn bloco_c(d: &DadosDaEfd<'_>, acc: &mut Acumulado) -> Vec<Vec<String>> {
    let cnpj = d.emitente.cnpj.sem_mascara();
    let mut proprias: Vec<&NotaPropria> = d.proprias.iter().collect();
    proprias.sort_by_key(|n| (n.modelo == ModeloFiscal::Nfce, n.serie, n.numero));
    let mut entradas: Vec<&EntradaEscriturada> = d.entradas.iter().collect();
    entradas.sort_by_key(|e| e.data_entrada);
    let mut linhas = Vec::new();
    for nota in proprias {
        linhas.extend(propria(nota, acc));
    }
    for e in entradas {
        linhas.extend(compra(e, &cnpj, d.configuracao.industrial, acc));
    }
    linhas
}

/// Os impedimentos: o que torna o arquivo inválido de saída.
fn impedimentos(d: &DadosDaEfd<'_>) -> Vec<String> {
    let mut v = Vec::new();
    if d.emitente.crt != Crt::RegimeNormal {
        v.push(EFD_DISPENSADA.to_owned());
    }
    let ie = digitos(&d.emitente.inscricao_estadual);
    if ie.is_empty() {
        v.push("A empresa está sem inscrição estadual (Configurações › Fiscal).".to_owned());
    }
    if digitos(&d.emitente.endereco.codigo_municipio).len() != 7 {
        v.push(
            "O código IBGE do município da empresa está faltando (Configurações › Fiscal)."
                .to_owned(),
        );
    }
    let falta = d.configuracao.pendencias();
    if !falta.is_empty() {
        v.push(format!(
            "Falta na configuração do SPED: {}.",
            falta.join(", ")
        ));
    }
    v
}

/// A abertura e a identificação: 0000, 0001, 0002 (indústria), 0005 e 0100.
fn abertura(d: &DadosDaEfd<'_>, arq: &mut Arquivo) {
    let empresa = d.emitente;
    let endereco = &empresa.endereco;
    let contador = &d.configuracao.contabilista;
    let mes = d.competencia;
    arq.no_bloco = 0;
    arq.linha(&[
        "0000".into(),
        codigo_da_versao(mes.ano()),
        if d.substituta { "1" } else { "0" }.into(),
        data(mes.primeiro_dia()),
        data(mes.ultimo_dia()),
        texto(&empresa.razao_social, 100),
        empresa.cnpj.sem_mascara(),
        String::new(),
        endereco.uf.sigla().into(),
        digitos(&empresa.inscricao_estadual),
        digitos(&endereco.codigo_municipio),
        empresa
            .inscricao_municipal
            .as_deref()
            .map(|im| texto(im, 30))
            .unwrap_or_default(),
        String::new(),
        d.configuracao.perfil.letra().into(),
        if d.configuracao.industrial { "0" } else { "1" }.into(),
    ]);
    arq.l(&["0001", "0"]);
    if d.configuracao.industrial {
        arq.linha(&[
            "0002".into(),
            digitos(&d.configuracao.classificacao_industrial),
        ]);
    }
    arq.linha(&[
        "0005".into(),
        texto(&empresa.nome_fantasia, 60),
        digitos(&endereco.cep),
        texto(&endereco.logradouro, 60),
        texto(&endereco.numero, 10),
        texto(&endereco.complemento, 60),
        texto(&endereco.bairro, 60),
        endereco
            .telefone
            .as_deref()
            .map(digitos)
            .unwrap_or_default(),
        String::new(),
        texto(d.email, 100),
    ]);
    arq.linha(&[
        "0100".into(),
        texto(&contador.nome, 100),
        digitos(&contador.cpf),
        texto(&contador.crc, 15),
        digitos(&contador.cnpj),
        digitos(&contador.cep),
        texto(&contador.endereco, 60),
        texto(&contador.numero, 10),
        texto(&contador.complemento, 60),
        texto(&contador.bairro, 60),
        digitos(&contador.telefone),
        String::new(),
        texto(&contador.email, 100),
        digitos(&contador.codigo_municipio),
    ]);
}

/// Os cadastros que o bloco C usa: 0150, 0190 e 0200.
fn cadastros(acc: &Acumulado, industrial: bool, arq: &mut Arquivo) {
    for (codigo, p) in &acc.participantes {
        let (cnpj, cpf) = if p.e_cnpj() {
            (codigo.clone(), String::new())
        } else {
            (String::new(), codigo.clone())
        };
        arq.linha(&[
            "0150".into(),
            codigo.clone(),
            texto(&p.nome, 100),
            "01058".into(),
            cnpj,
            cpf,
            p.inscricao_estadual
                .as_deref()
                .map(digitos)
                .unwrap_or_default(),
            digitos(&p.codigo_municipio),
            String::new(),
            texto(&p.logradouro, 60),
            texto(&p.numero, 10),
            texto(&p.complemento, 60),
            texto(&p.bairro, 60),
        ]);
    }
    let unidades: std::collections::BTreeSet<String> =
        acc.itens.values().map(|i| texto(&i.unidade, 6)).collect();
    for u in &unidades {
        arq.linha(&["0190".into(), u.clone(), u.clone()]);
    }
    for (codigo, item) in &acc.itens {
        let ncm = digitos(&item.ncm);
        let genero = if ncm.len() == 8 {
            ncm[..2].to_owned()
        } else {
            String::new()
        };
        arq.linha(&[
            "0200".into(),
            codigo.clone(),
            texto(&item.descricao, 200),
            item.gtin.as_deref().map(digitos).unwrap_or_default(),
            String::new(),
            texto(&item.unidade, 6),
            if industrial { "01" } else { "00" }.into(),
            ncm,
            String::new(),
            genero,
            String::new(),
            String::new(),
            item.cest.as_deref().map(digitos).unwrap_or_default(),
        ]);
    }
}

fn bloco_0(d: &DadosDaEfd<'_>, acc: &Acumulado, arq: &mut Arquivo) {
    abertura(d, arq);
    cadastros(acc, d.configuracao.industrial, arq);
    arq.fechar("0990");
}

fn apurar(acc: &Acumulado, saldo_anterior: Dinheiro) -> ApuracaoIcms {
    let saldo = acc.debitos - acc.creditos - saldo_anterior;
    let positivo = saldo.nao_negativo();
    ApuracaoIcms {
        debitos: acc.debitos,
        creditos: acc.creditos,
        saldo_credor_anterior: saldo_anterior,
        saldo_apurado: positivo,
        a_recolher: positivo,
        saldo_credor_a_transportar: (Dinheiro::ZERO - saldo).nao_negativo(),
    }
}

fn bloco_e(d: &DadosDaEfd<'_>, ap: &ApuracaoIcms, a: &mut Arquivo, avisos: &mut Vec<String>) {
    let m = d.competencia;
    a.abrir("E001", true);
    a.linha(&["E100".into(), data(m.primeiro_dia()), data(m.ultimo_dia())]);
    let z = valor(Dinheiro::ZERO);
    a.linha(&[
        "E110".into(),
        valor(ap.debitos),
        z.clone(),
        z.clone(),
        z.clone(),
        valor(ap.creditos),
        z.clone(),
        z.clone(),
        z.clone(),
        valor(ap.saldo_credor_anterior),
        valor(ap.saldo_apurado),
        z.clone(),
        valor(ap.a_recolher),
        valor(ap.saldo_credor_a_transportar),
        z,
    ]);
    if ap.a_recolher.e_positivo() {
        let seguinte = m.proxima();
        let dia = d.configuracao.dia_vencimento;
        let vencimento = (1..=31).contains(&dia).then(|| {
            Data::de_ymd(seguinte.ano(), seguinte.mes(), dia)
                .unwrap_or_else(|_| seguinte.ultimo_dia())
        });
        if vencimento.is_none() {
            avisos.push(
                "Sem o dia do vencimento do ICMS na configuração: o E116 saiu sem a data.".into(),
            );
        }
        if d.configuracao.codigo_receita.trim().is_empty() {
            avisos.push(
                "Sem o código de receita do ICMS na configuração: o E116 saiu sem ele.".into(),
            );
        }
        a.linha(&[
            "E116".into(),
            "000".into(),
            valor(ap.a_recolher),
            vencimento.map(data).unwrap_or_default(),
            texto(&d.configuracao.codigo_receita, 30),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            format!("{:02}{:04}", m.mes(), m.ano()),
        ]);
    }
    a.fechar("E990");
}

fn bloco_9(a: &mut Arquivo) {
    a.abrir("9001", true);
    let mut tipos: Vec<(String, usize)> = a.por_registro.clone();
    tipos.push(("9900".into(), 0));
    tipos.push(("9990".into(), 1));
    tipos.push(("9999".into(), 1));
    let n_9900 = tipos.len();
    for (reg, n) in &tipos {
        let n = if reg == "9900" { n_9900 } else { *n };
        a.linha(&["9900".into(), reg.clone(), n.to_string()]);
    }
    // O bloco 9 conta o 9990 e o 9999.
    let n = a.no_bloco + 2;
    a.l(&["9990", &n.to_string()]);
    let total = a.total + 1;
    a.l(&["9999", &total.to_string()]);
}

/// Gera o arquivo do mês — ou diz o que impede.
///
/// # Errors
/// Os impedimentos (regime, inscrição estadual, município, configuração).
pub fn gerar_efd(d: &DadosDaEfd<'_>) -> Result<ArquivoEfd, Vec<String>> {
    let imp = impedimentos(d);
    if !imp.is_empty() {
        return Err(imp);
    }
    let mut acc = Acumulado::default();
    let c = bloco_c(d, &mut acc);
    let apuracao = apurar(&acc, d.saldo_credor_anterior);
    let mut avisos = std::mem::take(&mut acc.avisos);

    let mut a = Arquivo::default();
    bloco_0(d, &acc, &mut a);
    a.vazio("B");
    a.abrir("C001", !c.is_empty());
    for l in &c {
        a.linha(l);
    }
    a.fechar("C990");
    a.vazio("D");
    bloco_e(d, &apuracao, &mut a, &mut avisos);
    a.vazio("G");
    a.vazio("H");
    a.vazio("K");
    a.abrir("1001", true);
    let mut l1010 = vec!["1010".to_owned()];
    l1010.extend(std::iter::repeat_n("N".to_owned(), 13));
    a.linha(&l1010);
    a.fechar("1990");
    bloco_9(&mut a);

    if d.competencia.mes() == 2 {
        avisos.push(
            "Fevereiro: o inventário de 31/12 (bloco H) vai nesta EFD e não foi gerado — o \
             contador inclui."
                .into(),
        );
    }
    if d.competencia.ano() > 2026 {
        avisos.push(format!(
            "Confira o código da versão do leiaute para {} (saiu {}).",
            d.competencia.ano(),
            codigo_da_versao(d.competencia.ano())
        ));
    }
    let nome = format!(
        "EFD_ICMS_IPI_{}_{:04}{:02}.txt",
        d.emitente.cnpj.sem_mascara(),
        d.competencia.ano(),
        d.competencia.mes()
    );
    Ok(ArquivoEfd {
        nome,
        texto: a.texto,
        apuracao,
        documentos: acc.documentos,
        avisos,
    })
}

// ── comandos e consultas ─────────────────────────────────────────────────────

/// Grava a configuração do SPED.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigurarEfd {
    /// A configuração.
    pub configuracao: ConfiguracaoEfd,
}

impl Comando for ConfigurarEfd {
    type Saida = ();
    const PERMISSAO: &'static str = "fiscal.configurar";
    const RISCO: Risco = Risco::Baixo;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<()> {
        sqlite::configurar(&self, ctx, uow)
    }
}

/// A configuração do SPED (a padrão, se nunca foi gravada).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ConfiguracaoDaEfd;

impl Consulta for ConfiguracaoDaEfd {
    type Saida = ConfiguracaoEfd;
    const PERMISSAO: &'static str = "fiscal.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<ConfiguracaoEfd> {
        sqlite::configuracao(c, ctx.empresa)
    }
}

/// Gera a EFD ICMS/IPI do mês. As notas de compra vêm de fora (`compras` as entrega já com o
/// produto de cada item); as emitidas, do próprio módulo. Sem `saldo_credor_anterior`, vale o da
/// apuração registrada do mês anterior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GerarEfd {
    /// O mês.
    pub competencia: Competencia,
    /// Arquivo substituto de um já entregue.
    pub substituta: bool,
    /// As notas de compra que entraram no mês.
    pub entradas: Vec<EntradaEscriturada>,
    /// O saldo credor do mês anterior, quando quem gera o informa.
    pub saldo_credor_anterior: Option<Dinheiro>,
}

/// O arquivo do mês — ou o que impede.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EfdDoMes {
    /// O arquivo.
    pub arquivo: Option<ArquivoEfd>,
    /// O que impede de gerar.
    pub impedimentos: Vec<String>,
}

impl Consulta for GerarEfd {
    type Saida = EfdDoMes;
    const PERMISSAO: &'static str = "fiscal.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<EfdDoMes> {
        sqlite::gerar(&self, ctx, c)
    }
}

/// Registra a apuração do mês (o arquivo entregue): o saldo credor dela abre o mês seguinte.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RegistrarApuracaoIcms {
    /// O mês.
    pub competencia: Competencia,
    /// A apuração.
    pub apuracao: ApuracaoIcms,
}

impl Comando for RegistrarApuracaoIcms {
    type Saida = ();
    const PERMISSAO: &'static str = "fiscal.configurar";
    const RISCO: Risco = Risco::Baixo;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<()> {
        sqlite::registrar(&self, ctx, uow)
    }
}

#[cfg(feature = "sqlite")]
mod sqlite;
#[cfg(test)]
pub(crate) mod testes;
