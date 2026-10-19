//! A NFC-e a emitir e o retorno da SEFAZ — os tipos que atravessam a [`crate::PortaFiscal`].
//!
//! Espelham o `NFCeRequest`/`SefazResponse` do `sefaz-api`, mas com os tipos do Cardeal
//! (dinheiro exato, CNPJ validado, UF). Quem monta a nota é `mod-fiscal`, a partir da venda;
//! quem converte para o JSON da API é o adaptador ([`crate::ApiFiscalHttp`]). A nota é
//! guardada (sem segredo nenhum) junto do documento fiscal, como foi emitida.

use cardeal_kernel::{Cnpj, Cpf, Dinheiro, Percentual, Preco, Quantidade, Uf};
use cardeal_storage::Segredo;
use serde::{Deserialize, Serialize};

/// O regime do emitente na NF-e (CRT).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Crt {
    /// 1 — Simples Nacional (inclui o MEI).
    SimplesNacional,
    /// 2 — Simples Nacional com receita acima do sublimite.
    SimplesExcessoSublimite,
    /// 3 — Regime normal (Lucro Presumido/Real).
    RegimeNormal,
}

impl Crt {
    /// O código do XML.
    #[must_use]
    pub const fn codigo(self) -> &'static str {
        match self {
            Self::SimplesNacional => "1",
            Self::SimplesExcessoSublimite => "2",
            Self::RegimeNormal => "3",
        }
    }

    /// Verdadeiro para o Simples (usa CSOSN no lugar do CST do ICMS).
    #[must_use]
    pub const fn e_simples(self) -> bool {
        !matches!(self, Self::RegimeNormal)
    }
}

/// O endereço do emitente.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnderecoEmitente {
    /// Logradouro.
    pub logradouro: String,
    /// Número.
    pub numero: String,
    /// Complemento.
    pub complemento: String,
    /// Bairro.
    pub bairro: String,
    /// Código IBGE do município (7 dígitos).
    pub codigo_municipio: String,
    /// Município.
    pub municipio: String,
    /// UF.
    pub uf: Uf,
    /// CEP (8 dígitos).
    pub cep: String,
    /// Telefone (só dígitos), se houver.
    pub telefone: Option<String>,
}

/// O emitente.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Emitente {
    /// CNPJ.
    pub cnpj: Cnpj,
    /// Razão social.
    pub razao_social: String,
    /// Nome fantasia.
    pub nome_fantasia: String,
    /// Inscrição estadual (ou `ISENTO`).
    pub inscricao_estadual: String,
    /// Inscrição municipal.
    pub inscricao_municipal: Option<String>,
    /// Regime.
    pub crt: Crt,
    /// Endereço.
    pub endereco: EnderecoEmitente,
}

/// O ICMS do item: CST (regime normal) ou CSOSN (Simples), com base, alíquota e valor quando
/// tributado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IcmsItem {
    /// Origem da mercadoria (0 a 8).
    pub origem: u8,
    /// CST (regime normal).
    pub cst: Option<String>,
    /// CSOSN (Simples).
    pub csosn: Option<String>,
    /// Base de cálculo.
    pub base: Option<Dinheiro>,
    /// Alíquota.
    pub aliquota: Option<Percentual>,
    /// Valor.
    pub valor: Option<Dinheiro>,
    /// Redução da base, quando houver (CST 20).
    pub reducao_base: Option<Percentual>,
}

/// PIS ou COFINS do item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContribuicaoItem {
    /// CST (2 dígitos).
    pub cst: String,
    /// Base de cálculo.
    pub base: Option<Dinheiro>,
    /// Alíquota.
    pub aliquota: Option<Percentual>,
    /// Valor.
    pub valor: Option<Dinheiro>,
}

/// Os impostos de um item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImpostosItem {
    /// ICMS.
    pub icms: IcmsItem,
    /// PIS.
    pub pis: ContribuicaoItem,
    /// COFINS.
    pub cofins: ContribuicaoItem,
}

/// Um item da nota.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemDocumento {
    /// Código do produto no emitente.
    pub codigo: String,
    /// GTIN/EAN (`None` sai como `SEM GTIN`).
    pub gtin: Option<String>,
    /// Descrição.
    pub descricao: String,
    /// NCM (8 dígitos).
    pub ncm: String,
    /// CEST (7 dígitos), quando houver.
    pub cest: Option<String>,
    /// CFOP (4 dígitos).
    pub cfop: String,
    /// Unidade comercial ("UN", "KG"...).
    pub unidade: String,
    /// Quantidade.
    pub quantidade: Quantidade,
    /// Valor unitário.
    pub valor_unitario: Preco,
    /// Valor bruto do item (`quantidade × valor_unitario`, arredondado) — o `vProd`.
    pub valor_bruto: Dinheiro,
    /// Desconto do item — o `vDesc`.
    pub desconto: Dinheiro,
    /// Impostos.
    pub impostos: ImpostosItem,
}

impl ItemDocumento {
    /// O valor líquido do item (bruto − desconto).
    #[must_use]
    pub fn valor_liquido(&self) -> Dinheiro {
        self.valor_bruto - self.desconto
    }
}

/// O meio de pagamento (`tPag`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MeioPagamento {
    /// 01 — dinheiro.
    Dinheiro,
    /// 03 — cartão de crédito.
    CartaoCredito,
    /// 04 — cartão de débito.
    CartaoDebito,
    /// 17 — Pix.
    Pix,
    /// 05 — crédito loja (a venda a prazo, o fiado, o crediário).
    CreditoLoja,
    /// 99 — outros.
    Outros,
}

impl MeioPagamento {
    /// O código `tPag`.
    #[must_use]
    pub const fn codigo(self) -> &'static str {
        match self {
            Self::Dinheiro => "01",
            Self::CartaoCredito => "03",
            Self::CartaoDebito => "04",
            Self::Pix => "17",
            Self::CreditoLoja => "05",
            Self::Outros => "99",
        }
    }

    /// Pagamento eletrônico (cartão e Pix) leva o grupo `card` na NFC-e — a NT 2025.001 o exige
    /// também no Pix (aqui sempre "não integrado": POS/QR à parte). É o que o `gestao-raiz`, em
    /// produção, manda (`03`, `04`, `17`).
    #[must_use]
    pub const fn exige_grupo_cartao(self) -> bool {
        matches!(self, Self::CartaoCredito | Self::CartaoDebito | Self::Pix)
    }
}

/// Um pagamento.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PagamentoDocumento {
    /// O meio.
    pub meio: MeioPagamento,
    /// O valor.
    pub valor: Dinheiro,
}

/// O documento do consumidor. Enum próprio (e não o `Documento` do kernel, que é serializado
/// com etiqueta adjacente) porque a nota é guardada em `postcard`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocumentoConsumidor {
    /// Pessoa física.
    Cpf(Cpf),
    /// Pessoa jurídica.
    Cnpj(Cnpj),
}

/// O consumidor identificado ("CPF na nota").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Consumidor {
    /// CPF ou CNPJ.
    pub documento: DocumentoConsumidor,
    /// Nome, se informado.
    pub nome: Option<String>,
}

/// Uma NFC-e (modelo 65) pronta para emitir — sem certificado nem CSC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Nfce {
    /// O emitente.
    pub emitente: Emitente,
    /// Os itens.
    pub itens: Vec<ItemDocumento>,
    /// Os pagamentos (somam o total da nota, ou mais — a diferença é o troco).
    pub pagamentos: Vec<PagamentoDocumento>,
    /// O troco.
    pub troco: Dinheiro,
    /// O consumidor, se identificado.
    pub consumidor: Option<Consumidor>,
    /// A série.
    pub serie: u32,
    /// O número.
    pub numero: u64,
    /// Informações complementares (vão impressas no DANFCE).
    pub informacoes: Option<String>,
}

impl Nfce {
    /// O total da nota (`vNF` = soma dos líquidos).
    #[must_use]
    pub fn total(&self) -> Dinheiro {
        self.itens.iter().map(ItemDocumento::valor_liquido).sum()
    }
}

/// O Código de Segurança do Contribuinte da NFC-e (cadastrado na SEFAZ da UF).
#[derive(Debug, Clone)]
pub struct Csc {
    /// O identificador (ex.: "000001").
    pub id: String,
    /// O token.
    pub token: Segredo<String>,
}

/// O pedido de inutilização de uma faixa de números que nunca vão ser usados.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PedidoInutilizacao {
    /// O CNPJ do emitente.
    pub cnpj: Cnpj,
    /// NF-e ou NFC-e.
    pub modelo: crate::porta::ModeloFiscal,
    /// A série.
    pub serie: u32,
    /// O primeiro número.
    pub inicio: u64,
    /// O último número.
    pub fim: u64,
    /// O ano (dois dígitos) em que os números seriam usados.
    pub ano: u16,
    /// A justificativa (15 a 255 caracteres).
    pub justificativa: String,
    /// A UF do emitente.
    pub uf: Uf,
}

/// Como a SEFAZ (ou o provedor) respondeu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SituacaoRetorno {
    /// Autorizado o uso (100) ou autorizado fora de prazo (150).
    Autorizado,
    /// Rejeitado — o número não foi usado; corrige e reenvia.
    Rejeitado,
    /// Denegado — irregularidade cadastral; o número foi consumido.
    Denegado,
    /// Ainda processando, ou o resultado não chegou: consultar pela chave antes de reenviar.
    Processando,
    /// Cancelado (na consulta de uma nota cancelada).
    Cancelado,
    /// Erro do provedor (falha ao montar/assinar/enviar); se veio chave, consultar antes.
    Erro,
}

/// O retorno de uma emissão ou consulta.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetornoSefaz {
    /// A situação.
    pub situacao: SituacaoRetorno,
    /// O código (`cStat`), ou o do provedor ("ERR", "TIMEOUT", "400").
    pub codigo: String,
    /// O motivo (`xMotivo`).
    pub motivo: String,
    /// A chave de acesso — vem mesmo em erro, quando o provedor chegou a gerá-la.
    pub chave: Option<String>,
    /// O protocolo de autorização.
    pub protocolo: Option<String>,
    /// Quando a SEFAZ recebeu.
    pub recebido_em: Option<String>,
    /// O XML autorizado (`nfeProc`), quando autorizado.
    pub xml: Option<String>,
    /// Os erros de validação do provedor.
    pub erros: Vec<String>,
}

/// Uma NFC-e de exemplo para os testes do crate: dois itens (um com desconto), Simples
/// Nacional, paga em Pix e cartão de débito, CPF na nota.
#[cfg(test)]
pub(crate) fn exemplo() -> Nfce {
    let impostos = || ImpostosItem {
        icms: IcmsItem {
            origem: 0,
            cst: None,
            csosn: Some("102".into()),
            base: None,
            aliquota: None,
            valor: None,
            reducao_base: None,
        },
        pis: ContribuicaoItem {
            cst: "49".into(),
            base: Some(Dinheiro::ZERO),
            aliquota: Some(Percentual::ZERO),
            valor: Some(Dinheiro::ZERO),
        },
        cofins: ContribuicaoItem {
            cst: "49".into(),
            base: Some(Dinheiro::ZERO),
            aliquota: Some(Percentual::ZERO),
            valor: Some(Dinheiro::ZERO),
        },
    };
    Nfce {
        emitente: Emitente {
            cnpj: Cnpj::novo("11222333000181").unwrap(),
            razao_social: "EMPRESA TESTE LTDA".into(),
            nome_fantasia: "Teste".into(),
            inscricao_estadual: "0960000000".into(),
            inscricao_municipal: None,
            crt: Crt::SimplesNacional,
            endereco: EnderecoEmitente {
                logradouro: "Rua A".into(),
                numero: "1".into(),
                complemento: String::new(),
                bairro: "Centro".into(),
                codigo_municipio: "4314902".into(),
                municipio: "Porto Alegre".into(),
                uf: Uf::Rs,
                cep: "90010000".into(),
                telefone: None,
            },
        },
        itens: vec![
            ItemDocumento {
                codigo: "P1".into(),
                gtin: Some("7891000315507".into()),
                descricao: "Cabo USB-C".into(),
                ncm: "85444200".into(),
                cest: None,
                cfop: "5102".into(),
                unidade: "UN".into(),
                quantidade: Quantidade::unidades(2),
                valor_unitario: Preco::centavos(2_990),
                valor_bruto: Dinheiro::centavos(5_980),
                desconto: Dinheiro::centavos(580),
                impostos: impostos(),
            },
            ItemDocumento {
                codigo: "P2".into(),
                gtin: None,
                descricao: "Película".into(),
                ncm: "39199090".into(),
                cest: Some("1001200".into()),
                cfop: "5102".into(),
                unidade: "UN".into(),
                quantidade: Quantidade::unidades(1),
                valor_unitario: Preco::centavos(1_500),
                valor_bruto: Dinheiro::centavos(1_500),
                desconto: Dinheiro::ZERO,
                impostos: impostos(),
            },
        ],
        pagamentos: vec![
            PagamentoDocumento {
                meio: MeioPagamento::Pix,
                valor: Dinheiro::centavos(4_000),
            },
            PagamentoDocumento {
                meio: MeioPagamento::CartaoDebito,
                valor: Dinheiro::centavos(2_900),
            },
        ],
        troco: Dinheiro::ZERO,
        consumidor: Some(Consumidor {
            documento: DocumentoConsumidor::Cpf(Cpf::novo("52998224725").unwrap()),
            nome: None,
        }),
        serie: 1,
        numero: 42,
        informacoes: None,
    }
}

#[cfg(test)]
mod testes {
    #[test]
    fn a_nota_guardada_volta_igual_em_postcard() {
        // A nota vai para a base em postcard (que não aceita enum com etiqueta adjacente,
        // como o `Documento` do kernel) — por isso o consumidor tem documento próprio.
        let nota = super::exemplo();
        let bytes = postcard::to_stdvec(&nota).unwrap();
        assert_eq!(postcard::from_bytes::<super::Nfce>(&bytes).unwrap(), nota);
    }
}
