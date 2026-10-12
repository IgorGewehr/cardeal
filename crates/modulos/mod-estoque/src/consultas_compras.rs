//! A consulta do histórico das últimas compras de um produto (`estoque.historico_de_compras.v1`).

use cardeal_kernel::Id;
#[cfg(feature = "sqlite")]
use cardeal_kernel::{Preco, Quantidade, Resultado};
use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::historico_compras::ResumoDeCompras;

/// As últimas compras (entradas com custo) de um produto, com a variação de custo entre elas.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct HistoricoDeCompras {
    /// O produto.
    pub produto: Id,
}

impl Consulta for HistoricoDeCompras {
    type Saida = ResumoDeCompras;
    const PERMISSAO: &'static str = "estoque.produto.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        use cardeal_kernel::Instante;

        use crate::historico_compras::{montar_historico, EntradaBruta, COMPRAS_NO_HISTORICO};
        use crate::repositorio::{blob, persist};

        // Uma a mais que o limite: a variação da última exibida precisa da anterior a ela.
        let mut stmt = conexao
            .prepare(
                "SELECT criado_em, quantidade, custo_unitario, origem_modulo
                 FROM estoque_movimento
                 WHERE produto = ?1 AND empresa = ?2 AND tipo = 'Entrada'
                   AND custo_unitario IS NOT NULL AND custo_unitario > 0
                 ORDER BY criado_em DESC, id DESC
                 LIMIT ?3",
            )
            .map_err(persist)?;
        let entradas = stmt
            .query_map(
                rusqlite::params![
                    blob(self.produto),
                    blob(ctx.empresa),
                    i64::try_from(COMPRAS_NO_HISTORICO + 1).unwrap_or(11)
                ],
                |r| {
                    Ok(EntradaBruta {
                        quando: Instante::de_micros(r.get::<_, i64>(0)?),
                        quantidade: Quantidade::interna(r.get::<_, i64>(1)?),
                        custo: Preco::interna(r.get::<_, i64>(2)?),
                        origem_modulo: r.get(3)?,
                    })
                },
            )
            .map_err(persist)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)?;
        Ok(montar_historico(&entradas))
    }
}
