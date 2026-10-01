//! Margem das OS: receita menos o custo das peças aplicadas — de um período (o cartão da
//! Visão geral do Financeiro) e de uma OS só (o diálogo de baixa da parcela da OS).

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
use cardeal_kernel::{Dinheiro, Id};
#[cfg(feature = "sqlite")]
use cardeal_modkit::{Consulta, Ctx};
#[cfg(feature = "sqlite")]
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use super::{blob, id_de, itens_peca_da_ordem, persist};

/// Receita, custo das peças e margem das OS faturadas num período — "quanto as OS deram de
/// verdade no mês". A data é a do faturamento (o passo `Faturada` da linha do tempo); OS
/// faturadas antes da linha do tempo existir ficam de fora. O custo é o das peças aplicadas e
/// não estornadas (o mesmo CMV do lançamento de faturamento); mão de obra não tem custo aqui.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MargemDasOrdensNoPeriodo {
    /// O período (datas de faturamento).
    pub periodo: cardeal_kernel::Periodo,
}

/// O que [`MargemDasOrdensNoPeriodo`] devolve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MargemDasOrdens {
    /// Quantas OS faturadas no período.
    pub ordens: u32,
    /// Soma do total faturado.
    pub receita: cardeal_kernel::Dinheiro,
    /// Soma do custo das peças aplicadas.
    pub custo_pecas: cardeal_kernel::Dinheiro,
}

impl MargemDasOrdens {
    /// Receita menos custo das peças.
    #[must_use]
    pub fn margem(&self) -> cardeal_kernel::Dinheiro {
        self.receita - self.custo_pecas
    }
}

#[cfg(feature = "sqlite")]
impl Consulta for MargemDasOrdensNoPeriodo {
    type Saida = MargemDasOrdens;
    const PERMISSAO: &'static str = "os.ordem.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        // O instante (micros UTC) do começo do primeiro dia e do fim do último, em Brasília.
        let meia_noite = |d: cardeal_kernel::Data| {
            cardeal_kernel::Instante::de_data_hora(
                d,
                cardeal_kernel::Hora::MEIA_NOITE,
                cardeal_kernel::Fuso::BRASILIA,
            )
            .em_micros()
        };
        let de = meia_noite(self.periodo.de);
        let ate = meia_noite(self.periodo.ate.mais_dias(1));
        let mut stmt = conexao
            .prepare(
                "SELECT o.id, o.valor_total
                 FROM os_ordem_servico o
                 WHERE o.empresa = ?1 AND o.estado = 'Faturada'
                   AND (SELECT MAX(h.momento) FROM os_historico h
                        WHERE h.ordem_servico = o.id AND h.estado = 'Faturada')
                       BETWEEN ?2 AND ?3 - 1",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map(rusqlite::params![blob(ctx.empresa), de, ate], |r| {
                Ok((id_de(r.get::<_, Vec<u8>>(0)?), r.get::<_, i64>(1)?))
            })
            .map_err(persist)?;
        let mut m = MargemDasOrdens::default();
        for linha in linhas {
            let (id, total) = linha.map_err(persist)?;
            m.ordens += 1;
            m.receita += cardeal_kernel::Dinheiro::centavos(total);
            for item in itens_peca_da_ordem(conexao, id)? {
                m.custo_pecas += item.total_custo();
            }
        }
        Ok(m)
    }
}

/// Receita, custo das peças e margem de uma OS — o que o Financeiro mostra ao receber a
/// parcela dela ("essa OS rendeu quanto?"). O custo é o das peças aplicadas e não estornadas;
/// peça ainda não aplicada entra em [`MargemDaOrdemServico::pecas_pendentes`], não no custo.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MargemDaOrdem {
    /// A ordem de serviço.
    pub ordem_servico: Id,
}

/// O que [`MargemDaOrdem`] devolve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MargemDaOrdemServico {
    /// O número da OS.
    pub numero: u64,
    /// O total da OS.
    pub receita: Dinheiro,
    /// O custo das peças aplicadas.
    pub custo_pecas: Dinheiro,
    /// Peças orçadas ainda não aplicadas (sem custo real ainda).
    pub pecas_pendentes: u32,
}

impl MargemDaOrdemServico {
    /// Receita menos custo das peças.
    #[must_use]
    pub fn margem(&self) -> Dinheiro {
        self.receita - self.custo_pecas
    }

    /// A margem em % da receita (inteiro, arredondado para baixo); `None` sem receita.
    #[must_use]
    pub fn percentual(&self) -> Option<i64> {
        self.receita
            .e_positivo()
            .then(|| self.margem().em_centavos() * 100 / self.receita.em_centavos())
    }
}

#[cfg(feature = "sqlite")]
impl Consulta for MargemDaOrdem {
    type Saida = Option<MargemDaOrdemServico>;
    const PERMISSAO: &'static str = "os.ordem.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let ordem = conexao
            .query_row(
                "SELECT numero, valor_total FROM os_ordem_servico WHERE empresa = ?1 AND id = ?2",
                [blob(ctx.empresa), blob(self.ordem_servico)],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
            )
            .optional()
            .map_err(persist)?;
        let Some((numero, total)) = ordem else {
            return Ok(None);
        };
        let mut m = MargemDaOrdemServico {
            numero: u64::try_from(numero).unwrap_or_default(),
            receita: Dinheiro::centavos(total),
            custo_pecas: Dinheiro::ZERO,
            pecas_pendentes: 0,
        };
        for item in itens_peca_da_ordem(conexao, self.ordem_servico)? {
            if item.estornada {
                continue;
            }
            if item.aplicada {
                m.custo_pecas += item.total_custo();
            } else {
                m.pecas_pendentes += 1;
            }
        }
        Ok(Some(m))
    }
}
