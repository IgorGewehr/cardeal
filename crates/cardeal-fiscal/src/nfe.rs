//! Leitura dos campos essenciais de um XML de NF-e 4.00 — **não** o esquema completo (sem
//! impostos discriminados, sem transporte, sem duplicatas): só o que `mod-compras` precisa
//! para montar a `NotaEntrada` em conferência (`docs/modulos/compras.md` §3).
//!
//! Feito com `quick-xml` em modo evento — sem validar contra o XSD oficial. Um XML fora do
//! padrão (tag essencial ausente, chave malformada) vira [`ErroFiscal::XmlInvalido`], nunca
//! panic.

use cardeal_kernel::{Cnpj, Data, Dinheiro, Preco, Quantidade};
use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::chave::ChaveAcesso;
use crate::erros::ErroFiscal;

/// Um item (`det/prod`) de uma NF-e.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemNfe {
    /// O código do produto no cadastro do fornecedor (`cProd`).
    pub codigo_fornecedor: String,
    /// A descrição do fornecedor (`xProd`).
    pub descricao: String,
    /// O NCM (8 dígitos).
    pub ncm: String,
    /// O código de barras (GTIN, `det/prod/cEAN`) — `None` quando o campo vem ausente ou com
    /// o literal `"SEM GTIN"` que a NF-e usa para produto sem código de barras. Usado por
    /// `mod-compras` como a primeira tentativa da cascata de casamento (mais forte que a
    /// regra aprendida: um GTIN batendo é o próprio fabricante confirmando o produto).
    pub codigo_barras: Option<String>,
    /// A quantidade comercial (`qCom`).
    pub quantidade: Quantidade,
    /// O valor unitário comercial (`vUnCom`).
    pub valor_unitario: Preco,
    /// O valor total do item (`vProd`).
    pub valor_total: Dinheiro,
    /// O desconto do item (`det/prod/vDesc`); zero sem desconto.
    pub valor_desconto: Dinheiro,
    /// O ICMS de substituição tributária cobrado na nota para este item (`vICMSST` + `vFCPST`
    /// de `det/imposto/ICMS/*`) — quem compra para revender não recupera: é custo. Não inclui o
    /// ST **retido anteriormente** (`vICMSSTRet`), que já está embutido no preço.
    pub valor_icms_st: Dinheiro,
    /// O IPI do item (`det/imposto/IPI/IPITrib/vIPI`) — para o comércio, custo.
    pub valor_ipi: Dinheiro,
    /// O CFOP do item (`det/prod/CFOP`), na ótica de quem emitiu.
    pub cfop: Option<String>,
}

impl ItemNfe {
    /// O quanto o item soma no total da nota: produto − desconto + ST + IPI.
    #[must_use]
    pub fn valor_na_nota(&self) -> Dinheiro {
        self.valor_total - self.valor_desconto + self.valor_icms_st + self.valor_ipi
    }
}

/// Os campos essenciais de uma NF-e, extraídos do XML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotaFiscalXml {
    /// A chave de acesso (atributo `Id` de `infNFe`, sem o prefixo `"NFe"`).
    pub chave_acesso: ChaveAcesso,
    /// Número da nota (`ide/nNF`).
    pub numero: String,
    /// Série (`ide/serie`).
    pub serie: String,
    /// Data de emissão (`ide/dhEmi`, só a parte de data).
    pub emissao: Data,
    /// CNPJ do emitente (`emit/CNPJ`).
    pub fornecedor_cnpj: Cnpj,
    /// Razão social do emitente (`emit/xNome`).
    pub fornecedor_nome: String,
    /// Os itens (`det/prod`), na ordem do XML.
    pub itens: Vec<ItemNfe>,
    /// Valor do frete (`total/ICMSTot/vFrete`).
    pub valor_frete: Dinheiro,
    /// Valor do seguro (`total/ICMSTot/vSeg`).
    pub valor_seguro: Dinheiro,
    /// Outras despesas acessórias (`total/ICMSTot/vOutro`).
    pub valor_outras_despesas: Dinheiro,
    /// Valor total da nota (`total/ICMSTot/vNF`).
    pub valor_total: Dinheiro,
    /// `ide/tpNF = 0`: uma nota **de entrada** de quem a emitiu — quem recebe a mercadoria é o
    /// emitente, não o destinatário.
    pub entrada_do_emitente: bool,
    /// A finalidade (`ide/finNFe`).
    pub finalidade: FinalidadeNfe,
    /// O documento do destinatário (`dest/CNPJ` ou `dest/CPF`, só dígitos), quando há.
    pub destinatario: Option<String>,
    /// As duplicatas (`cobr/dup`): como o fornecedor vai cobrar — vencimento e valor de cada
    /// parcela. Vazio quando a nota não traz cobrança (paga à vista, ou boleto à parte).
    pub duplicatas: Vec<Duplicata>,
}

/// Uma duplicata da nota (`cobr/dup`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Duplicata {
    /// O número (`nDup`), como veio — `"001"`, `"1234/1"`… Vazio se ausente.
    pub numero: String,
    /// O vencimento (`dVenc`).
    pub vencimento: Data,
    /// O valor (`vDup`).
    pub valor: Dinheiro,
}

/// A finalidade da NF-e (`ide/finNFe`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FinalidadeNfe {
    /// 1 — normal.
    #[default]
    Normal,
    /// 2 — complementar.
    Complementar,
    /// 3 — de ajuste.
    Ajuste,
    /// 4 — devolução de mercadoria.
    Devolucao,
}

impl FinalidadeNfe {
    fn de_codigo(codigo: &str) -> Self {
        match codigo.trim() {
            "2" => Self::Complementar,
            "3" => Self::Ajuste,
            "4" => Self::Devolucao,
            _ => Self::Normal,
        }
    }
}

/// O que uma NF-e recebida é para a empresa que a importa como compra. Lição do `gestao-raiz`
/// (`classifyNote`): o `tpNF` **não** decide sozinho — a venda do fornecedor vem com `tpNF = 1`
/// (saída **dele**) e, para nós, é compra. Decide quem emitiu e quem recebe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassificacaoEntrada {
    /// O fornecedor nos vendeu: entra como compra.
    Compra,
    /// Nós emitimos esta nota (venda, remessa, devolução nossa).
    SaidaPropria,
    /// O destinatário é outra empresa ou pessoa.
    DeOutroDestinatario,
    /// Um cliente devolvendo mercadoria (`finNFe = 4`) — entra no estoque, mas não é compra
    /// e não gera conta a pagar.
    DevolucaoDeCliente,
    /// Nota de entrada **do emitente** (`tpNF = 0`): a mercadoria sai de nós para ele (ele nos
    /// comprou e emitiu a própria entrada, por exemplo).
    EntradaDoEmitente,
}

impl ClassificacaoEntrada {
    /// Por que esta nota não entra como compra — `None` quando entra.
    #[must_use]
    pub const fn motivo_da_recusa(self) -> Option<&'static str> {
        match self {
            Self::Compra => None,
            Self::SaidaPropria => Some(
                "ela foi emitida pela sua própria empresa (é uma saída sua, não uma compra)",
            ),
            Self::DeOutroDestinatario => {
                Some("o destinatário dela não é a sua empresa")
            }
            Self::DevolucaoDeCliente => Some(
                "é uma devolução de mercadoria de um cliente (finalidade 4), não uma compra",
            ),
            Self::EntradaDoEmitente => Some(
                "é uma nota de entrada de quem a emitiu (a mercadoria saiu da sua empresa para ele)",
            ),
        }
    }
}

impl NotaFiscalXml {
    /// O que esta nota é para a empresa de CNPJ `empresa`. Sem destinatário no XML (resumo,
    /// XML incompleto), não há como conferir: segue como compra.
    #[must_use]
    pub fn classificar_para(&self, empresa: &Cnpj) -> ClassificacaoEntrada {
        let nossa = empresa.sem_mascara();
        if self.fornecedor_cnpj.sem_mascara() == nossa {
            ClassificacaoEntrada::SaidaPropria
        } else if self.destinatario.as_deref().is_some_and(|d| d != nossa) {
            ClassificacaoEntrada::DeOutroDestinatario
        } else if self.finalidade == FinalidadeNfe::Devolucao {
            ClassificacaoEntrada::DevolucaoDeCliente
        } else if self.entrada_do_emitente {
            ClassificacaoEntrada::EntradaDoEmitente
        } else {
            ClassificacaoEntrada::Compra
        }
    }

    /// Os CFOPs dos itens que **não** são venda de mercadoria (remessa para conserto,
    /// bonificação, consignação, faturamento de entrega futura…), sem repetir, na ordem dos
    /// itens. Uma nota com eles não deve entrar sozinha no estoque com conta a pagar.
    #[must_use]
    pub fn cfops_fora_de_venda(&self) -> Vec<String> {
        let mut cfops: Vec<String> = Vec::new();
        for c in self.itens.iter().filter_map(|i| i.cfop.as_deref()) {
            if !cfop_de_venda(c) && !cfops.iter().any(|x| x == c) {
                cfops.push(c.to_owned());
            }
        }
        cfops
    }
}

/// O CFOP (de quem emitiu) é de **venda de mercadoria**: `x10x`–`x12x` (vendas; as `x15x` são
/// transferências), `x401`–`x405` (vendas com substituição tributária) e `x651`–`x656`
/// (combustíveis), em operação de saída (`5`, `6` ou `7`).
#[must_use]
pub fn cfop_de_venda(cfop: &str) -> bool {
    let digitos: Vec<u32> = cfop.trim().chars().filter_map(|c| c.to_digit(10)).collect();
    let [operacao, grupo, subgrupo, item] = digitos[..] else {
        return false;
    };
    if !matches!(operacao, 5..=7) {
        return false;
    }
    match (grupo, subgrupo) {
        (1, 0..=2) => true,
        (4, 0) => matches!(item, 1..=3 | 5),
        (6, 5) => matches!(item, 1..=6),
        _ => false,
    }
}

/// O que um CFOP fora de venda significa, para o aviso na conferência.
#[must_use]
pub fn descrever_cfop(cfop: &str) -> &'static str {
    match cfop.trim().get(1..).unwrap_or("") {
        "910" => "bonificação, doação ou brinde",
        "911" => "amostra grátis",
        "912" => "remessa para demonstração",
        "913" => "retorno de demonstração",
        "915" => "remessa para conserto ou reparo",
        "916" => "retorno de conserto ou reparo",
        "917" => "remessa em consignação",
        "908" => "remessa em comodato",
        "901" => "remessa para industrialização",
        "922" => "faturamento de venda para entrega futura (a mercadoria vem em outra nota)",
        "933" => "prestação de serviço",
        "949" => "outra saída que não é venda",
        "151" | "152" | "155" | "156" => "transferência entre estabelecimentos",
        "551" => "venda de ativo imobilizado",
        "556" => "devolução de material de uso e consumo",
        _ => "operação que não é venda de mercadoria",
    }
}

fn campo_ausente(nome: &str) -> ErroFiscal {
    ErroFiscal::XmlInvalido(format!("campo obrigatório ausente: {nome}"))
}

fn dinheiro_de(texto: &str, campo: &str) -> Result<Dinheiro, ErroFiscal> {
    Dinheiro::de_str(texto)
        .map_err(|_| ErroFiscal::XmlInvalido(format!("{campo} inválido: {texto}")))
}

fn data_de_dh_emi(dh_emi: &str) -> Result<Data, ErroFiscal> {
    let erro = || ErroFiscal::XmlInvalido(format!("dhEmi inválido: {dh_emi}"));
    let data = dh_emi.get(0..10).ok_or_else(erro)?;
    let ano: i32 = data
        .get(0..4)
        .and_then(|s| s.parse().ok())
        .ok_or_else(erro)?;
    let mes: u32 = data
        .get(5..7)
        .and_then(|s| s.parse().ok())
        .ok_or_else(erro)?;
    let dia: u32 = data
        .get(8..10)
        .and_then(|s| s.parse().ok())
        .ok_or_else(erro)?;
    Data::de_ymd(ano, mes, dia).map_err(|_| erro())
}

/// Interpreta o XML de uma NF-e, extraindo só os campos essenciais.
///
/// # Errors
/// [`ErroFiscal::XmlInvalido`] se o XML não puder ser lido ou faltar um campo obrigatório.
pub fn interpretar(xml: &str) -> Result<NotaFiscalXml, ErroFiscal> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut chave_acesso = None;
    let mut numero = None;
    let mut serie = None;
    let mut emissao = None;
    let mut fornecedor_cnpj = None;
    let mut fornecedor_nome = None;
    let mut valor_frete = Dinheiro::ZERO;
    let mut valor_seguro = Dinheiro::ZERO;
    let mut valor_outras_despesas = Dinheiro::ZERO;
    let mut valor_total = None;
    let mut itens = Vec::new();
    let mut ide = Identificacao::default();
    let mut cobranca = Cobranca::default();

    // Pilha de nomes de tag abertos — dá contexto para tags de nome repetido em seções
    // diferentes (nenhuma colide aqui, mas a pilha deixa o parser honesto sobre onde está).
    let mut pilha: Vec<String> = Vec::new();
    let mut item_atual: Option<ItemParcial> = None;

    loop {
        match reader
            .read_event()
            .map_err(|e| ErroFiscal::XmlInvalido(e.to_string()))?
        {
            Event::Eof => break,
            Event::Start(e) => {
                let nome = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if nome == "infNFe" {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"Id" {
                            let valor = attr
                                .decode_and_unescape_value(reader.decoder())
                                .unwrap_or_default();
                            let sem_prefixo = valor.strip_prefix("NFe").unwrap_or(&valor);
                            chave_acesso = Some(ChaveAcesso::nova(sem_prefixo)?);
                        }
                    }
                }
                if nome == "det" {
                    item_atual = Some(ItemParcial::default());
                }
                cobranca.abrir(&nome, &pilha);
                pilha.push(nome);
            }
            Event::End(e) => {
                let nome = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if nome == "det" {
                    if let Some(parcial) = item_atual.take() {
                        itens.push(parcial.finalizar()?);
                    }
                }
                cobranca.fechar(&nome)?;
                pilha.pop();
            }
            Event::Text(t) => {
                let texto = t.unescape().unwrap_or_default().into_owned();
                if texto.trim().is_empty() {
                    continue;
                }
                let caminho: Vec<&str> = pilha.iter().map(String::as_str).collect();
                if cobranca.ler(&caminho, &texto)? {
                    continue;
                }
                processar_texto(
                    &caminho,
                    &texto,
                    &mut numero,
                    &mut serie,
                    &mut emissao,
                    &mut fornecedor_cnpj,
                    &mut fornecedor_nome,
                    &mut valor_frete,
                    &mut valor_seguro,
                    &mut valor_outras_despesas,
                    &mut valor_total,
                    &mut item_atual,
                    &mut ide,
                )?;
            }
            // Inclui `Event::Empty` (`<tag/>` autofechada): nenhuma tag deste XML que nos
            // importa é vazia (todas carregam texto), então só ignoramos — e nunca empilha,
            // porque uma tag autofechada não tem `Event::End` para desempilhar depois.
            _ => {}
        }
    }

    Ok(NotaFiscalXml {
        chave_acesso: chave_acesso.ok_or_else(|| campo_ausente("infNFe/Id"))?,
        numero: numero.ok_or_else(|| campo_ausente("ide/nNF"))?,
        serie: serie.ok_or_else(|| campo_ausente("ide/serie"))?,
        emissao: emissao.ok_or_else(|| campo_ausente("ide/dhEmi"))?,
        fornecedor_cnpj: fornecedor_cnpj.ok_or_else(|| campo_ausente("emit/CNPJ"))?,
        fornecedor_nome: fornecedor_nome.ok_or_else(|| campo_ausente("emit/xNome"))?,
        itens,
        valor_frete,
        valor_seguro,
        valor_outras_despesas,
        valor_total: valor_total.ok_or_else(|| campo_ausente("total/ICMSTot/vNF"))?,
        entrada_do_emitente: ide.entrada_do_emitente,
        finalidade: ide.finalidade,
        destinatario: ide.destinatario,
        duplicatas: cobranca.duplicatas,
    })
}

/// As duplicatas (`cobr/dup`) sendo lidas.
#[derive(Default)]
struct Cobranca {
    duplicatas: Vec<Duplicata>,
    atual: Option<DuplicataParcial>,
}

impl Cobranca {
    fn abrir(&mut self, nome: &str, pilha: &[String]) {
        if nome == "dup" && pilha.last().is_some_and(|p| p == "cobr") {
            self.atual = Some(DuplicataParcial::default());
        }
    }

    fn fechar(&mut self, nome: &str) -> Result<(), ErroFiscal> {
        if nome == "dup" {
            if let Some(parcial) = self.atual.take() {
                self.duplicatas.push(parcial.finalizar()?);
            }
        }
        Ok(())
    }

    /// Lê o texto quando ele é de uma duplicata; `true` se era.
    fn ler(&mut self, caminho: &[&str], texto: &str) -> Result<bool, ErroFiscal> {
        match &mut self.atual {
            Some(dup) => dup.ler(caminho, texto).map(|()| true),
            None => Ok(false),
        }
    }
}

/// Uma duplicata sendo lida.
#[derive(Default)]
struct DuplicataParcial {
    numero: String,
    vencimento: Option<Data>,
    valor: Option<Dinheiro>,
}

impl DuplicataParcial {
    fn ler(&mut self, caminho: &[&str], texto: &str) -> Result<(), ErroFiscal> {
        match caminho {
            [.., "dup", "nDup"] => texto.trim().clone_into(&mut self.numero),
            [.., "dup", "dVenc"] => {
                self.vencimento =
                    Some(data_de_dh_emi(texto.trim()).map_err(|_| {
                        ErroFiscal::XmlInvalido(format!("dVenc inválido: {texto}"))
                    })?);
            }
            [.., "dup", "vDup"] => self.valor = Some(dinheiro_de(texto, "vDup")?),
            _ => {}
        }
        Ok(())
    }

    fn finalizar(self) -> Result<Duplicata, ErroFiscal> {
        Ok(Duplicata {
            numero: self.numero,
            vencimento: self
                .vencimento
                .ok_or_else(|| campo_ausente("cobr/dup/dVenc"))?,
            valor: self.valor.ok_or_else(|| campo_ausente("cobr/dup/vDup"))?,
        })
    }
}

/// Tipo, finalidade e destinatário — o que diz se a nota é mesmo uma compra.
#[derive(Default)]
struct Identificacao {
    entrada_do_emitente: bool,
    finalidade: FinalidadeNfe,
    destinatario: Option<String>,
}

#[derive(Default)]
struct ItemParcial {
    codigo_fornecedor: Option<String>,
    descricao: Option<String>,
    ncm: Option<String>,
    codigo_barras: Option<String>,
    quantidade: Option<Quantidade>,
    valor_unitario: Option<Preco>,
    valor_total: Option<Dinheiro>,
    valor_desconto: Dinheiro,
    valor_icms_st: Dinheiro,
    valor_ipi: Dinheiro,
    cfop: Option<String>,
}

/// O literal que a NF-e usa em `cEAN`/`cEANTrib` para "este produto não tem código de
/// barras" — não é um GTIN de verdade, então vira `None`.
const SEM_GTIN: &str = "SEM GTIN";

impl ItemParcial {
    fn finalizar(self) -> Result<ItemNfe, ErroFiscal> {
        Ok(ItemNfe {
            codigo_fornecedor: self
                .codigo_fornecedor
                .ok_or_else(|| campo_ausente("det/prod/cProd"))?,
            descricao: self
                .descricao
                .ok_or_else(|| campo_ausente("det/prod/xProd"))?,
            ncm: self.ncm.ok_or_else(|| campo_ausente("det/prod/NCM"))?,
            codigo_barras: self.codigo_barras,
            quantidade: self
                .quantidade
                .ok_or_else(|| campo_ausente("det/prod/qCom"))?,
            valor_unitario: self
                .valor_unitario
                .ok_or_else(|| campo_ausente("det/prod/vUnCom"))?,
            valor_total: self
                .valor_total
                .ok_or_else(|| campo_ausente("det/prod/vProd"))?,
            valor_desconto: self.valor_desconto,
            valor_icms_st: self.valor_icms_st,
            valor_ipi: self.valor_ipi,
            cfop: self.cfop,
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn processar_texto(
    caminho: &[&str],
    texto: &str,
    numero: &mut Option<String>,
    serie: &mut Option<String>,
    emissao: &mut Option<Data>,
    fornecedor_cnpj: &mut Option<Cnpj>,
    fornecedor_nome: &mut Option<String>,
    valor_frete: &mut Dinheiro,
    valor_seguro: &mut Dinheiro,
    valor_outras_despesas: &mut Dinheiro,
    valor_total: &mut Option<Dinheiro>,
    item_atual: &mut Option<ItemParcial>,
    ide: &mut Identificacao,
) -> Result<(), ErroFiscal> {
    match caminho {
        [.., "ide", "tpNF"] => ide.entrada_do_emitente = texto.trim() == "0",
        [.., "ide", "finNFe"] => ide.finalidade = FinalidadeNfe::de_codigo(texto),
        [.., "dest", "CNPJ" | "CPF"] => {
            ide.destinatario = Some(texto.chars().filter(char::is_ascii_digit).collect());
        }
        [.., "prod", "CFOP"] => set_item(item_atual, |i| i.cfop = Some(texto.trim().to_string())),
        [.., "ide", "nNF"] => *numero = Some(texto.to_string()),
        [.., "ide", "serie"] => *serie = Some(texto.to_string()),
        [.., "ide", "dhEmi"] => *emissao = Some(data_de_dh_emi(texto)?),
        [.., "emit", "CNPJ"] => {
            *fornecedor_cnpj = Some(
                Cnpj::novo(texto)
                    .map_err(|_| ErroFiscal::XmlInvalido(format!("CNPJ inválido: {texto}")))?,
            );
        }
        [.., "emit", "xNome"] => *fornecedor_nome = Some(texto.to_string()),
        [.., "ICMSTot", "vFrete"] => *valor_frete = dinheiro_de(texto, "vFrete")?,
        [.., "ICMSTot", "vSeg"] => *valor_seguro = dinheiro_de(texto, "vSeg")?,
        [.., "ICMSTot", "vOutro"] => *valor_outras_despesas = dinheiro_de(texto, "vOutro")?,
        [.., "ICMSTot", "vNF"] => *valor_total = Some(dinheiro_de(texto, "vNF")?),
        [.., "prod", "cProd"] => {
            set_item(item_atual, |i| {
                i.codigo_fornecedor = Some(texto.to_string());
            });
        }
        [.., "prod", "xProd"] => set_item(item_atual, |i| i.descricao = Some(texto.to_string())),
        [.., "prod", "NCM"] => set_item(item_atual, |i| i.ncm = Some(texto.to_string())),
        // Só `cEAN` (a embalagem/unidade comercializada), nunca `cEANTrib` (a unidade
        // tributável, que pode ser um GTIN diferente para fracionados) — é `cEAN` que
        // identifica o item que `mod-compras` está casando contra o cadastro do estoque.
        [.., "prod", "cEAN"] => {
            let gtin = texto.trim();
            set_item(item_atual, |i| {
                i.codigo_barras = if gtin.is_empty() || gtin.eq_ignore_ascii_case(SEM_GTIN) {
                    None
                } else {
                    Some(gtin.to_string())
                };
            });
        }
        [.., "prod", "qCom"] => {
            let q = Quantidade::de_str(texto)
                .map_err(|_| ErroFiscal::XmlInvalido(format!("qCom inválido: {texto}")))?;
            if !q.e_positiva() {
                return Err(ErroFiscal::XmlInvalido(format!(
                    "qCom deve ser maior que zero: {texto}"
                )));
            }
            set_item(item_atual, |i| i.quantidade = Some(q));
        }
        [.., "prod", "vUnCom"] => {
            let p = Preco::de_str(texto)
                .map_err(|_| ErroFiscal::XmlInvalido(format!("vUnCom inválido: {texto}")))?;
            if p.unidades_internas() < 0 {
                return Err(ErroFiscal::XmlInvalido(format!(
                    "vUnCom não pode ser negativo: {texto}"
                )));
            }
            set_item(item_atual, |i| i.valor_unitario = Some(p));
        }
        [.., "prod", "vProd"] => {
            let v = dinheiro_de(texto, "vProd")?;
            set_item(item_atual, |i| i.valor_total = Some(v));
        }
        [.., "prod", "vDesc"] => {
            let v = dinheiro_de(texto, "vDesc")?;
            set_item(item_atual, |i| i.valor_desconto = v);
        }
        // ST cobrado nesta nota (ICMS10/30/70/90, ICMSSN201/202/900, ICMSPart) e o FCP dele.
        [.., "imposto", "ICMS", _, "vICMSST" | "vFCPST"] => {
            let v = dinheiro_de(texto, "vICMSST")?;
            set_item(item_atual, |i| i.valor_icms_st += v);
        }
        [.., "imposto", "IPI", "IPITrib", "vIPI"] => {
            let v = dinheiro_de(texto, "vIPI")?;
            set_item(item_atual, |i| i.valor_ipi = v);
        }
        _ => {}
    }
    Ok(())
}

fn set_item(item_atual: &mut Option<ItemParcial>, aplicar: impl FnOnce(&mut ItemParcial)) {
    if let Some(item) = item_atual {
        aplicar(item);
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn st_fcp_ipi_e_desconto_do_item_e_o_st_retido_fica_de_fora() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<nfeProc xmlns="http://www.portalfiscal.inf.br/nfe"><NFe>
<infNFe Id="NFe35240912345678000181550010000012341123456789" versao="4.00">
  <ide><nNF>9</nNF><serie>1</serie><dhEmi>2024-09-12T10:00:00-03:00</dhEmi></ide>
  <emit><CNPJ>11222333000181</CNPJ><xNome>Fornecedor</xNome></emit>
  <det nItem="1">
    <prod><cProd>A</cProd><xProd>CERVEJA</xProd><NCM>22030000</NCM><qCom>12</qCom>
      <vUnCom>5.00</vUnCom><vProd>60.00</vProd><vDesc>6.00</vDesc></prod>
    <imposto>
      <ICMS><ICMS10><vICMS>10.80</vICMS><vICMSST>9.50</vICMSST><vFCPST>1.20</vFCPST></ICMS10></ICMS>
      <IPI><IPITrib><vIPI>3.00</vIPI></IPITrib></IPI>
    </imposto>
  </det>
  <det nItem="2">
    <prod><cProd>B</cProd><xProd>AGUA</xProd><NCM>22011000</NCM><qCom>6</qCom>
      <vUnCom>2.00</vUnCom><vProd>12.00</vProd></prod>
    <imposto><ICMS><ICMS60><vICMSSTRet>1.00</vICMSSTRet></ICMS60></ICMS></imposto>
  </det>
  <total><ICMSTot><vProd>72.00</vProd><vST>10.70</vST><vIPI>3.00</vIPI><vDesc>6.00</vDesc>
    <vFrete>0.00</vFrete><vSeg>0.00</vSeg><vOutro>0.00</vOutro><vNF>79.70</vNF></ICMSTot></total>
</infNFe></NFe></nfeProc>"#;
        let n = interpretar(xml).unwrap();
        let a = &n.itens[0];
        assert_eq!(a.valor_desconto, Dinheiro::reais(6));
        assert_eq!(a.valor_icms_st, Dinheiro::centavos(1_070));
        assert_eq!(a.valor_ipi, Dinheiro::reais(3));
        assert_eq!(a.valor_na_nota(), Dinheiro::centavos(6_770));
        let b = &n.itens[1];
        assert_eq!(
            b.valor_icms_st,
            Dinheiro::ZERO,
            "ST retido antes não é desta nota"
        );
        // Itens somados batem com o vNF.
        let soma: Dinheiro = n.itens.iter().map(ItemNfe::valor_na_nota).sum();
        assert_eq!(soma, n.valor_total);
    }

    const XML_EXEMPLO: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<nfeProc xmlns="http://www.portalfiscal.inf.br/nfe">
  <NFe>
    <infNFe Id="NFe35240912345678000181550010000012341123456789" versao="4.00">
      <ide>
        <nNF>1234</nNF>
        <serie>1</serie>
        <dhEmi>2024-09-12T10:00:00-03:00</dhEmi>
      </ide>
      <emit>
        <CNPJ>11222333000181</CNPJ>
        <xNome>Distribuidora Alfa LTDA</xNome>
      </emit>
      <det nItem="1">
        <prod>
          <cProd>REF-8821</cProd>
          <cEAN>7894900011517</cEAN>
          <xProd>REFRIG COLA PET 2L</xProd>
          <NCM>22021000</NCM>
          <qCom>120.0000</qCom>
          <vUnCom>4.320000</vUnCom>
          <vProd>518.40</vProd>
        </prod>
      </det>
      <det nItem="2">
        <prod>
          <cProd>REF-8822</cProd>
          <cEAN>SEM GTIN</cEAN>
          <xProd>REFRIG GUARANA PET 2L</xProd>
          <NCM>22021000</NCM>
          <qCom>60.0000</qCom>
          <vUnCom>4.320000</vUnCom>
          <vProd>259.20</vProd>
        </prod>
      </det>
      <total>
        <ICMSTot>
          <vProd>777.60</vProd>
          <vFrete>180.00</vFrete>
          <vSeg>0.00</vSeg>
          <vOutro>0.00</vOutro>
          <vNF>957.60</vNF>
        </ICMSTot>
      </total>
    </infNFe>
  </NFe>
</nfeProc>"#;

    #[test]
    fn interpreta_os_campos_essenciais() {
        let nota = interpretar(XML_EXEMPLO).unwrap();
        assert_eq!(nota.numero, "1234");
        assert_eq!(nota.serie, "1");
        assert_eq!(nota.fornecedor_nome, "Distribuidora Alfa LTDA");
        assert_eq!(nota.itens.len(), 2);
        assert_eq!(nota.itens[0].codigo_fornecedor, "REF-8821");
        assert_eq!(nota.itens[0].valor_total, Dinheiro::centavos(51_840));
        assert_eq!(nota.valor_frete, Dinheiro::reais(180));
        assert_eq!(
            nota.valor_total,
            Dinheiro::reais(957) + Dinheiro::centavos(60)
        );
    }

    #[test]
    fn ce_an_vira_codigo_de_barras_e_sem_gtin_vira_none() {
        let nota = interpretar(XML_EXEMPLO).unwrap();
        assert_eq!(
            nota.itens[0].codigo_barras.as_deref(),
            Some("7894900011517")
        );
        // "SEM GTIN" é o literal da NF-e para "sem código de barras" — não é um GTIN de
        // verdade, então vira `None`, não a string literal.
        assert_eq!(nota.itens[1].codigo_barras, None);
    }

    fn com_ide(tp_nf: &str, fin: &str, dest: &str, cfop: &str) -> NotaFiscalXml {
        let xml = XML_EXEMPLO
            .replace(
                "<serie>1</serie>",
                &format!("<serie>1</serie><tpNF>{tp_nf}</tpNF><finNFe>{fin}</finNFe>"),
            )
            .replace(
                "</emit>",
                &format!(
                    "</emit><dest>{dest}<enderDest><CNPJ>00000000000000</CNPJ></enderDest></dest>"
                ),
            )
            .replace(
                "<NCM>22021000</NCM>",
                &format!("<NCM>22021000</NCM><CFOP>{cfop}</CFOP>"),
            );
        interpretar(&xml).unwrap()
    }

    #[test]
    fn le_as_duplicatas_da_cobranca() {
        let xml = XML_EXEMPLO.replace(
            "</total>",
            "</total><cobr><fat><nFat>123</nFat><vOrig>100.00</vOrig><vLiq>100.00</vLiq></fat>\
             <dup><nDup>001</nDup><dVenc>2024-10-12</dVenc><vDup>40.00</vDup></dup>\
             <dup><nDup>002</nDup><dVenc>2024-11-12</dVenc><vDup>60.00</vDup></dup></cobr>",
        );
        let n = interpretar(&xml).unwrap();
        assert_eq!(
            n.duplicatas,
            vec![
                Duplicata {
                    numero: "001".into(),
                    vencimento: Data::de_ymd(2024, 10, 12).unwrap(),
                    valor: Dinheiro::reais(40),
                },
                Duplicata {
                    numero: "002".into(),
                    vencimento: Data::de_ymd(2024, 11, 12).unwrap(),
                    valor: Dinheiro::reais(60),
                },
            ]
        );
        assert!(interpretar(XML_EXEMPLO).unwrap().duplicatas.is_empty());
        let sem_valor = xml.replace("<vDup>60.00</vDup>", "");
        assert!(interpretar(&sem_valor).is_err(), "duplicata sem valor");
    }

    #[test]
    fn le_tipo_finalidade_destinatario_e_cfop() {
        let n = com_ide("0", "4", "<CNPJ>45.543.915/0001-81</CNPJ>", "5915");
        assert!(n.entrada_do_emitente);
        assert_eq!(n.finalidade, FinalidadeNfe::Devolucao);
        assert_eq!(
            n.destinatario.as_deref(),
            Some("45543915000181"),
            "o CNPJ direto de dest"
        );
        assert_eq!(n.itens[0].cfop.as_deref(), Some("5915"));
        let sem = interpretar(XML_EXEMPLO).unwrap();
        assert_eq!(
            (sem.entrada_do_emitente, sem.finalidade, sem.destinatario),
            (false, FinalidadeNfe::Normal, None)
        );
    }

    #[test]
    fn classifica_o_que_e_compra_para_a_empresa() {
        use ClassificacaoEntrada::*;
        let nos = Cnpj::novo("45543915000181").unwrap();
        let dest_nos = "<CNPJ>45543915000181</CNPJ>";
        // A venda do fornecedor (tpNF 1) para nós é compra — o tpNF não decide sozinho.
        assert_eq!(
            com_ide("1", "1", dest_nos, "5102").classificar_para(&nos),
            Compra
        );
        // Emitida por nós.
        let emitente = Cnpj::novo("11222333000181").unwrap();
        assert_eq!(
            com_ide("1", "1", dest_nos, "5102").classificar_para(&emitente),
            SaidaPropria
        );
        // Para outra empresa, ou para uma pessoa.
        assert_eq!(
            com_ide("1", "1", "<CNPJ>12345678000195</CNPJ>", "5102").classificar_para(&nos),
            DeOutroDestinatario
        );
        assert_eq!(
            com_ide("1", "1", "<CPF>52998224725</CPF>", "5102").classificar_para(&nos),
            DeOutroDestinatario
        );
        assert_eq!(
            com_ide("1", "4", dest_nos, "5202").classificar_para(&nos),
            DevolucaoDeCliente
        );
        assert_eq!(
            com_ide("0", "1", dest_nos, "1102").classificar_para(&nos),
            EntradaDoEmitente
        );
        // Sem destinatário no XML não há como conferir: segue.
        assert_eq!(
            interpretar(XML_EXEMPLO).unwrap().classificar_para(&nos),
            Compra
        );
        assert!(SaidaPropria.motivo_da_recusa().is_some() && Compra.motivo_da_recusa().is_none());
    }

    #[test]
    fn cfop_de_venda_e_os_que_nao_sao() {
        for venda in [
            "5101", "5102", "6102", "5117", "5403", "5405", "6403", "5656",
        ] {
            assert!(cfop_de_venda(venda), "{venda}");
        }
        for outro in [
            "5915", "5910", "5917", "5922", "5949", "5152", "5551", "5933", "1102", "5408", "51",
        ] {
            assert!(!cfop_de_venda(outro), "{outro}");
        }
        let n = com_ide("1", "1", "<CNPJ>45543915000181</CNPJ>", "5915");
        assert_eq!(
            n.cfops_fora_de_venda(),
            vec!["5915".to_string()],
            "sem repetir"
        );
        assert_eq!(descrever_cfop("6915"), "remessa para conserto ou reparo");
    }

    #[test]
    fn xml_sem_campo_obrigatorio_e_recusado() {
        let erro = interpretar("<nfeProc><NFe><infNFe Id=\"NFe12345678901234567890123456789012345678901234\"></infNFe></NFe></nfeProc>")
            .unwrap_err();
        assert!(matches!(erro, ErroFiscal::XmlInvalido(_)));
    }

    #[test]
    fn quantidade_zero_ou_negativa_e_recusada() {
        // Endurecimento pra importação em lote (2026-09-15): um XML anômalo não deve entrar
        // silenciosamente só porque o resto do arquivo é válido.
        let xml = XML_EXEMPLO.replace("<qCom>120.0000</qCom>", "<qCom>0</qCom>");
        let erro = interpretar(&xml).unwrap_err();
        assert!(matches!(erro, ErroFiscal::XmlInvalido(_)));

        let xml = XML_EXEMPLO.replace("<qCom>120.0000</qCom>", "<qCom>-5</qCom>");
        let erro = interpretar(&xml).unwrap_err();
        assert!(matches!(erro, ErroFiscal::XmlInvalido(_)));
    }

    #[test]
    fn preco_unitario_negativo_e_recusado() {
        let xml = XML_EXEMPLO.replace("<vUnCom>4.320000</vUnCom>", "<vUnCom>-1</vUnCom>");
        let erro = interpretar(&xml).unwrap_err();
        assert!(matches!(erro, ErroFiscal::XmlInvalido(_)));
    }
}
