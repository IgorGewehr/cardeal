//! A **NF-e de entrada da devolução de venda** (finalidade 4, `tpNF` 0): a mercadoria que o
//! cliente devolveu (`docs/modulos/vendas.md`, `ConcluirDevolucao`) entra numa NF-e emitida pela
//! própria empresa, referenciando a NF-e de venda. Os itens são os da venda, na quantidade que
//! voltou (o valor e o desconto na proporção); CFOP 1202/2202, ou 1411/2411 na mercadoria com
//! ST (vendida com 5405/6404); CSOSN 900.
//!
//! Só quando o cliente **não** é contribuinte do ICMS: o contribuinte emite ele mesmo a NF-e de
//! devolução (a empresa a recebe como qualquer nota). Só no Simples Nacional (no regime normal a
//! devolução recupera o ICMS da venda, que ainda não está no cálculo) e só da venda que saiu em
//! NF-e autorizada — a da NFC-e do PDV fica para a devolução do PDV.

use cardeal_fiscal::{ContribuicaoItem, FinalidadeNfe, IcmsItem, IndicadorIe, ItemDocumento, Nfe};
#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
use cardeal_kernel::{Arredondamento, Data, Dinheiro, Id, Percentual, Quantidade};
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::{Comando, Risco};
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::montagem::TEXTO_DO_SIMPLES;

/// A origem do documento fiscal (o id é o da devolução de venda).
pub const ORIGEM_DEVOLUCAO_VENDA: &str = "vendas_devolucao";

/// O CFOP de entrada da devolução de uma venda com `cfop`: 5102 → 1202, 6102 → 2202, 5101 →
/// 1201, 5405 → 1411, 6404 → 2411, 5401/5403 → 1410 (e 2410 de fora). `None`: sem devolução
/// conhecida.
#[must_use]
pub fn cfop_da_devolucao_de_venda(cfop: &str) -> Option<String> {
    let (dentro, resto) = match cfop.get(..1)? {
        "5" => ('1', cfop.get(1..)?),
        "6" => ('2', cfop.get(1..)?),
        _ => return None,
    };
    let fim = match resto {
        "101" => "201",
        "102" => "202",
        "405" | "404" | "403" => "411",
        "401" => "410",
        _ => return None,
    };
    Some(format!("{dentro}{fim}"))
}

/// Um item que voltou: o índice do item na NF-e de venda e a quantidade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemQueVoltou {
    /// A posição do item na NF-e de venda (a mesma do pedido).
    pub indice: usize,
    /// Quanto voltou.
    pub quantidade: Quantidade,
}

/// Monta a NF-e de entrada da devolução a partir da NF-e de venda — ou o que impede.
///
/// # Errors
/// O que impede: cliente contribuinte, regime normal, CFOP sem devolução, item fora da venda.
#[allow(clippy::too_many_arguments)]
pub fn montar_nfe_de_devolucao_de_venda(
    venda: &Nfe,
    chave_da_venda: &str,
    emissao_da_venda: Data,
    emitente: cardeal_fiscal::Emitente,
    itens: &[ItemQueVoltou],
    motivo: &str,
    serie: u32,
    numero: u64,
) -> Result<Nfe, Vec<String>> {
    let mut problemas = Vec::new();
    if venda.destinatario.indicador_ie == IndicadorIe::Contribuinte {
        problemas.push(
            "o cliente é contribuinte do ICMS: a NF-e de devolução é ele quem emite".to_string(),
        );
    }
    if !emitente.crt.e_simples() {
        problemas.push(
            "a devolução de venda no regime normal ainda não sai em NF-e — a recuperação do ICMS \
             da venda não está no cálculo"
                .to_string(),
        );
    }
    let mut saida = Vec::with_capacity(itens.len());
    for v in itens {
        let Some(i) = venda.itens.get(v.indice) else {
            problemas.push(format!("o item {} não está na NF-e de venda", v.indice + 1));
            continue;
        };
        let Some(cfop) = cfop_da_devolucao_de_venda(&i.cfop) else {
            problemas.push(format!(
                "{}: a venda saiu com o CFOP {}, sem devolução conhecida",
                i.descricao, i.cfop
            ));
            continue;
        };
        let parte = |d: Dinheiro| {
            d.fracao(
                v.quantidade.unidades_internas(),
                i.quantidade.unidades_internas().max(1),
                Arredondamento::MeioAcima,
            )
        };
        let outras = || ContribuicaoItem {
            cst: "98".into(),
            base: Some(Dinheiro::ZERO),
            aliquota: Some(Percentual::ZERO),
            valor: Some(Dinheiro::ZERO),
        };
        saida.push(ItemDocumento {
            cfop,
            quantidade: v.quantidade,
            valor_bruto: Dinheiro::de_total(
                v.quantidade,
                i.valor_unitario,
                Arredondamento::MeioAcima,
            ),
            desconto: parte(i.desconto),
            impostos: cardeal_fiscal::ImpostosItem {
                icms: IcmsItem {
                    origem: i.impostos.icms.origem,
                    cst: None,
                    csosn: Some("900".into()),
                    base: None,
                    aliquota: None,
                    valor: None,
                    reducao_base: None,
                },
                pis: outras(),
                cofins: outras(),
            },
            ..i.clone()
        });
    }
    if itens.is_empty() {
        problemas.push("a devolução não tem itens".into());
    }
    if !problemas.is_empty() {
        return Err(problemas);
    }
    let informacoes = format!(
        "Devolucao referente a NF-e {} serie {} de {}, chave {}. Motivo: {}. {TEXTO_DO_SIMPLES}",
        venda.numero,
        venda.serie,
        emissao_da_venda.formatar(),
        chave_da_venda,
        motivo.trim()
    );
    Ok(Nfe {
        emitente,
        destinatario: venda.destinatario.clone(),
        itens: saida,
        natureza: "Devolucao de venda".into(),
        saida: false,
        finalidade: FinalidadeNfe::Devolucao,
        referencias: vec![chave_da_venda.to_owned()],
        consumidor_final: venda.consumidor_final,
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

/// Cria (ou, rejeitada, remonta) a NF-e de entrada da devolução de venda e a põe na fila.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EmitirNfeDaDevolucaoDeVenda {
    /// A devolução (concluída).
    pub devolucao: Id,
}

impl Comando for EmitirNfeDaDevolucaoDeVenda {
    type Saida = crate::DocumentoResumo;
    const PERMISSAO: &'static str = "fiscal.emitir";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<crate::DocumentoResumo> {
        sqlite::emitir(self.devolucao, ctx, uow)
    }
}

/// A NF-e de entrada de uma devolução de venda, se já foi criada.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DocumentoDaDevolucaoDeVenda {
    /// A devolução.
    pub devolucao: Id,
}

impl cardeal_modkit::Consulta for DocumentoDaDevolucaoDeVenda {
    type Saida = Option<crate::DocumentoResumo>;
    const PERMISSAO: &'static str = "fiscal.ver";

    #[cfg(feature = "sqlite")]
    fn executar(
        self,
        ctx: &Ctx,
        c: &rusqlite::Connection,
    ) -> Resultado<Option<crate::DocumentoResumo>> {
        crate::sql_documento::documento_da_origem(
            c,
            ctx.empresa,
            ORIGEM_DEVOLUCAO_VENDA,
            self.devolucao,
        )
    }
}

#[cfg(feature = "sqlite")]
pub(crate) mod sqlite {
    use cardeal_fiscal::Nfe;
    use cardeal_kernel::{CodigoErro, Erro, Id, Resultado};
    use cardeal_modkit::Ctx;
    use cardeal_storage::UnidadeDeTrabalho;

    use super::{montar_nfe_de_devolucao_de_venda, ItemQueVoltou, ORIGEM_DEVOLUCAO_VENDA};
    use crate::documento::{DocumentoResumo, EstadoDocumento};
    use crate::sql_documento as sqld;

    /// A NF-e da devolução com a série/número dados — ou o que impede.
    pub(crate) fn montar(
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
        devolucao: Id,
        serie: u32,
        numero: u64,
    ) -> Resultado<Result<Nfe, Vec<String>>> {
        let d = mod_vendas::devolucao_por_id(uow.conexao(), ctx.empresa, devolucao)?;
        if d.estado != mod_vendas::EstadoDevolucao::Concluida {
            return Err(Erro::novo(
                CodigoErro::ESTADO_INVALIDO,
                "A NF-e de devolução sai da devolução concluída",
            ));
        }
        let pedido = mod_vendas::RepositorioVendas::novo(uow)
            .buscar_pedido(d.pedido_origem)?
            .ok_or_else(|| Erro::nao_encontrado("pedido"))?;
        let c = uow.conexao();
        let Some(venda) =
            sqld::documento_da_origem(c, ctx.empresa, crate::nfe_venda::ORIGEM_VENDAS, pedido.id)?
                .filter(|v| v.estado == EstadoDocumento::Autorizada)
        else {
            return Ok(Err(vec![
                "a venda não saiu em NF-e autorizada: sem ela não há NF-e de devolução".into(),
            ]));
        };
        let guardada = sqld::documento(c, ctx.empresa, venda.id)?
            .ok_or_else(|| Erro::nao_encontrado("NF-e de venda"))?;
        let Some(nfe) = guardada
            .nota
            .as_deref()
            .and_then(|b| postcard::from_bytes::<Nfe>(b).ok())
        else {
            return Ok(Err(
                vec!["a NF-e de venda guardada não está legível".into()],
            ));
        };
        let emissao = crate::fechamento::dia_da_emissao(
            guardada.xml.as_deref(),
            venda.autorizado_em.unwrap_or(venda.criado_em),
            ctx.fuso,
        );
        let itens: Vec<ItemQueVoltou> = d
            .itens
            .iter()
            .filter_map(|i| {
                Some(ItemQueVoltou {
                    indice: pedido.itens.iter().position(|p| p.id == i.item_pedido)?,
                    quantidade: i.quantidade,
                })
            })
            .collect();
        let emitente = match crate::nfce::sqlite::emitente(c, ctx.empresa)? {
            Ok(e) => e,
            Err(p) => return Ok(Err(p)),
        };
        let mut problemas = Vec::new();
        if crate::sql::certificado(c, ctx.empresa)?.is_none() {
            problemas.push(
                "o certificado digital A1 não está instalado (Configurações › Fiscal)".to_string(),
            );
        }
        let nota = montar_nfe_de_devolucao_de_venda(
            &nfe,
            venda.chave.as_deref().unwrap_or_default(),
            emissao,
            emitente,
            &itens,
            &d.motivo,
            serie,
            numero,
        );
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
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
    ) -> Resultado<DocumentoResumo> {
        let total =
            mod_vendas::devolucao_por_id(uow.conexao(), ctx.empresa, devolucao)?.valor_total;
        crate::devolucao_compra::sqlite::criar_ou_remontar(
            ORIGEM_DEVOLUCAO_VENDA,
            devolucao,
            total,
            ctx,
            uow,
            |ctx, uow, serie, numero| montar(ctx, uow, devolucao, serie, numero),
        )
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_cfop_de_entrada_da_devolucao() {
        assert_eq!(cfop_da_devolucao_de_venda("5102").as_deref(), Some("1202"));
        assert_eq!(cfop_da_devolucao_de_venda("6102").as_deref(), Some("2202"));
        assert_eq!(cfop_da_devolucao_de_venda("5405").as_deref(), Some("1411"));
        assert_eq!(cfop_da_devolucao_de_venda("6404").as_deref(), Some("2411"));
        assert_eq!(cfop_da_devolucao_de_venda("5101").as_deref(), Some("1201"));
        assert_eq!(cfop_da_devolucao_de_venda("5933"), None);
        assert_eq!(cfop_da_devolucao_de_venda("1202"), None);
    }
}
