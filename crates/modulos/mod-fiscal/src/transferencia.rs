//! A **NF-e de transferência** entre estabelecimentos do mesmo titular (CFOP 5152/6152): a
//! mercadoria que vai de um CNPJ do grupo para outro da **mesma raiz** (matriz e filiais), ao
//! custo, sem pagamento.
//!
//! Quem junta os dados é o `mod-compras` (a transferência, os itens ao custo médio): o fiscal
//! recebe pronto ([`DadosDaTransferencia`]), monta ([`montar_nfe_de_transferencia`], função pura)
//! e põe na fila ([`emitir_nfe_de_transferencia_comum`]). A rejeitada se reenvia pela
//! transferência, como a devolução de compra.
//!
//! Só no **Simples Nacional**, com o CSOSN 400: desde o STF (ADC 49) e a LC 204/2023 não há ICMS
//! na transferência entre estabelecimentos do mesmo titular; no regime normal a LC 204 manda (ou
//! deixa) transferir o crédito de ICMS destacando-o na nota, e essa regra ainda não está no
//! cálculo. CNPJs de **raízes diferentes** são empresas diferentes: a mercadoria vai por venda,
//! não por transferência.

use std::collections::HashMap;

use cardeal_fiscal::{
    ContribuicaoItem, Destinatario, DocumentoConsumidor, Emitente, FinalidadeNfe, IcmsItem,
    ImpostosItem, IndicadorIe, ItemDocumento, Nfe,
};
use cardeal_kernel::{Dinheiro, Id, Percentual, Preco, Quantidade};
use mod_estoque::ProdutoParaNota;

use crate::montagem::TEXTO_DO_SIMPLES;

/// A origem do documento fiscal da transferência (o id é o da transferência).
pub const ORIGEM_TRANSFERENCIA: &str = "compras_transferencia";

/// Um item que vai para a outra filial, ao custo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemTransferidoNaNota {
    /// O produto do estoque (o catálogo é do grupo: o mesmo nas duas filiais).
    pub produto: Id,
    /// Quanto vai.
    pub quantidade: Quantidade,
    /// O custo unitário (o custo médio na origem, na hora da saída).
    pub custo_unitario: Preco,
    /// O valor do item (quantidade × custo, arredondado como o estoque arredondou).
    pub valor: Dinheiro,
}

/// A transferência que vira NF-e.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DadosDaTransferencia {
    /// A filial que recebe (uma empresa da mesma base).
    pub destino: Id,
    /// Uma observação (vai às informações complementares).
    pub observacao: Option<String>,
    /// Os itens.
    pub itens: Vec<ItemTransferidoNaNota>,
}

/// O CFOP da transferência de mercadoria adquirida de terceiros: 5152, ou 6152 para outra UF.
#[must_use]
pub const fn cfop_da_transferencia(interestadual: bool) -> &'static str {
    if interestadual {
        "6152"
    } else {
        "5152"
    }
}

/// Os dois CNPJs são do mesmo titular (a mesma raiz, os 8 primeiros dígitos)?
#[must_use]
pub fn mesma_raiz(a: &str, b: &str) -> bool {
    let raiz = |c: &str| -> String { c.chars().filter(char::is_ascii_digit).take(8).collect() };
    let (ra, rb) = (raiz(a), raiz(b));
    ra.len() == 8 && ra == rb
}

/// O código do produto na nota (o mesmo da venda e da devolução).
fn codigo_do_produto(id: Id) -> String {
    let hex: String = id
        .to_string()
        .chars()
        .filter(char::is_ascii_hexdigit)
        .collect();
    hex[hex.len().saturating_sub(12)..].to_ascii_uppercase()
}

/// A outra filial como destinatária da nota (pelo emitente que ela seria).
#[must_use]
pub fn destinatario_da_filial(filial: &Emitente) -> Destinatario {
    let ie: String = filial
        .inscricao_estadual
        .chars()
        .filter(char::is_ascii_digit)
        .collect();
    let isenta = filial
        .inscricao_estadual
        .trim()
        .eq_ignore_ascii_case("ISENTO");
    let (indicador_ie, inscricao_estadual) = if !ie.is_empty() {
        (IndicadorIe::Contribuinte, Some(ie))
    } else if isenta {
        (IndicadorIe::Isento, None)
    } else {
        (IndicadorIe::NaoContribuinte, None)
    };
    Destinatario {
        documento: DocumentoConsumidor::Cnpj(filial.cnpj),
        nome: filial.razao_social.clone(),
        indicador_ie,
        inscricao_estadual,
        endereco: filial.endereco.clone(),
        email: None,
    }
}

fn item_da_nota(
    it: &ItemTransferidoNaNota,
    p: &ProdutoParaNota,
    interestadual: bool,
) -> Result<ItemDocumento, String> {
    let ncm: String = p.ncm.chars().filter(char::is_ascii_digit).collect();
    if ncm.len() != 8 {
        return Err(format!("{}: sem NCM no cadastro", p.nome));
    }
    if !it.quantidade.e_positiva() || !it.valor.e_positivo() {
        return Err(format!("{}: sem quantidade ou sem custo", p.nome));
    }
    let zerado = || ContribuicaoItem {
        cst: "49".into(),
        base: Some(Dinheiro::ZERO),
        aliquota: Some(Percentual::ZERO),
        valor: Some(Dinheiro::ZERO),
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
        cfop: cfop_da_transferencia(interestadual).into(),
        unidade: p.unidade.trim().chars().take(6).collect(),
        quantidade: it.quantidade,
        valor_unitario: it.custo_unitario,
        valor_bruto: it.valor,
        desconto: Dinheiro::ZERO,
        impostos: ImpostosItem {
            icms: IcmsItem {
                origem: p.fiscal.origem.unwrap_or(0),
                cst: None,
                csosn: Some("400".into()),
                base: None,
                aliquota: None,
                valor: None,
                reducao_base: None,
            },
            pis: zerado(),
            cofins: zerado(),
        },
    })
}

/// O que impede a transferência entre a `origem` e o `destino`, antes de mexer no estoque: o
/// regime e a raiz dos CNPJs. Vazio = pode.
#[must_use]
pub fn impedimentos_da_transferencia(origem: &Emitente, destino: &Emitente) -> Vec<String> {
    let mut v = Vec::new();
    if !origem.crt.e_simples() {
        v.push(
            "a transferência no regime normal ainda não sai em NF-e — a transferência do crédito \
             de ICMS (LC 204/2023) não está no cálculo"
                .to_owned(),
        );
    }
    if !mesma_raiz(&origem.cnpj.sem_mascara(), &destino.cnpj.sem_mascara()) {
        v.push(format!(
            "{} e {} são empresas diferentes (a raiz do CNPJ não é a mesma): a mercadoria vai \
             por venda, não por transferência",
            origem.cnpj.formatar(),
            destino.cnpj.formatar()
        ));
    }
    v
}

/// Monta a NF-e de transferência — ou o que impede, em frases.
///
/// # Errors
/// O que impede: regime normal, raízes diferentes, produto sem NCM, transferência vazia.
#[allow(clippy::implicit_hasher)]
pub fn montar_nfe_de_transferencia(
    emitente: Emitente,
    filial: &Emitente,
    d: &DadosDaTransferencia,
    produtos: &HashMap<Id, ProdutoParaNota>,
    serie: u32,
    numero: u64,
) -> Result<Nfe, Vec<String>> {
    let mut problemas = impedimentos_da_transferencia(&emitente, filial);
    if d.itens.is_empty() {
        problemas.push("a transferência não tem itens".into());
    }
    let interestadual = filial.endereco.uf != emitente.endereco.uf;
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
        "Transferencia entre estabelecimentos do mesmo titular (CNPJ base {}), ao custo. ICMS \
         nao incidente (STF ADC 49; LC 87/1996 art. 12 I, red. LC 204/2023).",
        emitente.cnpj.raiz()
    );
    if let Some(obs) = d
        .observacao
        .as_deref()
        .map(str::trim)
        .filter(|o| !o.is_empty())
    {
        informacoes.push(' ');
        informacoes.push_str(obs);
        if !obs.ends_with('.') {
            informacoes.push('.');
        }
    }
    informacoes.push(' ');
    informacoes.push_str(TEXTO_DO_SIMPLES);
    let destinatario = destinatario_da_filial(filial);
    let consumidor_final = destinatario.indicador_ie != IndicadorIe::Contribuinte;
    Ok(Nfe {
        emitente,
        destinatario,
        itens,
        natureza: "Transferencia de mercadoria adquirida de terceiros".into(),
        saida: true,
        finalidade: FinalidadeNfe::Normal,
        referencias: Vec::new(),
        consumidor_final,
        presencial: false,
        pagamentos: Vec::new(),
        a_prazo: false,
        duplicatas: Vec::new(),
        serie,
        numero,
        informacoes: Some(informacoes),
        devolvidos: Vec::new(),
    })
}

/// Cria (ou, rejeitada, remonta) a NF-e da transferência `transferencia` e a põe na fila.
/// Idempotente: a nota que já existe volta como está.
///
/// # Errors
/// Os do SQLite; a série da NF-e sem número livre.
#[cfg(feature = "sqlite")]
pub fn emitir_nfe_de_transferencia_comum(
    transferencia: Id,
    dados: &DadosDaTransferencia,
    ctx: &cardeal_modkit::Ctx,
    uow: &mut cardeal_storage::UnidadeDeTrabalho,
) -> cardeal_kernel::Resultado<crate::DocumentoResumo> {
    sqlite::emitir(transferencia, dados, ctx, uow)
}

/// O que impede a transferência da empresa da sessão para `destino` (o regime, a raiz dos
/// CNPJs, os dados fiscais das duas), antes de mexer no estoque. Vazio = pode.
///
/// # Errors
/// Os do SQLite.
#[cfg(feature = "sqlite")]
pub fn conferir_transferencia_comum(
    c: &rusqlite::Connection,
    origem: Id,
    destino: Id,
) -> cardeal_kernel::Resultado<Vec<String>> {
    Ok(
        match (
            crate::nfce::sqlite::emitente(c, origem)?,
            crate::nfce::sqlite::emitente(c, destino)?,
        ) {
            (Ok(o), Ok(d)) => impedimentos_da_transferencia(&o, &d),
            (Err(a), Ok(_)) => a,
            (Ok(_), Err(b)) => b
                .into_iter()
                .map(|p| format!("na filial que recebe, {p}"))
                .collect(),
            (Err(mut a), Err(b)) => {
                a.extend(b.into_iter().map(|p| format!("na filial que recebe, {p}")));
                a
            }
        },
    )
}

/// A NF-e da transferência na empresa `empresa` (a que envia), se já foi criada — para a filial
/// que recebe conferir que a nota está autorizada.
///
/// # Errors
/// Os do SQLite.
#[cfg(feature = "sqlite")]
pub fn documento_da_transferencia_comum(
    c: &rusqlite::Connection,
    empresa: Id,
    transferencia: Id,
) -> cardeal_kernel::Resultado<Option<crate::DocumentoResumo>> {
    crate::sql_documento::documento_da_origem(c, empresa, ORIGEM_TRANSFERENCIA, transferencia)
}

#[cfg(feature = "sqlite")]
pub(crate) mod sqlite {
    use cardeal_kernel::{Id, Resultado};
    use cardeal_modkit::Ctx;
    use cardeal_storage::UnidadeDeTrabalho;

    use super::{montar_nfe_de_transferencia, DadosDaTransferencia, ORIGEM_TRANSFERENCIA};
    use crate::devolucao_compra::sqlite::criar_ou_remontar;
    use crate::documento::DocumentoResumo;

    /// A nota montada com o emitente, a filial e os produtos — ou o que impede.
    fn montar(
        c: &rusqlite::Connection,
        ctx: &Ctx,
        dados: &DadosDaTransferencia,
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
        let filial = crate::nfce::sqlite::emitente(c, dados.destino)?.map_err(|p| {
            p.into_iter()
                .map(|x| format!("na filial que recebe, {x}"))
                .collect::<Vec<_>>()
        });
        let ids: Vec<Id> = dados.itens.iter().map(|i| i.produto).collect();
        let produtos = mod_estoque::produtos_para_nota(c, ctx.organizacao, &ids)?;
        let nota = match (emitente, filial) {
            (Ok(e), Ok(f)) => montar_nfe_de_transferencia(e, &f, dados, &produtos, serie, numero),
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
        transferencia: Id,
        dados: &DadosDaTransferencia,
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
    ) -> Resultado<DocumentoResumo> {
        let total = dados.itens.iter().map(|i| i.valor).sum();
        criar_ou_remontar(
            ORIGEM_TRANSFERENCIA,
            transferencia,
            total,
            ctx,
            uow,
            |ctx, uow, serie, numero| montar(uow.conexao(), ctx, dados, serie, numero),
        )
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use cardeal_fiscal::{Crt, EnderecoEmitente};
    use cardeal_kernel::{Cnpj, Uf};

    fn empresa(cnpj: &str, uf: Uf, crt: Crt) -> Emitente {
        Emitente {
            cnpj: Cnpj::novo(cnpj).unwrap(),
            razao_social: "Loja do Grupo LTDA".into(),
            nome_fantasia: "Loja".into(),
            inscricao_estadual: "0960000000".into(),
            inscricao_municipal: None,
            crt,
            endereco: EnderecoEmitente {
                logradouro: "Rua A".into(),
                numero: "1".into(),
                complemento: String::new(),
                bairro: "Centro".into(),
                codigo_municipio: "4314902".into(),
                municipio: "Porto Alegre".into(),
                uf,
                cep: "90010000".into(),
                telefone: None,
            },
        }
    }

    fn produto(id: Id) -> ProdutoParaNota {
        ProdutoParaNota {
            id,
            nome: "Parafuso sextavado".into(),
            ncm: "73181500".into(),
            cest: None,
            gtin: None,
            unidade: "UN".into(),
            fiscal: mod_estoque::fiscal::CamposFiscais::default(),
        }
    }

    #[test]
    fn a_raiz_do_cnpj_separa_filial_de_empresa_diferente() {
        assert!(mesma_raiz("11.222.333/0001-81", "11222333000262"));
        assert!(!mesma_raiz("11222333000181", "45543915000181"));
        assert!(!mesma_raiz("", ""));
    }

    #[test]
    fn a_transferencia_vai_ao_custo_com_cfop_de_transferencia_e_sem_icms() {
        let p = Id::novo();
        let produtos: HashMap<Id, ProdutoParaNota> = [(p, produto(p))].into();
        let dados = DadosDaTransferencia {
            destino: Id::novo(),
            observacao: Some("reposição da filial".into()),
            itens: vec![ItemTransferidoNaNota {
                produto: p,
                quantidade: Quantidade::unidades(10),
                custo_unitario: Preco::de_str("2.5").unwrap(),
                valor: Dinheiro::reais(25),
            }],
        };
        let matriz = empresa("11222333000181", Uf::Rs, Crt::SimplesNacional);
        let filial = empresa("11222333000262", Uf::Rs, Crt::SimplesNacional);
        let nfe =
            montar_nfe_de_transferencia(matriz.clone(), &filial, &dados, &produtos, 1, 7).unwrap();
        assert_eq!(nfe.itens[0].cfop, "5152");
        assert_eq!(nfe.itens[0].impostos.icms.csosn.as_deref(), Some("400"));
        assert_eq!(nfe.total(), Dinheiro::reais(25));
        assert_eq!(nfe.finalidade, FinalidadeNfe::Normal);
        assert!(nfe.pagamentos.is_empty());
        assert!(!nfe.consumidor_final, "a filial é contribuinte");
        assert!(nfe
            .informacoes
            .as_deref()
            .is_some_and(|i| i.contains("CNPJ base 11222333") && i.contains("reposição")));

        // Outra UF: 6152.
        let fora = empresa("11222333000262", Uf::Sc, Crt::SimplesNacional);
        let nfe =
            montar_nfe_de_transferencia(matriz.clone(), &fora, &dados, &produtos, 1, 8).unwrap();
        assert_eq!(nfe.itens[0].cfop, "6152");

        // Empresa de outra raiz e regime normal: recusa, com o porquê.
        let outra = empresa("45543915000181", Uf::Rs, Crt::SimplesNacional);
        let erro =
            montar_nfe_de_transferencia(matriz, &outra, &dados, &produtos, 1, 9).unwrap_err();
        assert!(erro[0].contains("por venda"), "{erro:?}");
        let normal = empresa("11222333000181", Uf::Rs, Crt::RegimeNormal);
        let erro =
            montar_nfe_de_transferencia(normal, &filial, &dados, &produtos, 1, 9).unwrap_err();
        assert!(erro[0].contains("regime normal"), "{erro:?}");
    }
}
