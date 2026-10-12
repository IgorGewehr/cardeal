//! A receita por produto dos cupons finalizados num período — a parte de `pdv` da rentabilidade
//! por nível da estrutura mercadológica (`cardeal-analytics`).

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

/// Quanto cada produto vendeu em cupons **finalizados** abertos no período. A receita é o
/// `total_item` (já com o desconto do item); item cancelado, cupom em andamento e cupom
/// cancelado ficam de fora. O dia é o de Brasília na abertura do cupom.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ReceitaPorProduto {
    /// O período (datas, inclusive).
    pub periodo: Periodo,
}

impl Consulta for ReceitaPorProduto {
    type Saida = Vec<ReceitaDoProduto>;
    const PERMISSAO: &'static str = "pdv.margem.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        use cardeal_kernel::{Hora, Instante};

        use crate::repositorio::{blob, id_de, persist};

        let inicio = |d| Instante::de_data_hora(d, Hora::MEIA_NOITE, ctx.fuso).em_micros();
        let de = inicio(self.periodo.de);
        let ate = inicio(self.periodo.ate.mais_dias(1));
        let mut stmt = conexao
            .prepare(
                "SELECT i.produto, SUM(i.quantidade), SUM(i.total_item)
                 FROM pdv_item_cupom i
                 JOIN pdv_cupom c ON c.id = i.cupom
                 WHERE c.empresa = ?1 AND c.estado = 'Finalizado' AND i.cancelado = 0
                   AND c.abertura >= ?2 AND c.abertura < ?3
                 GROUP BY i.produto
                 ORDER BY i.produto",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map(rusqlite::params![blob(ctx.empresa), de, ate], |r| {
                Ok(ReceitaDoProduto {
                    produto: id_de(r.get::<_, Vec<u8>>(0)?),
                    quantidade: Quantidade::interna(r.get::<_, i64>(1)?),
                    receita: Dinheiro::centavos(r.get::<_, i64>(2)?),
                })
            })
            .map_err(persist)?;
        linhas
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)
    }
}
