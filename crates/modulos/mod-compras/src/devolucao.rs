//! **Devolução ao fornecedor** (`docs/modulos/compras.md` §5 `DevolverAoFornecedor`): parte ou
//! toda a mercadoria de uma nota confirmada volta. Numa transação só:
//! - a mercadoria sai do estoque ao valor da compra (`mod_estoque::devolver_parte_da_entrada`),
//!   e só se ainda está no local;
//! - a conta a pagar da nota é **abatida** (D Fornecedores / C Estoque, sem juros nem caixa); o
//!   que não couber — a conta já paga — vira **crédito com o fornecedor**, um título a receber
//!   dele (`mod_financeiro::abater_titulo_da_origem_comum`);
//! - a devolução fica registrada, item a item; devolvida toda, a nota vira `Devolvida`.
//!
//! O valor de cada item é a parte dele no custo da nota (produto − desconto + frete, seguro e
//! outras despesas rateados + ST e IPI), na proporção do que volta; a última devolução do item
//! leva o resto, para a soma fechar ao centavo. A **NF-e de devolução** sai à parte
//! ([`EmitirNfeDaDevolucao`]): com o XML da compra, os tributos voltam campo a campo.
//!
//! Sem aprovação em duas etapas (a spec previa `Solicitada` → `Concluida`): quem tem
//! `compras.devolucao.criar` devolve, e o comando é auditado.

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
use cardeal_kernel::{Arredondamento, Data, Dinheiro, Id, Instante, Quantidade};
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::{Comando, Consulta, Risco};
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// O mínimo de caracteres do motivo — vai à NF-e de devolução e ao histórico.
pub const MOTIVO_MINIMO_DEVOLUCAO: usize = 10;

/// Um item a devolver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemADevolver {
    /// O item da nota.
    pub item: Id,
    /// Quanto volta.
    pub quantidade: Quantidade,
}

/// Devolve ao fornecedor parte ou toda a mercadoria de uma nota confirmada.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevolverAoFornecedor {
    /// A nota de entrada.
    pub nota_entrada: Id,
    /// O que volta.
    pub itens: Vec<ItemADevolver>,
    /// Por quê (ao menos [`MOTIVO_MINIMO_DEVOLUCAO`] caracteres).
    pub motivo: String,
}

/// O que a devolução fez.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevolucaoFeita {
    /// A devolução.
    pub devolucao: Id,
    /// O valor devolvido.
    pub valor_total: Dinheiro,
    /// Quanto abateu da conta a pagar da nota.
    pub abatido: Dinheiro,
    /// O crédito com o fornecedor (título a receber), quando a conta não cobria.
    pub credito: Option<Id>,
    /// A nota foi devolvida inteira.
    pub nota_devolvida: bool,
}

/// Um item devolvido.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemDevolvido {
    /// O item da nota.
    pub item: Id,
    /// A descrição do fornecedor.
    pub descricao: String,
    /// Quanto voltou.
    pub quantidade: Quantidade,
    /// O valor.
    pub valor: Dinheiro,
}

/// Uma devolução ao fornecedor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevolucaoAoFornecedor {
    /// Identidade.
    pub id: Id,
    /// A nota de entrada.
    pub nota_entrada: Id,
    /// O fornecedor.
    pub fornecedor: Id,
    /// O dia.
    pub data: Data,
    /// Por quê.
    pub motivo: String,
    /// O valor devolvido.
    pub valor_total: Dinheiro,
    /// Quanto abateu da conta a pagar.
    pub abatido: Dinheiro,
    /// O crédito com o fornecedor, se houve.
    pub credito: Option<Id>,
    /// Os itens.
    pub itens: Vec<ItemDevolvido>,
    /// Quando.
    pub criada_em: Instante,
}

/// O valor de `quantidade` de um item cujo custo total é `custo_total` para `total` unidades,
/// dado o que dele já voltou (`ja`: quantidade e valor). A última devolução leva o resto.
#[must_use]
pub fn valor_devolvido(
    custo_total: Dinheiro,
    total: Quantidade,
    ja: (Quantidade, Dinheiro),
    quantidade: Quantidade,
) -> Dinheiro {
    if ja.0 + quantidade >= total {
        return custo_total - ja.1;
    }
    custo_total.fracao(
        quantidade.unidades_internas(),
        total.unidades_internas().max(1),
        Arredondamento::MeioAcima,
    )
}

/// As devoluções de uma nota, da mais nova para a mais antiga.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DevolucoesDaNota {
    /// A nota de entrada.
    pub nota_entrada: Id,
}

impl Consulta for DevolucoesDaNota {
    type Saida = Vec<DevolucaoAoFornecedor>;
    const PERMISSAO: &'static str = "compras.entrada.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Vec<DevolucaoAoFornecedor>> {
        sqlite::devolucoes_da_nota(c, ctx.empresa, self.nota_entrada)
    }
}

impl Comando for DevolverAoFornecedor {
    type Saida = DevolucaoFeita;
    const PERMISSAO: &'static str = "compras.devolucao.criar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<DevolucaoFeita> {
        sqlite::devolver(&self, ctx, uow)
    }
}

/// Cria (ou, rejeitada, remonta) a NF-e de devolução e a põe na fila fiscal. Só com o
/// SQLite: o `mod-fiscal` só vem com ele.
#[cfg(feature = "sqlite")]
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EmitirNfeDaDevolucao {
    /// A devolução.
    pub devolucao: Id,
}

#[cfg(feature = "sqlite")]
impl Comando for EmitirNfeDaDevolucao {
    type Saida = mod_fiscal::DocumentoResumo;
    const PERMISSAO: &'static str = "fiscal.emitir";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    fn executar(
        self,
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
    ) -> Resultado<mod_fiscal::DocumentoResumo> {
        sqlite::emitir_nfe(self.devolucao, ctx, uow)
    }
}

/// A NF-e de devolução a partir da nota, dos itens dela, da devolução e do XML da compra:
/// os valores e os tributos campo a campo, na proporção do que volta. O que o XML não detalha
/// (o rateio do frete, do seguro e das outras despesas, ou tudo, na nota lançada à mão) fecha o
/// valor da devolução: vai em frete, seguro e outras na proporção da nota.
#[cfg(feature = "sqlite")]
#[must_use]
pub fn dados_da_nfe(
    nota: &crate::NotaEntrada,
    itens: &[crate::ItemNotaEntrada],
    devolucao: &DevolucaoAoFornecedor,
    xml: Option<&cardeal_fiscal::NotaFiscalXml>,
) -> mod_fiscal::devolucao_compra::DadosDaDevolucao {
    use mod_fiscal::devolucao_compra::DadosDaDevolucao;
    let dados = devolucao
        .itens
        .iter()
        .filter_map(|d| {
            let (i, item) = itens.iter().enumerate().find(|(_, i)| i.id == d.item)?;
            // O item do XML: o da mesma posição, se o código bate; senão, o primeiro com o
            // código.
            let do_xml = xml.and_then(|x| {
                x.itens
                    .get(i)
                    .filter(|xi| xi.codigo_fornecedor == item.codigo_fornecedor)
                    .or_else(|| {
                        x.itens
                            .iter()
                            .find(|xi| xi.codigo_fornecedor == item.codigo_fornecedor)
                    })
            });
            Some(item_da_nfe(nota, item, d, do_xml))
        })
        .collect();
    DadosDaDevolucao {
        chave_da_compra: nota.chave_acesso.clone(),
        numero_da_compra: nota.numero.clone(),
        serie_da_compra: nota.serie.clone(),
        emissao_da_compra: nota.data_emissao,
        motivo: devolucao.motivo.clone(),
        itens: dados,
    }
}

#[cfg(feature = "sqlite")]
fn item_da_nfe(
    nota: &crate::NotaEntrada,
    item: &crate::ItemNotaEntrada,
    d: &ItemDevolvido,
    xml: Option<&cardeal_fiscal::ItemNfe>,
) -> mod_fiscal::devolucao_compra::ItemDevolvidoNaNota {
    use cardeal_fiscal::DevolucaoDoItem;
    use cardeal_kernel::Percentual;
    let (q, total) = (
        d.quantidade.unidades_internas(),
        item.quantidade.unidades_internas().max(1),
    );
    let parte = |v: Dinheiro| v.fracao(q, total, Arredondamento::MeioAcima);
    let bruto = Dinheiro::de_total(d.quantidade, item.valor_unitario, Arredondamento::MeioAcima);
    let mut desconto = parte(item.valor_desconto);
    let t = xml.map(|x| x.tributos.clone()).unwrap_or_default();
    let ipi = xml.map_or(Dinheiro::ZERO, |x| parte(x.valor_ipi));
    let mut devolvido = DevolucaoDoItem {
        modalidade_st: t.modalidade_st.clone(),
        mva_st: t.mva_st,
        base_st: parte(t.base_st),
        aliquota_st: t.aliquota_st,
        valor_st: parte(t.valor_st),
        base_fcp_st: parte(t.base_fcp_st),
        aliquota_fcp_st: t.aliquota_fcp_st,
        valor_fcp_st: parte(t.valor_fcp_st),
        percentual_devolvido: Percentual::da_razao(q, total),
        ipi_devolvido: ipi,
        ..DevolucaoDoItem::default()
    };
    // O que falta para fechar o valor da devolução: o rateio de frete, seguro e outras.
    let resto = d.valor - (bruto - desconto) - devolvido.soma();
    if resto.e_negativo() {
        desconto += resto.abs();
    } else if resto.e_positivo() {
        let pesos = [
            nota.valor_frete.em_centavos(),
            nota.valor_seguro.em_centavos(),
            nota.valor_outras_despesas.em_centavos(),
        ];
        let partes = if pesos.iter().any(|p| *p > 0) {
            resto.ratear_por_pesos(&pesos)
        } else {
            vec![Dinheiro::ZERO, Dinheiro::ZERO, resto]
        };
        devolvido.frete = partes[0];
        devolvido.seguro = partes[1];
        devolvido.outras = partes[2];
    }
    let situacao = t.situacao.as_deref().unwrap_or_default();
    mod_fiscal::devolucao_compra::ItemDevolvidoNaNota {
        produto: item.produto_casado.unwrap_or(Id::NULO),
        ncm: item.ncm.clone(),
        quantidade: d.quantidade,
        valor_unitario: item.valor_unitario,
        valor_bruto: bruto,
        desconto,
        origem: t.origem,
        icms: t
            .aliquota_icms
            .filter(|_| t.valor_icms.e_positivo())
            .map(|a| (parte(t.base_icms), a)),
        com_st: devolvido.valor_st.e_positivo()
            || matches!(
                situacao,
                "10" | "30" | "60" | "70" | "201" | "202" | "203" | "500"
            ),
        devolvido,
    }
}

#[cfg(feature = "sqlite")]
pub(crate) mod sqlite {
    use std::collections::HashMap;

    use cardeal_kernel::{CodigoErro, Data, Dinheiro, Erro, Id, Instante, Quantidade, Resultado};
    use cardeal_ledger::{Contraparte, PapelConta};
    use cardeal_modkit::Ctx;
    use cardeal_storage::UnidadeDeTrabalho;
    use rusqlite::{params, Connection};

    use super::{
        valor_devolvido, DevolucaoAoFornecedor, DevolucaoFeita, DevolverAoFornecedor,
        ItemDevolvido, MOTIVO_MINIMO_DEVOLUCAO,
    };
    use crate::erros::ErroCompras;
    use crate::eventos::DevolucaoConcluida;
    use crate::nota::EstadoNotaEntrada;
    use crate::repositorio::RepositorioCompras;

    #[allow(clippy::needless_pass_by_value)]
    fn falha(e: rusqlite::Error) -> Erro {
        Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string())
    }

    fn id_de(b: &[u8]) -> Id {
        <[u8; 16]>::try_from(b).map_or(Id::NULO, Id::de_bytes)
    }

    /// Quanto de cada item da nota já voltou: quantidade e valor.
    pub(crate) fn ja_devolvido(
        c: &Connection,
        nota: Id,
    ) -> Resultado<HashMap<Id, (Quantidade, Dinheiro)>> {
        let mut stmt = c
            .prepare(
                "SELECT i.item, SUM(i.quantidade), SUM(i.valor)
                 FROM compras_devolucao_item i JOIN compras_devolucao d ON d.id = i.devolucao
                 WHERE d.nota_entrada = ?1 GROUP BY i.item",
            )
            .map_err(falha)?;
        let v = stmt
            .query_map([nota.em_bytes().as_slice()], |r| {
                Ok((
                    id_de(&r.get::<_, Vec<u8>>(0)?),
                    (
                        Quantidade::interna(r.get(1)?),
                        Dinheiro::centavos(r.get(2)?),
                    ),
                ))
            })
            .map_err(falha)?
            .collect::<rusqlite::Result<HashMap<_, _>>>()
            .map_err(falha);
        v
    }

    /// A nota tem devolução?
    pub(crate) fn tem_devolucao(c: &Connection, nota: Id) -> Resultado<bool> {
        c.query_row(
            "SELECT EXISTS (SELECT 1 FROM compras_devolucao WHERE nota_entrada = ?1)",
            [nota.em_bytes().as_slice()],
            |r| r.get(0),
        )
        .map_err(falha)
    }

    pub(super) fn devolucoes_da_nota(
        c: &Connection,
        empresa: Id,
        nota: Id,
    ) -> Resultado<Vec<DevolucaoAoFornecedor>> {
        let mut stmt = c
            .prepare(
                "SELECT id FROM compras_devolucao WHERE empresa = ?1 AND nota_entrada = ?2
                 ORDER BY criada_em DESC",
            )
            .map_err(falha)?;
        let ids = stmt
            .query_map(
                params![empresa.em_bytes().as_slice(), nota.em_bytes().as_slice()],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .map_err(falha)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(falha)?;
        ids.iter()
            .filter_map(|b| devolucao(c, empresa, id_de(b)).transpose())
            .collect()
    }

    pub(crate) fn devolucao(
        c: &Connection,
        empresa: Id,
        id: Id,
    ) -> Resultado<Option<DevolucaoAoFornecedor>> {
        use rusqlite::OptionalExtension;
        let Some(mut d) = c
            .query_row(
                "SELECT nota_entrada, fornecedor, data, motivo, valor_total, abatido, credito,
                        criada_em
                 FROM compras_devolucao WHERE empresa = ?1 AND id = ?2",
                params![empresa.em_bytes().as_slice(), id.em_bytes().as_slice()],
                |r| {
                    Ok(DevolucaoAoFornecedor {
                        id,
                        nota_entrada: id_de(&r.get::<_, Vec<u8>>(0)?),
                        fornecedor: id_de(&r.get::<_, Vec<u8>>(1)?),
                        data: Data::de_dias(r.get(2)?),
                        motivo: r.get(3)?,
                        valor_total: Dinheiro::centavos(r.get(4)?),
                        abatido: Dinheiro::centavos(r.get(5)?),
                        credito: r.get::<_, Option<Vec<u8>>>(6)?.map(|b| id_de(&b)),
                        itens: Vec::new(),
                        criada_em: Instante::de_micros(r.get(7)?),
                    })
                },
            )
            .optional()
            .map_err(falha)?
        else {
            return Ok(None);
        };
        let mut stmt = c
            .prepare(
                "SELECT d.item, n.descricao_fornecedor, d.quantidade, d.valor
                 FROM compras_devolucao_item d
                 JOIN compras_item_nota_entrada n ON n.id = d.item
                 WHERE d.devolucao = ?1 ORDER BY n.rowid",
            )
            .map_err(falha)?;
        d.itens = stmt
            .query_map([id.em_bytes().as_slice()], |r| {
                Ok(ItemDevolvido {
                    item: id_de(&r.get::<_, Vec<u8>>(0)?),
                    descricao: r.get(1)?,
                    quantidade: Quantidade::interna(r.get(2)?),
                    valor: Dinheiro::centavos(r.get(3)?),
                })
            })
            .map_err(falha)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(falha)?;
        Ok(Some(d))
    }

    /// Os itens validados: (item, quantidade, valor), na ordem da nota.
    fn itens_validados(
        cmd: &DevolverAoFornecedor,
        itens: &[crate::ItemNotaEntrada],
        ja: &HashMap<Id, (Quantidade, Dinheiro)>,
    ) -> Resultado<Vec<(crate::ItemNotaEntrada, Quantidade, Dinheiro)>> {
        let mut pedidos: HashMap<Id, Quantidade> = HashMap::new();
        for p in &cmd.itens {
            if !p.quantidade.e_positiva() {
                continue;
            }
            if !itens.iter().any(|i| i.id == p.item) {
                return Err(Erro::de_dominio(&ErroCompras::ItemNaoPertenceANota));
            }
            *pedidos.entry(p.item).or_insert(Quantidade::ZERO) += p.quantidade;
        }
        if pedidos.is_empty() {
            return Err(Erro::de_dominio(&ErroCompras::DevolucaoVazia));
        }
        let mut v = Vec::new();
        for item in itens {
            let Some(&q) = pedidos.get(&item.id) else {
                continue;
            };
            let ja = ja
                .get(&item.id)
                .copied()
                .unwrap_or((Quantidade::ZERO, Dinheiro::ZERO));
            let resta = item.quantidade - ja.0;
            if q > resta {
                return Err(Erro::de_dominio(&ErroCompras::DevolveMaisQueEntrou {
                    item: item.descricao_fornecedor.clone(),
                    resta: resta.formatar(0),
                }));
            }
            let valor = valor_devolvido(item.custo_total(), item.quantidade, ja, q);
            v.push((item.clone(), q, valor));
        }
        Ok(v)
    }

    pub(super) fn devolver(
        cmd: &DevolverAoFornecedor,
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
    ) -> Resultado<DevolucaoFeita> {
        // 1. Carregar e validar.
        let motivo = cmd.motivo.split_whitespace().collect::<Vec<_>>().join(" ");
        if motivo.chars().count() < MOTIVO_MINIMO_DEVOLUCAO {
            return Err(Erro::de_dominio(&ErroCompras::MotivoDaDevolucaoCurto));
        }
        let mut nota = crate::comandos::carregar_nota(uow, cmd.nota_entrada)?;
        if nota.estado != EstadoNotaEntrada::Confirmada {
            return Err(Erro::de_dominio(&ErroCompras::EstadoDeNotaInvalido {
                atual: nota.estado.rotulo(),
                esperado: EstadoNotaEntrada::Confirmada.rotulo(),
            }));
        }
        let itens = RepositorioCompras::novo(uow).itens_da_nota(nota.id)?;
        let ja = ja_devolvido(uow.conexao(), nota.id)?;
        let validados = itens_validados(cmd, &itens, &ja)?;
        let total: Dinheiro = validados.iter().map(|(_, _, v)| *v).sum();
        let id = Id::novo();

        // 2. Estoque: sai ao valor da compra, se ainda está no local.
        for (item, q, valor) in &validados {
            let produto = item
                .produto_casado
                .ok_or_else(|| Erro::de_dominio(&ErroCompras::ItemNaoCasado(1)))?;
            mod_estoque::devolver_parte_da_entrada(
                "compras", nota.id, produto, *q, *valor, ctx, uow,
            )?;
        }

        // 3. Financeiro: abate a conta a pagar; o que sobra é crédito com o fornecedor.
        let abatimento = mod_financeiro::abater_titulo_da_origem_comum(
            &mod_financeiro::DadosAbatimento {
                origem_modulo: "compras",
                origem_id: nota.id,
                valor: total,
                papel_contrapartida: PapelConta::EstoqueMercadorias,
                origem_do_credito: "compras_devolucao",
                id_do_credito: id,
                contraparte: Contraparte::Fornecedor(nota.fornecedor),
                historico: format!("Devolução de compra — nota {}/{}", nota.numero, nota.serie),
            },
            ctx,
            uow,
        )?;

        // 4. Persistir.
        let c = uow.conexao();
        c.execute(
            "INSERT INTO compras_devolucao
               (id, empresa, nota_entrada, fornecedor, data, motivo, valor_total, abatido,
                credito, criada_em, criada_por)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                id.em_bytes().as_slice(),
                ctx.empresa.em_bytes().as_slice(),
                nota.id.em_bytes().as_slice(),
                nota.fornecedor.em_bytes().as_slice(),
                ctx.hoje().em_dias(),
                motivo,
                total.em_centavos(),
                abatimento.abatido.em_centavos(),
                abatimento.credito.map(|i| i.em_bytes().to_vec()),
                ctx.agora.em_micros(),
                ctx.usuario.em_bytes().as_slice(),
            ],
        )
        .map_err(falha)?;
        for (item, q, valor) in &validados {
            c.execute(
                "INSERT INTO compras_devolucao_item (devolucao, item, quantidade, valor)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    id.em_bytes().as_slice(),
                    item.id.em_bytes().as_slice(),
                    q.unidades_internas(),
                    valor.em_centavos()
                ],
            )
            .map_err(falha)?;
        }
        let ja = ja_devolvido(uow.conexao(), nota.id)?;
        let inteira = itens
            .iter()
            .all(|i| ja.get(&i.id).is_some_and(|(q, _)| *q >= i.quantidade));
        if inteira {
            nota.devolver().map_err(|e| Erro::de_dominio(&e))?;
            RepositorioCompras::novo(uow).atualizar_nota(&nota)?;
        }

        // 5. Publicar.
        uow.publicar(DevolucaoConcluida {
            devolucao: id,
            nota_entrada: nota.id,
            fornecedor: nota.fornecedor,
            valor_total: total,
            abatido: abatimento.abatido,
            credito: abatimento.credito,
        })
        .map_err(|e| Erro::de_dominio(&e))?;
        Ok(DevolucaoFeita {
            devolucao: id,
            valor_total: total,
            abatido: abatimento.abatido,
            credito: abatimento.credito,
            nota_devolvida: inteira,
        })
    }

    pub(super) fn emitir_nfe(
        devolucao: Id,
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
    ) -> Resultado<mod_fiscal::DocumentoResumo> {
        let d = self::devolucao(uow.conexao(), ctx.empresa, devolucao)?
            .ok_or_else(|| Erro::nao_encontrado("devolução"))?;
        let nota = crate::comandos::carregar_nota(uow, d.nota_entrada)?;
        let itens = RepositorioCompras::novo(uow).itens_da_nota(nota.id)?;
        let xml = RepositorioCompras::novo(uow)
            .xml_da_nota(nota.id)?
            .and_then(|x| cardeal_fiscal::interpretar(&x).ok());
        let dados = super::dados_da_nfe(&nota, &itens, &d, xml.as_ref());
        if dados.itens.iter().any(|i| i.produto == Id::NULO) {
            return Err(Erro::novo(
                CodigoErro::REGRA_VIOLADA,
                "Um item devolvido não está vinculado a um produto do estoque",
            ));
        }
        mod_fiscal::devolucao_compra::emitir_nfe_de_devolucao_comum(
            d.id,
            d.fornecedor,
            &dados,
            ctx,
            uow,
        )
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_valor_e_proporcional_e_a_ultima_leva_o_resto() {
        let custo = Dinheiro::centavos(10_000); // 100,00 por 3 unidades
        let tres = Quantidade::unidades(3);
        let um = Quantidade::unidades(1);
        let primeira = valor_devolvido(custo, tres, (Quantidade::ZERO, Dinheiro::ZERO), um);
        assert_eq!(primeira, Dinheiro::centavos(3_333));
        let segunda = valor_devolvido(custo, tres, (um, primeira), um);
        assert_eq!(segunda, Dinheiro::centavos(3_333));
        let terceira = valor_devolvido(
            custo,
            tres,
            (Quantidade::unidades(2), primeira + segunda),
            um,
        );
        assert_eq!(terceira, Dinheiro::centavos(3_334), "fecha ao centavo");
        assert_eq!(
            valor_devolvido(custo, tres, (Quantidade::ZERO, Dinheiro::ZERO), tres),
            custo
        );
    }
}
