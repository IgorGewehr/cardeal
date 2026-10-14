//! O último lançamento de uma pessoa, para pré-preencher o próximo: escolheu o fornecedor da
//! luz, o formulário já vem com "Conta de luz", a categoria Energia e o valor do mês passado.
//! Só conta o que foi lançado à mão (ou por recorrência) — o título de uma nota de compra, de
//! um pedido ou de uma OS não é modelo de lançamento manual.

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
use cardeal_kernel::{Data, Dinheiro, Id};
use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::repositorio::{blob, data_de, id_de, persist};

/// O que o último lançamento da pessoa deixa para o próximo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UltimoLancamento {
    /// A data (emissão) do último.
    pub emissao: Data,
    /// O valor total.
    pub valor_total: Dinheiro,
    /// A descrição.
    pub observacao: Option<String>,
    /// A categoria.
    pub categoria: Option<Id>,
    /// Em quantas parcelas foi.
    pub parcelas: u32,
}

#[cfg(feature = "sqlite")]
fn ultimo(
    conexao: &Connection,
    empresa: Id,
    especie: &str,
    pessoa: Id,
) -> Resultado<Option<UltimoLancamento>> {
    conexao
        .query_row(
            "SELECT t.emissao, t.valor_original, t.observacao, t.categoria,
                    (SELECT COUNT(*) FROM financeiro_parcela p WHERE p.titulo = t.id)
             FROM financeiro_titulo t
             WHERE t.empresa = ?1 AND t.especie = ?2 AND t.contraparte_id = ?3
               AND t.cancelado_em IS NULL
               AND t.origem_modulo IN ('avulso', 'financeiro_recorrencia')
             ORDER BY t.emissao DESC, t.criado_em DESC LIMIT 1",
            params![blob(empresa), especie, blob(pessoa)],
            |r| {
                Ok(UltimoLancamento {
                    emissao: data_de(r.get(0)?),
                    valor_total: Dinheiro::centavos(r.get(1)?),
                    observacao: r.get(2)?,
                    categoria: r.get::<_, Option<Vec<u8>>>(3)?.map(id_de),
                    parcelas: u32::try_from(r.get::<_, i64>(4)?).unwrap_or(1),
                })
            },
        )
        .optional()
        .map_err(persist)
}

/// O último lançamento a receber (à mão) de um cliente.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct UltimoLancamentoAReceber {
    /// O cliente.
    pub pessoa: Id,
}

impl Consulta for UltimoLancamentoAReceber {
    type Saida = Option<UltimoLancamento>;
    const PERMISSAO: &'static str = "financeiro.receber.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        ultimo(conexao, ctx.empresa, "Receber", self.pessoa)
    }
}

/// O último lançamento a pagar (à mão) de um fornecedor.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct UltimoLancamentoAPagar {
    /// O fornecedor.
    pub pessoa: Id,
}

impl Consulta for UltimoLancamentoAPagar {
    type Saida = Option<UltimoLancamento>;
    const PERMISSAO: &'static str = "financeiro.pagar.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        ultimo(conexao, ctx.empresa, "Pagar", self.pessoa)
    }
}
