//! A **NF-e de devolução de compra** (finalidade 4): a mercadoria que volta ao fornecedor, com a
//! nota de compra referenciada (`refNFe`) e, na proporção do que volta, os valores e os tributos
//! que a compra destacou — o ICMS próprio, o ST e o FCP do ST, o IPI (`impostoDevol`), o frete, o
//! seguro e as outras despesas —, para o fornecedor recuperar o crédito.
//!
//! Quem junta os dados é o `mod-compras` (a nota, o XML, a devolução): o fiscal recebe pronto
//! ([`DadosDaDevolucao`]), monta ([`montar_nfe_de_devolucao`], função pura) e põe na fila
//! ([`emitir_nfe_de_devolucao_comum`]). Por isso a nota rejeitada se reenvia pela devolução, em
//! Compras — o fiscal sozinho não remonta.
//!
//! Só no Simples Nacional, com o CSOSN 900: no regime normal a devolução estorna o crédito de
//! ICMS, PIS e COFINS da compra, que ainda não está no cálculo.

use std::collections::HashMap;
use std::fmt::Write as _;

use cardeal_fiscal::{
    ContribuicaoItem, Destinatario, DevolucaoDoItem, Emitente, FinalidadeNfe, IcmsItem,
    ImpostosItem, IndicadorIe, ItemDocumento, Nfe,
};
use cardeal_kernel::{Arredondamento, Data, Dinheiro, Id, Percentual, Preco, Quantidade};
use mod_estoque::ProdutoParaNota;

use crate::montagem::TEXTO_DO_SIMPLES;

/// A origem do documento fiscal da devolução (o id é o da devolução).
pub const ORIGEM_DEVOLUCAO_COMPRA: &str = "compras_devolucao";

/// Um item que volta ao fornecedor, com os valores e os tributos da compra já na proporção.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemDevolvidoNaNota {
    /// O produto do estoque.
    pub produto: Id,
    /// O NCM da nota de compra (vazio: o do cadastro).
    pub ncm: String,
    /// Quanto volta.
    pub quantidade: Quantidade,
    /// O valor unitário da compra (`vUnCom`).
    pub valor_unitario: Preco,
    /// O valor bruto do que volta.
    pub valor_bruto: Dinheiro,
    /// O desconto da compra, na proporção.
    pub desconto: Dinheiro,
    /// A origem da mercadoria (`orig` da compra).
    pub origem: u8,
    /// A base e a alíquota do ICMS próprio destacado na compra (a base na proporção).
    pub icms: Option<(Dinheiro, Percentual)>,
    /// A compra veio com ST (cobrado nela ou retido antes): o CFOP é o da devolução com ST.
    pub com_st: bool,
    /// O ST, o FCP do ST, o IPI, o frete, o seguro e as outras despesas que voltam.
    pub devolvido: DevolucaoDoItem,
}

/// A devolução que vira NF-e.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DadosDaDevolucao {
    /// A chave da nota de compra (sem ela não há NF-e de devolução).
    pub chave_da_compra: Option<String>,
    /// O número da nota de compra.
    pub numero_da_compra: String,
    /// A série.
    pub serie_da_compra: String,
    /// A emissão.
    pub emissao_da_compra: Data,
    /// Por que volta.
    pub motivo: String,
    /// Os itens.
    pub itens: Vec<ItemDevolvidoNaNota>,
}

/// O CFOP da devolução de compra para comercialização: 5202/6202, ou 5411/6411 com ST.
#[must_use]
pub const fn cfop_da_devolucao(com_st: bool, interestadual: bool) -> &'static str {
    match (com_st, interestadual) {
        (false, false) => "5202",
        (false, true) => "6202",
        (true, false) => "5411",
        (true, true) => "6411",
    }
}

/// O código do produto na nota (o mesmo da venda).
fn codigo_do_produto(id: Id) -> String {
    let hex: String = id
        .to_string()
        .chars()
        .filter(char::is_ascii_hexdigit)
        .collect();
    hex[hex.len().saturating_sub(12)..].to_ascii_uppercase()
}

fn item_da_nota(
    it: &ItemDevolvidoNaNota,
    p: &ProdutoParaNota,
    interestadual: bool,
) -> Result<ItemDocumento, String> {
    let ncm: String = if it.ncm.trim().is_empty() {
        &p.ncm
    } else {
        &it.ncm
    }
    .chars()
    .filter(char::is_ascii_digit)
    .collect();
    if ncm.len() != 8 {
        return Err(format!(
            "{}: sem NCM (nem na nota de compra nem no cadastro)",
            p.nome
        ));
    }
    let zerado = || ContribuicaoItem {
        cst: "49".into(),
        base: Some(Dinheiro::ZERO),
        aliquota: Some(Percentual::ZERO),
        valor: Some(Dinheiro::ZERO),
    };
    // O ICMS próprio que a compra destacou volta com a mesma alíquota: o valor sai da base,
    // como a SEFAZ confere.
    let icms = IcmsItem {
        origem: it.origem,
        cst: None,
        csosn: Some("900".into()),
        base: it.icms.map(|(b, _)| b),
        aliquota: it.icms.map(|(_, a)| a),
        valor: it
            .icms
            .map(|(b, a)| b.aplicar(a, Arredondamento::MeioAcima)),
        reducao_base: None,
    };
    Ok(ItemDocumento {
        codigo: codigo_do_produto(p.id),
        gtin: p.gtin.clone(),
        descricao: p.nome.trim().chars().take(120).collect(),
        ncm,
        cest: p
            .cest
            .as_ref()
            .map(|c| c.chars().filter(char::is_ascii_digit).collect()),
        cfop: cfop_da_devolucao(it.com_st, interestadual).into(),
        unidade: p.unidade.trim().chars().take(6).collect(),
        quantidade: it.quantidade,
        valor_unitario: it.valor_unitario,
        valor_bruto: it.valor_bruto,
        desconto: it.desconto,
        impostos: ImpostosItem {
            icms,
            pis: zerado(),
            cofins: zerado(),
        },
    })
}

/// Monta a NF-e de devolução — ou o que impede, em frases.
///
/// # Errors
/// O que impede: regime normal, nota de compra sem chave, produto sem NCM, devolução vazia.
#[allow(clippy::implicit_hasher)]
pub fn montar_nfe_de_devolucao(
    emitente: Emitente,
    destinatario: Destinatario,
    d: &DadosDaDevolucao,
    produtos: &HashMap<Id, ProdutoParaNota>,
    serie: u32,
    numero: u64,
) -> Result<Nfe, Vec<String>> {
    let mut problemas = Vec::new();
    if !emitente.crt.e_simples() {
        problemas.push(
            "a devolução de compra no regime normal ainda não sai em NF-e — o estorno do crédito \
             de ICMS, PIS e COFINS da compra não está no cálculo"
                .to_string(),
        );
    }
    let chave = d
        .chave_da_compra
        .as_deref()
        .map(|c| c.chars().filter(char::is_ascii_digit).collect::<String>())
        .filter(|c| c.len() == 44);
    if chave.is_none() {
        problemas.push(format!(
            "a nota de compra {}/{} não tem chave de acesso (foi lançada à mão): a NF-e de \
             devolução precisa referenciá-la",
            d.numero_da_compra, d.serie_da_compra
        ));
    }
    if d.itens.is_empty() {
        problemas.push("a devolução não tem itens".into());
    }
    let interestadual = destinatario.endereco.uf != emitente.endereco.uf;
    let mut itens = Vec::with_capacity(d.itens.len());
    for (i, it) in d.itens.iter().enumerate() {
        let Some(p) = produtos.get(&it.produto) else {
            problemas.push(format!(
                "Item {}: o produto não existe mais no cadastro",
                i + 1
            ));
            continue;
        };
        match item_da_nota(it, p, interestadual) {
            Ok(item) => itens.push(item),
            Err(e) => problemas.push(format!("Item {}: {e}", i + 1)),
        }
    }
    if !problemas.is_empty() {
        return Err(problemas);
    }
    let mut informacoes = format!(
        "Devolucao referente a NF-e {} serie {} de {}, chave {}. Motivo: {}.",
        d.numero_da_compra,
        d.serie_da_compra,
        d.emissao_da_compra.formatar(),
        chave.clone().unwrap_or_default(),
        d.motivo.trim()
    );
    let ipi: Dinheiro = d.itens.iter().map(|i| i.devolvido.ipi_devolvido).sum();
    if ipi.e_positivo() {
        let _ = write!(
            informacoes,
            " IPI devolvido: {}.",
            ipi.formatar_com_simbolo()
        );
    }
    informacoes.push(' ');
    informacoes.push_str(TEXTO_DO_SIMPLES);
    let consumidor_final = destinatario.indicador_ie != IndicadorIe::Contribuinte;
    Ok(Nfe {
        emitente,
        destinatario,
        itens,
        natureza: "Devolucao de compra para comercializacao".into(),
        saida: true,
        finalidade: FinalidadeNfe::Devolucao,
        referencias: chave.into_iter().collect(),
        consumidor_final,
        presencial: false,
        pagamentos: Vec::new(),
        a_prazo: false,
        duplicatas: Vec::new(),
        serie,
        numero,
        informacoes: Some(informacoes),
        devolvidos: d.itens.iter().map(|i| i.devolvido.clone()).collect(),
    })
}

/// Cria (ou, rejeitada, remonta) a NF-e de devolução da devolução `devolucao` do fornecedor
/// `fornecedor` e a põe na fila. Idempotente: a nota que já existe volta como está.
///
/// # Errors
/// Os do SQLite; a série da NF-e sem número livre.
#[cfg(feature = "sqlite")]
pub fn emitir_nfe_de_devolucao_comum(
    devolucao: Id,
    fornecedor: Id,
    dados: &DadosDaDevolucao,
    ctx: &cardeal_modkit::Ctx,
    uow: &mut cardeal_storage::UnidadeDeTrabalho,
) -> cardeal_kernel::Resultado<crate::DocumentoResumo> {
    sqlite::emitir(devolucao, fornecedor, dados, ctx, uow)
}

/// A NF-e de uma devolução de compra, se já foi criada.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct DocumentoDaDevolucao {
    /// A devolução.
    pub devolucao: Id,
}

impl cardeal_modkit::Consulta for DocumentoDaDevolucao {
    type Saida = Option<crate::DocumentoResumo>;
    const PERMISSAO: &'static str = "fiscal.ver";

    #[cfg(feature = "sqlite")]
    fn executar(
        self,
        ctx: &cardeal_modkit::Ctx,
        c: &rusqlite::Connection,
    ) -> cardeal_kernel::Resultado<Option<crate::DocumentoResumo>> {
        crate::sql_documento::documento_da_origem(
            c,
            ctx.empresa,
            ORIGEM_DEVOLUCAO_COMPRA,
            self.devolucao,
        )
    }
}

#[cfg(feature = "sqlite")]
mod sqlite {
    use cardeal_kernel::{CodigoErro, Erro, Id, Resultado};
    use cardeal_modkit::Ctx;
    use cardeal_storage::UnidadeDeTrabalho;

    use super::{montar_nfe_de_devolucao, DadosDaDevolucao, ORIGEM_DEVOLUCAO_COMPRA};
    use crate::documento::{DocumentoResumo, EstadoDocumento};
    use crate::sql_documento as sqld;
    use crate::ModeloFiscal;

    const MODELO: ModeloFiscal = ModeloFiscal::Nfe;

    /// A nota montada com o emitente, o fornecedor e os produtos — ou o que impede.
    fn montar(
        c: &rusqlite::Connection,
        ctx: &Ctx,
        fornecedor: Id,
        dados: &DadosDaDevolucao,
        serie: u32,
        numero: u64,
    ) -> Resultado<Result<cardeal_fiscal::Nfe, Vec<String>>> {
        let mut problemas = Vec::new();
        if crate::sql::certificado(c, ctx.empresa)?.is_none() {
            problemas.push(
                "o certificado digital A1 não está instalado (Configurações › Fiscal)".into(),
            );
        }
        let emitente = crate::nfce::sqlite::emitente(c, ctx.empresa)?;
        let destinatario = crate::nfe_venda::sqlite::destinatario(c, fornecedor)?;
        let ids: Vec<Id> = dados.itens.iter().map(|i| i.produto).collect();
        let produtos = mod_estoque::produtos_para_nota(c, ctx.organizacao, &ids)?;
        let nota = match (emitente, destinatario) {
            (Ok(e), Ok(d)) => montar_nfe_de_devolucao(e, d, dados, &produtos, serie, numero),
            (Err(mut a), Err(mut b)) => {
                a.append(&mut b);
                Err(a)
            }
            (Err(a), _) | (_, Err(a)) => Err(a),
        };
        Ok(match nota {
            Ok(n) if problemas.is_empty() => Ok(n),
            Ok(_) => Err(problemas),
            Err(mut p) => {
                problemas.append(&mut p);
                Err(problemas)
            }
        })
    }

    pub(super) fn emitir(
        devolucao: Id,
        fornecedor: Id,
        dados: &DadosDaDevolucao,
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
    ) -> Resultado<DocumentoResumo> {
        let existente = sqld::documento_da_origem(
            uow.conexao(),
            ctx.empresa,
            ORIGEM_DEVOLUCAO_COMPRA,
            devolucao,
        )?;
        if let Some(d) = &existente {
            let remontavel =
                d.estado == EstadoDocumento::Rejeitada && !d.anulada && d.pedido.is_none();
            if !remontavel {
                return Ok(d.clone());
            }
        }
        let (serie, numero) = if let Some(d) = &existente {
            (d.serie, d.numero)
        } else {
            let c = uow.conexao();
            let serie = sqld::serie_nfe(c, ctx.empresa)?;
            let ambiente = crate::sql::ambiente(c, ctx.empresa)?;
            (
                serie,
                sqld::reservar_numero(c, ctx.empresa, MODELO, serie, ambiente)?,
            )
        };
        let nota = montar(uow.conexao(), ctx, fornecedor, dados, serie, numero)?;
        let total = match &nota {
            Ok(n) => n.total(),
            Err(_) => dados
                .itens
                .iter()
                .map(|i| i.valor_bruto - i.desconto + i.devolvido.soma())
                .sum(),
        };
        let (estado, blob, motivo) = match nota {
            Ok(n) => (
                EstadoDocumento::Pendente,
                Some(
                    postcard::to_stdvec(&n)
                        .map_err(|e| Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string()))?,
                ),
                None,
            ),
            Err(p) => (EstadoDocumento::Rejeitada, None, Some(p.join("; "))),
        };
        let c = uow.conexao();
        match existente {
            Some(d) => sqld::remontar(
                c,
                ctx.empresa,
                d.id,
                blob.as_deref(),
                total,
                motivo.as_deref(),
                ctx.agora,
            )?,
            None => sqld::inserir_documento(
                c,
                &sqld::NovoDocumento {
                    id: Id::novo(),
                    empresa: ctx.empresa,
                    modelo: MODELO,
                    serie,
                    numero,
                    ambiente: crate::sql::ambiente(c, ctx.empresa)?,
                    origem_modulo: ORIGEM_DEVOLUCAO_COMPRA,
                    origem: devolucao,
                    estado,
                    total,
                    nota: blob,
                    motivo,
                    agora: ctx.agora,
                    substitui: None,
                },
            )?,
        }
        sqld::documento_da_origem(c, ctx.empresa, ORIGEM_DEVOLUCAO_COMPRA, devolucao)?
            .ok_or_else(|| Erro::nao_encontrado("documento fiscal"))
    }
}

#[cfg(test)]
mod testes {
    use cardeal_fiscal::{Crt, DocumentoConsumidor, EnderecoEmitente, IndicadorIe, MeioPagamento};
    use cardeal_kernel::{Cnpj, Uf};

    use super::*;

    fn endereco(uf: Uf) -> EnderecoEmitente {
        EnderecoEmitente {
            logradouro: "Rua A".into(),
            numero: "1".into(),
            complemento: String::new(),
            bairro: "Centro".into(),
            codigo_municipio: "4314902".into(),
            municipio: "Porto Alegre".into(),
            uf,
            cep: "90010000".into(),
            telefone: None,
        }
    }

    fn emitente(crt: Crt) -> Emitente {
        Emitente {
            cnpj: Cnpj::novo("11222333000181").unwrap(),
            razao_social: "LOJA".into(),
            nome_fantasia: "L".into(),
            inscricao_estadual: "0960000000".into(),
            inscricao_municipal: None,
            crt,
            endereco: endereco(Uf::Rs),
        }
    }

    fn fornecedor(uf: Uf) -> Destinatario {
        Destinatario {
            documento: DocumentoConsumidor::Cnpj(Cnpj::novo("45543915000181").unwrap()),
            nome: "ATACADO BETA LTDA".into(),
            indicador_ie: IndicadorIe::Contribuinte,
            inscricao_estadual: Some("111222333".into()),
            endereco: endereco(uf),
            email: None,
        }
    }

    fn produto(id: Id) -> ProdutoParaNota {
        ProdutoParaNota {
            id,
            nome: "Fermento biológico 10g".into(),
            ncm: String::new(),
            cest: None,
            gtin: None,
            unidade: "UN".into(),
            fiscal: mod_estoque::fiscal::CamposFiscais::default(),
        }
    }

    fn dados(produto: Id) -> DadosDaDevolucao {
        DadosDaDevolucao {
            chave_da_compra: Some("4".repeat(44)),
            numero_da_compra: "77".into(),
            serie_da_compra: "1".into(),
            emissao_da_compra: Data::de_ymd(2026, 10, 20).unwrap(),
            motivo: "Produto com validade vencida".into(),
            itens: vec![ItemDevolvidoNaNota {
                produto,
                ncm: "21021090".into(),
                quantidade: Quantidade::unidades(12),
                valor_unitario: Preco::de_str("1.50").unwrap(),
                valor_bruto: Dinheiro::reais(18),
                desconto: Dinheiro::centavos(90),
                origem: 0,
                icms: Some((Dinheiro::reais(18), Percentual::de_str("12").unwrap())),
                com_st: true,
                devolvido: DevolucaoDoItem {
                    modalidade_st: Some("4".into()),
                    base_st: Dinheiro::centavos(2_520),
                    aliquota_st: Some(Percentual::de_str("17").unwrap()),
                    valor_st: Dinheiro::centavos(212),
                    ipi_devolvido: Dinheiro::centavos(150),
                    percentual_devolvido: Percentual::de_str("50").unwrap(),
                    frete: Dinheiro::centavos(100),
                    ..DevolucaoDoItem::default()
                },
            }],
        }
    }

    #[test]
    fn a_devolucao_referencia_a_compra_e_devolve_os_tributos() {
        let id = Id::novo();
        let produtos = HashMap::from([(id, produto(id))]);
        let n = montar_nfe_de_devolucao(
            emitente(Crt::SimplesNacional),
            fornecedor(Uf::Rs),
            &dados(id),
            &produtos,
            1,
            90,
        )
        .unwrap();
        assert_eq!(n.finalidade, FinalidadeNfe::Devolucao);
        assert_eq!(n.referencias, vec!["4".repeat(44)]);
        assert!(!n.consumidor_final && n.saida && n.pagamentos.is_empty());
        let item = &n.itens[0];
        assert_eq!(item.cfop, "5411");
        assert_eq!(item.ncm, "21021090", "o NCM da compra");
        assert_eq!(item.impostos.icms.csosn.as_deref(), Some("900"));
        assert_eq!(item.impostos.icms.valor, Some(Dinheiro::centavos(216)));
        // 18,00 − 0,90 + ST 2,12 + IPI 1,50 + frete 1,00
        assert_eq!(n.total(), Dinheiro::centavos(2_172));
        let info = n.informacoes.unwrap();
        assert!(info.starts_with("Devolucao referente a NF-e 77 serie 1 de 20/10/2026"));
        assert!(info.contains("IPI devolvido: R$ 1,50"));
        assert!(info.ends_with(TEXTO_DO_SIMPLES));
        // Para outra UF, o CFOP de fora; sem ST, o 5202/6202.
        assert_eq!(cfop_da_devolucao(true, true), "6411");
        assert_eq!(cfop_da_devolucao(false, false), "5202");
        let _ = MeioPagamento::Outros;
    }

    #[test]
    fn regime_normal_e_compra_sem_chave_nao_saem() {
        let id = Id::novo();
        let produtos = HashMap::from([(id, produto(id))]);
        let mut d = dados(id);
        d.chave_da_compra = None;
        let e = montar_nfe_de_devolucao(
            emitente(Crt::RegimeNormal),
            fornecedor(Uf::Sc),
            &d,
            &produtos,
            1,
            90,
        )
        .unwrap_err();
        assert_eq!(e.len(), 2, "{e:?}");
        assert!(e[0].contains("regime normal"));
        assert!(e[1].contains("77/1 não tem chave de acesso"));
    }
}
