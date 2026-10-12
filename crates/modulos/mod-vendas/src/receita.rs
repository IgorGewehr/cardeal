//! A receita por produto dos pedidos faturados num período — a parte de `vendas` da
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
use cardeal_kernel::{Dinheiro, Quantidade, Resultado};

/// Quanto cada produto vendeu em pedidos **faturados** cuja data está no período. A receita é o
/// `total_item` (já com o desconto do item); rascunho, confirmado sem faturar e cancelado ficam
/// de fora.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ReceitaPorProduto {
    /// O período (datas dos pedidos, inclusive).
    pub periodo: Periodo,
}

impl Consulta for ReceitaPorProduto {
    type Saida = Vec<ReceitaDoProduto>;
    const PERMISSAO: &'static str = "vendas.margem.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        use crate::repositorio::{blob, id_de, persist};

        let mut stmt = conexao
            .prepare(
                "SELECT i.produto, SUM(i.quantidade), SUM(i.total_item)
                 FROM vendas_item_pedido i
                 JOIN vendas_pedido p ON p.id = i.pedido
                 WHERE p.empresa = ?1 AND p.estado = 'Faturado'
                   AND p.data BETWEEN ?2 AND ?3
                 GROUP BY i.produto
                 ORDER BY i.produto",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map(
                rusqlite::params![
                    blob(ctx.empresa),
                    self.periodo.de.em_dias(),
                    self.periodo.ate.em_dias()
                ],
                |r| {
                    Ok(ReceitaDoProduto {
                        produto: id_de(r.get::<_, Vec<u8>>(0)?),
                        quantidade: Quantidade::interna(r.get::<_, i64>(1)?),
                        receita: Dinheiro::centavos(r.get::<_, i64>(2)?),
                    })
                },
            )
            .map_err(persist)?;
        linhas
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)
    }
}
