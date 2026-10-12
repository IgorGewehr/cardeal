//! A receita por produto das peças de OS faturadas num período — a parte de `os` da
//! rentabilidade por nível da estrutura mercadológica (`cardeal-analytics`).

use cardeal_analytics::ReceitaDoProduto;
use cardeal_kernel::Periodo;
use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;

/// Quanto cada produto (peça) rendeu em OS **faturadas** no período — a data é a do faturamento
/// (o passo `Faturada` da linha do tempo, como em [`MargemDasOrdensNoPeriodo`](super::MargemDasOrdensNoPeriodo)).
/// A receita é `preço × quantidade` da peça; peça coberta por garantia entra com receita zero
/// (ela custou, mas não rendeu) e peça estornada fica de fora. **Mão de obra não entra**: não é
/// produto de estoque.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ReceitaPorProduto {
    /// O período (datas de faturamento, inclusive).
    pub periodo: Periodo,
}

impl Consulta for ReceitaPorProduto {
    type Saida = Vec<ReceitaDoProduto>;
    const PERMISSAO: &'static str = "os.margem.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        use std::collections::BTreeMap;

        use cardeal_kernel::{Arredondamento, Dinheiro, Hora, Instante, Quantidade};

        use super::{blob, id_de, itens_peca_da_ordem, persist};

        let inicio = |d| Instante::de_data_hora(d, Hora::MEIA_NOITE, ctx.fuso).em_micros();
        let de = inicio(self.periodo.de);
        let ate = inicio(self.periodo.ate.mais_dias(1));
        let mut stmt = conexao
            .prepare(
                "SELECT o.id
                 FROM os_ordem_servico o
                 WHERE o.empresa = ?1 AND o.estado = 'Faturada'
                   AND (SELECT MAX(h.momento) FROM os_historico h
                        WHERE h.ordem_servico = o.id AND h.estado = 'Faturada')
                       BETWEEN ?2 AND ?3 - 1",
            )
            .map_err(persist)?;
        let ordens = stmt
            .query_map(rusqlite::params![blob(ctx.empresa), de, ate], |r| {
                Ok(id_de(r.get::<_, Vec<u8>>(0)?))
            })
            .map_err(persist)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)?;

        let mut por_produto: BTreeMap<_, (Quantidade, Dinheiro)> = BTreeMap::new();
        for ordem in ordens {
            for peca in itens_peca_da_ordem(conexao, ordem)? {
                if peca.estornada {
                    continue;
                }
                let receita = if peca.coberto_garantia {
                    Dinheiro::ZERO
                } else {
                    Dinheiro::de_total(
                        peca.quantidade,
                        peca.preco_unitario,
                        Arredondamento::MeioAcima,
                    )
                };
                let e = por_produto
                    .entry(peca.produto)
                    .or_insert((Quantidade::interna(0), Dinheiro::ZERO));
                e.0 += peca.quantidade;
                e.1 += receita;
            }
        }
        Ok(por_produto
            .into_iter()
            .map(|(produto, (quantidade, receita))| ReceitaDoProduto {
                produto,
                quantidade,
                receita,
            })
            .collect())
    }
}
