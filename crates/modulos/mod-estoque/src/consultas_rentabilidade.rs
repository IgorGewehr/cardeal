//! As consultas de `estoque` que alimentam a rentabilidade por nível da estrutura
//! mercadológica (`cardeal-analytics`): o custo do que saiu, por produto e origem, e os
//! encargos que a empresa carrega sobre a venda.

use cardeal_kernel::{Dinheiro, Id, Periodo, Quantidade};
use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::consultas_preco::EncargosDaEmpresa;
#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;

/// O que saiu do estoque de um produto por uma origem, e quanto custou.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaidaDoProduto {
    /// O produto.
    pub produto: Id,
    /// O módulo que originou a saída (`vendas`, `pdv`, `os`, `estoque`…).
    pub origem_modulo: String,
    /// A quantidade que saiu.
    pub quantidade: Quantidade,
    /// Σ(quantidade × custo unitário gravado na saída). Saída sem custo gravado usa o custo
    /// médio atual do produto.
    pub custo: Dinheiro,
}

/// As saídas (movimentos `Saida`) do período, somadas por produto e origem. O dia vai da meia-noite
/// do primeiro à meia-noite seguinte ao último, no fuso da empresa.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SaidasPorProdutoEOrigem {
    /// O período (datas, inclusive).
    pub periodo: Periodo,
}

impl Consulta for SaidasPorProdutoEOrigem {
    type Saida = Vec<SaidaDoProduto>;
    const PERMISSAO: &'static str = "estoque.abc.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        use cardeal_kernel::{Hora, Instante};

        use crate::repositorio::{blob, id_de, persist};

        let inicio = |d| Instante::de_data_hora(d, Hora::MEIA_NOITE, ctx.fuso).em_micros();
        let de = inicio(self.periodo.de);
        let ate = inicio(self.periodo.ate.mais_dias(1));
        let mut stmt = conexao
            .prepare(
                "SELECT m.produto, m.origem_modulo, SUM(m.quantidade),
                        SUM(m.quantidade * COALESCE(m.custo_unitario,
                              (SELECT MAX(s.custo_medio) FROM estoque_saldo_local s
                               WHERE s.produto = m.produto AND s.empresa = m.empresa), 0))
                 FROM estoque_movimento m
                 WHERE m.empresa = ?1 AND m.tipo = 'Saida'
                   AND m.criado_em >= ?2 AND m.criado_em < ?3
                 GROUP BY m.produto, m.origem_modulo
                 ORDER BY m.produto, m.origem_modulo",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map(rusqlite::params![blob(ctx.empresa), de, ate], |r| {
                // Σ(qtd × custo) vem em 1e-10 de real; o centavo é 1e-2.
                let bruto: i64 = r.get(3)?;
                let centavos = (i128::from(bruto) + 50_000_000).div_euclid(100_000_000);
                Ok(SaidaDoProduto {
                    produto: id_de(r.get::<_, Vec<u8>>(0)?),
                    origem_modulo: r.get(1)?,
                    quantidade: Quantidade::interna(r.get::<_, i64>(2)?),
                    custo: Dinheiro::centavos(i64::try_from(centavos).unwrap_or(0)),
                })
            })
            .map_err(persist)?;
        linhas
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)
    }
}

/// Os percentuais que incidem hoje sobre a venda da empresa (operação, imposto, comissão,
/// taxas) e a margem líquida mínima — a régua da margem líquida **estimada** da rentabilidade.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EncargosAtuais;

impl Consulta for EncargosAtuais {
    type Saida = EncargosDaEmpresa;
    const PERMISSAO: &'static str = "estoque.produto.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        crate::consultas_preco::encargos_da_empresa(conexao, ctx)
    }
}
