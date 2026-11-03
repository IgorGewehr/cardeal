//! As notas de compra que entraram num mês, para o SPED (a EFD ICMS/IPI do `fiscal`): o XML de
//! cada uma, o dia da entrada e o produto do cadastro de cada item. O `fiscal` não enxerga as
//! compras (é o `compras` que depende dele), então quem gera o arquivo pede as entradas aqui e as
//! entrega ao `fiscal.gerar_efd.v1`.
//!
//! O mês da entrada é o da **confirmação** (o dia em que a mercadoria entrou no estoque, gravado
//! desde a migração v11); a nota confirmada antes disso usa o dia da emissão. Entram as
//! confirmadas e as devolvidas depois de confirmadas (a devolução é outra nota, a de saída).

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
use cardeal_kernel::{Competencia, Data, Id};
use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// O produto do cadastro casado a um item da nota.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProdutoDaEntrada {
    /// O produto.
    pub produto: Id,
    /// O nome.
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

/// Uma nota de compra que entrou no mês.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntradaParaEscriturar {
    /// O XML original.
    pub xml: String,
    /// O dia da entrada.
    pub data_entrada: Data,
    /// O produto de cada item, na ordem do XML (`None` = sem produto casado).
    pub itens: Vec<Option<ProdutoDaEntrada>>,
}

/// As entradas do mês.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntradasDoMes {
    /// As notas com XML.
    pub entradas: Vec<EntradaParaEscriturar>,
    /// As que entraram sem o XML guardado (número e fornecedor) — ficam fora do arquivo.
    pub sem_xml: Vec<String>,
}

/// As notas de compra confirmadas no mês, para escriturar.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EntradasParaEscriturar {
    /// O mês.
    pub competencia: Competencia,
}

impl Consulta for EntradasParaEscriturar {
    type Saida = EntradasDoMes;
    const PERMISSAO: &'static str = "compras.entrada.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<EntradasDoMes> {
        sqlite::entradas(c, ctx, self.competencia)
    }
}

#[cfg(feature = "sqlite")]
mod sqlite {
    use std::collections::HashMap;

    use cardeal_kernel::{CodigoErro, Competencia, Data, Erro, Id, Resultado};
    use cardeal_modkit::Ctx;
    use rusqlite::Connection;

    use super::{EntradaParaEscriturar, EntradasDoMes, ProdutoDaEntrada};

    #[allow(clippy::needless_pass_by_value)]
    fn falha(e: rusqlite::Error) -> Erro {
        Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string())
    }

    /// id, XML, dia da entrada, número e chave.
    type LinhaDaNota = (Vec<u8>, Option<String>, i64, String, Option<String>);

    pub(super) fn entradas(
        c: &Connection,
        ctx: &Ctx,
        competencia: Competencia,
    ) -> Resultado<EntradasDoMes> {
        let dias = |d: Data| i64::from(d.em_dias());
        let mut stmt = c
            .prepare(
                "SELECT id, xml, COALESCE(data_entrada, data_emissao), numero, chave_acesso
                 FROM compras_nota_entrada
                 WHERE empresa = ?1 AND estado IN ('Confirmada','Devolvida')
                   AND COALESCE(data_entrada, data_emissao) BETWEEN ?2 AND ?3
                 ORDER BY COALESCE(data_entrada, data_emissao), rowid",
            )
            .map_err(falha)?;
        let notas: Vec<LinhaDaNota> = stmt
            .query_map(
                rusqlite::params![
                    ctx.empresa.em_bytes().as_slice(),
                    dias(competencia.primeiro_dia()),
                    dias(competencia.ultimo_dia())
                ],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .map_err(falha)?
            .collect::<rusqlite::Result<_>>()
            .map_err(falha)?;
        let mut saida = EntradasDoMes::default();
        let mut por_nota = Vec::new();
        let mut todos: Vec<Id> = Vec::new();
        for (id, xml, dia, numero, chave) in notas {
            let id = <[u8; 16]>::try_from(id.as_slice()).map_or(Id::NULO, Id::de_bytes);
            let Some(xml) = xml.filter(|x| !x.trim().is_empty()) else {
                saida.sem_xml.push(format!(
                    "nº {numero}{}",
                    chave.map(|k| format!(" (chave {k})")).unwrap_or_default()
                ));
                continue;
            };
            let itens = crate::repositorio::itens_da_nota_por_conexao(c, id)?;
            let casados: Vec<Option<Id>> = itens.iter().map(|i| i.produto_casado).collect();
            todos.extend(casados.iter().flatten());
            let data = Data::de_dias(i32::try_from(dia).unwrap_or_default());
            por_nota.push((xml, data, casados));
        }
        todos.sort_unstable();
        todos.dedup();
        let produtos: HashMap<Id, mod_estoque::ProdutoParaNota> =
            mod_estoque::produtos_para_nota(c, ctx.empresa, &todos)?;
        for (xml, data_entrada, casados) in por_nota {
            let itens = casados
                .into_iter()
                .map(|p| {
                    p.and_then(|p| produtos.get(&p)).map(|p| ProdutoDaEntrada {
                        produto: p.id,
                        descricao: p.nome.clone(),
                        unidade: p.unidade.clone(),
                        ncm: p.ncm.clone(),
                        gtin: p.gtin.clone(),
                        cest: p.cest.clone(),
                    })
                })
                .collect();
            saida.entradas.push(EntradaParaEscriturar {
                xml,
                data_entrada,
                itens,
            });
        }
        Ok(saida)
    }
}
