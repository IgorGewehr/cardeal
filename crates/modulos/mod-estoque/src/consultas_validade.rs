//! Os lotes com validade ainda em estoque: os vencidos e os que vencem num prazo
//! (`docs/modulos/estoque.md` §6 `LotesProximosDoVencimento`). O vencimento é calculado na
//! hora contra a data de hoje — não depende de uma tarefa diária ter marcado o lote como
//! `Vencido`.

use cardeal_kernel::{Data, Dinheiro, Id, Preco, Quantidade};
use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use cardeal_kernel::{Arredondamento, Resultado};

/// Um lote com validade e saldo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoteComValidade {
    /// O lote.
    pub lote: Id,
    /// O código do lote.
    pub codigo: String,
    /// O produto.
    pub produto: Id,
    /// O nome do produto.
    pub nome: String,
    /// A sigla da unidade.
    pub unidade: String,
    /// O local.
    pub local: Id,
    /// O nome do local.
    pub nome_local: String,
    /// A validade.
    pub validade: Data,
    /// Dias até vencer (negativo = vencido há tantos dias).
    pub dias: i32,
    /// O que ainda resta do lote.
    pub saldo: Quantidade,
    /// O custo unitário do lote.
    pub custo_unitario: Preco,
    /// `saldo × custo` — o dinheiro em risco.
    pub valor: Dinheiro,
}

impl LoteComValidade {
    /// Já venceu?
    #[must_use]
    pub const fn vencido(&self) -> bool {
        self.dias < 0
    }
}

/// Os lotes com validade e saldo no CNPJ, do que vence primeiro para o último: todos os
/// vencidos e os que vencem em até `dias` (sem limite = todos).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LotesComValidade {
    /// Até quantos dias à frente; `None` = todos.
    pub dias: Option<i32>,
}

impl Consulta for LotesComValidade {
    type Saida = Vec<LoteComValidade>;
    const PERMISSAO: &'static str = "estoque.lote.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        use crate::repositorio::{blob, id_de, persist};

        let hoje = ctx.hoje();
        let limite = self.dias.map(|d| hoje.mais_dias(d));
        let mut stmt = conexao
            .prepare(
                "SELECT l.id, l.codigo, l.produto, p.nome, COALESCE(u.sigla, ''), l.local,
                        COALESCE(lc.nome, ''), l.validade, l.custo_unitario,
                        COALESCE((SELECT SUM(CASE WHEN m.tipo = 'Entrada' THEN m.quantidade
                                                  ELSE -m.quantidade END)
                                  FROM estoque_movimento m WHERE m.lote = l.id), 0)
                 FROM estoque_lote l
                 JOIN estoque_produto p ON p.id = l.produto
                 LEFT JOIN estoque_unidade u ON u.id = p.unidade_padrao
                 LEFT JOIN estoque_local lc ON lc.id = l.local
                 WHERE l.empresa = ?1 AND l.validade IS NOT NULL AND l.estado <> 'Esgotado'
                 ORDER BY l.validade ASC, p.nome ASC",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map([blob(ctx.empresa)], |r| {
                let validade = Data::de_dias(i32::try_from(r.get::<_, i64>(7)?).unwrap_or(0));
                let custo = Preco::interna(r.get(8)?);
                let saldo = Quantidade::interna(r.get(9)?);
                Ok(LoteComValidade {
                    lote: id_de(r.get::<_, Vec<u8>>(0)?),
                    codigo: r.get(1)?,
                    produto: id_de(r.get::<_, Vec<u8>>(2)?),
                    nome: r.get(3)?,
                    unidade: r.get(4)?,
                    local: id_de(r.get::<_, Vec<u8>>(5)?),
                    nome_local: r.get(6)?,
                    validade,
                    dias: hoje.dias_ate(validade),
                    saldo,
                    custo_unitario: custo,
                    valor: Dinheiro::de_total(saldo, custo, Arredondamento::MeioAcima),
                })
            })
            .map_err(persist)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)?;
        Ok(linhas
            .into_iter()
            .filter(|l| l.saldo.e_positiva())
            .filter(|l| limite.is_none_or(|lim| l.validade <= lim))
            .collect())
    }
}
