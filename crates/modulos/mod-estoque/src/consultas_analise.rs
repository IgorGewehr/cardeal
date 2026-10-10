//! A consulta da análise de estoque (curva ABC, cobertura, giro).

use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::analise::ItemDeAnalise;
#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;

/// A curva ABC, a cobertura e o giro dos produtos ativos, pelo que saiu nos últimos `dias`
/// (de 7 a 365).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AnaliseDeEstoque {
    /// A janela, em dias.
    pub dias: u32,
}

impl Consulta for AnaliseDeEstoque {
    type Saida = Vec<ItemDeAnalise>;
    const PERMISSAO: &'static str = "estoque.abc.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        use cardeal_kernel::{Arredondamento, Dinheiro, Id, Preco, Quantidade};

        use crate::analise::{analisar, EntradaDeAnalise};
        use crate::repositorio::{blob, id_de, persist};

        let dias = self.dias.clamp(7, 365);
        let desde = ctx.agora.mais_segundos(-i64::from(dias) * 24 * 60 * 60);
        let mut stmt = conexao
            .prepare(
                "SELECT p.id, p.nome,
                    COALESCE((SELECT SUM(m.quantidade) FROM estoque_movimento m
                              WHERE m.produto = p.id AND m.empresa = ?2
                                AND m.tipo = 'Saida' AND m.criado_em >= ?3), 0),
                    COALESCE((SELECT SUM(m.quantidade * COALESCE(m.custo_unitario, 0))
                              FROM estoque_movimento m
                              WHERE m.produto = p.id AND m.empresa = ?2
                                AND m.tipo = 'Saida' AND m.criado_em >= ?3), 0),
                    COALESCE((SELECT SUM(s.quantidade_disponivel) FROM estoque_saldo_local s
                              WHERE s.produto = p.id AND s.empresa = ?2), 0),
                    COALESCE((SELECT MAX(s.custo_medio) FROM estoque_saldo_local s
                              WHERE s.produto = p.id AND s.empresa = ?2), 0)
                 FROM estoque_produto p WHERE p.empresa = ?1 AND p.ativo = 1",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map(
                rusqlite::params![blob(ctx.organizacao), blob(ctx.empresa), desde.em_micros()],
                |r| {
                    Ok((
                        id_de(r.get::<_, Vec<u8>>(0)?),
                        r.get::<_, String>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, i64>(3)?,
                        r.get::<_, i64>(4)?,
                        r.get::<_, i64>(5)?,
                    ))
                },
            )
            .map_err(persist)?
            .collect::<rusqlite::Result<Vec<(Id, String, i64, i64, i64, i64)>>>()
            .map_err(persist)?;
        let entradas: Vec<EntradaDeAnalise> = linhas
            .into_iter()
            .map(|(produto, nome, saidas, bruto, saldo, custo)| {
                let saidas_q = Quantidade::interna(saidas);
                // Σ(qtd × custo) vem em 1e-10 de real; o centavo é 1e-2.
                let mut valor = Dinheiro::centavos(
                    i64::try_from((i128::from(bruto) + 50_000_000) / 100_000_000).unwrap_or(0),
                );
                if valor.e_zero() && saidas > 0 {
                    // Saída sem custo gravado: usa o custo médio de hoje.
                    valor = Dinheiro::de_total(
                        saidas_q,
                        Preco::interna(custo),
                        Arredondamento::MeioAcima,
                    );
                }
                EntradaDeAnalise {
                    produto,
                    nome,
                    saidas: saidas_q,
                    valor_saidas: valor,
                    saldo: Quantidade::interna(saldo),
                    custo: Dinheiro::de_total(
                        Quantidade::unidades(1),
                        Preco::interna(custo),
                        Arredondamento::MeioAcima,
                    ),
                }
            })
            .collect();
        Ok(analisar(&entradas, dias))
    }
}
