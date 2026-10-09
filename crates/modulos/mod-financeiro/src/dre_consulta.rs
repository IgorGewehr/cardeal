//! As consultas da DRE gerencial — `docs/modulos/financeiro.md` §6 (`DreDoMes`).

#[cfg(feature = "sqlite")]
use cardeal_kernel::{CodigoErro, Erro, Resultado};
use cardeal_kernel::{Competencia, Periodo};
use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::dre::Dre;

#[cfg(feature = "sqlite")]
fn apurar(conexao: &Connection, ctx: &Ctx, periodo: Periodo) -> Resultado<Dre> {
    let linhas = cardeal_ledger::resultado_por_conta(conexao, ctx.empresa, periodo)
        .map_err(|e| Erro::novo(CodigoErro::FALHA_INTERNA, format!("financeiro/DRE: {e}")))?;
    Ok(crate::dre::montar(periodo, &linhas))
}

/// A DRE de um período qualquer (competência de cada lançamento dentro dele).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DreDoPeriodo {
    /// O período, inclusive nas pontas.
    pub periodo: Periodo,
}

impl Consulta for DreDoPeriodo {
    type Saida = Dre;
    const PERMISSAO: &'static str = "financeiro.dre.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        apurar(conexao, ctx, self.periodo)
    }
}

/// O mês pedido ao lado do anterior e do mesmo mês do ano anterior — o comparativo da tela.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComparativoDre {
    /// O mês pedido.
    pub atual: Dre,
    /// O mês imediatamente anterior.
    pub mes_anterior: Dre,
    /// O mesmo mês, um ano antes.
    pub mesmo_mes_ano_anterior: Dre,
}

/// A DRE do mês, comparada com o mês anterior e com o mesmo mês do ano anterior.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DreComparativa {
    /// O mês de referência.
    pub mes: Competencia,
}

#[cfg(feature = "sqlite")]
fn periodo_do_mes(c: Competencia) -> Periodo {
    Periodo::novo(c.primeiro_dia(), c.ultimo_dia())
}

impl Consulta for DreComparativa {
    type Saida = ComparativoDre;
    const PERMISSAO: &'static str = "financeiro.dre.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let ano_antes = Competencia::nova(self.mes.ano() - 1, self.mes.mes());
        Ok(ComparativoDre {
            atual: apurar(conexao, ctx, periodo_do_mes(self.mes))?,
            mes_anterior: apurar(conexao, ctx, periodo_do_mes(self.mes.anterior()))?,
            mesmo_mes_ano_anterior: apurar(conexao, ctx, periodo_do_mes(ano_antes))?,
        })
    }
}
