//! Escrita dos lançamentos pessoais. Toda alteração filtra por `usuario` da sessão: ninguém
//! mexe no pessoal de outro, nem com o id em mãos.

use cardeal_kernel::{Data, Id};
#[cfg(feature = "sqlite")]
use cardeal_kernel::{Erro, Resultado};
#[cfg(feature = "sqlite")]
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_modkit::{Comando, Ctx};
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
#[cfg(feature = "sqlite")]
use rusqlite::params;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use super::consultas::{dias, tipo_txt};
#[cfg(feature = "sqlite")]
use super::gerar;
use super::NovoPessoal;
#[cfg(feature = "sqlite")]
use crate::repositorio::{blob, persist};

/// Lança uma receita/despesa pessoal (com as parcelas ou meses que ela tiver).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LancarPessoal(pub NovoPessoal);

#[cfg(feature = "sqlite")]
impl Comando for LancarPessoal {
    /// Os ids gerados, em ordem.
    type Saida = Vec<Id>;
    const PERMISSAO: &'static str = "financeiro.pessoal";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let lancamentos = gerar(&self.0, ctx.hoje())?;
        let agora = uow.agora().em_micros();
        let conn = uow.conexao();
        let mut ids = Vec::with_capacity(lancamentos.len());
        for l in lancamentos {
            conn.execute(
                "INSERT INTO financeiro_pessoal
                   (id, empresa, usuario, grupo, tipo, descricao, categoria, valor, vencimento,
                    parcela, parcelas, cartao, pago_em, criado_em)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
                params![
                    blob(l.id),
                    blob(ctx.empresa),
                    blob(ctx.usuario),
                    blob(l.grupo),
                    tipo_txt(l.tipo),
                    l.descricao,
                    l.categoria,
                    l.valor.em_centavos(),
                    dias(l.vencimento),
                    i64::from(l.parcela),
                    i64::from(l.parcelas),
                    l.cartao,
                    l.pago_em.map(dias),
                    agora,
                ],
            )
            .map_err(persist)?;
            ids.push(l.id);
        }
        Ok(ids)
    }
}

/// Marca (ou desmarca) lançamentos como pagos/recebidos — um só, ou a fatura inteira.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarcarPessoalPago {
    /// Os lançamentos.
    pub lancamentos: Vec<Id>,
    /// `true` = pago em `data`; `false` = volta a previsto.
    pub pago: bool,
    /// A data do pagamento.
    pub data: Data,
}

#[cfg(feature = "sqlite")]
impl Comando for MarcarPessoalPago {
    /// Quantos foram alterados.
    type Saida = usize;
    const PERMISSAO: &'static str = "financeiro.pessoal";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let pago_em = self.pago.then(|| dias(self.data));
        let mut n = 0;
        for id in self.lancamentos {
            n += uow
                .conexao()
                .execute(
                    "UPDATE financeiro_pessoal SET pago_em = ?1
                     WHERE id = ?2 AND empresa = ?3 AND usuario = ?4",
                    params![pago_em, blob(id), blob(ctx.empresa), blob(ctx.usuario)],
                )
                .map_err(persist)?;
        }
        Ok(n)
    }
}

/// Exclui um lançamento — e, se pedido, as parcelas/meses seguintes do mesmo grupo (cancelar
/// a faculdade a partir deste mês).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ExcluirPessoal {
    /// O lançamento.
    pub lancamento: Id,
    /// Também os seguintes do grupo.
    pub e_seguintes: bool,
}

#[cfg(feature = "sqlite")]
impl Comando for ExcluirPessoal {
    /// Quantos foram excluídos.
    type Saida = usize;
    const PERMISSAO: &'static str = "financeiro.pessoal";
    const RISCO: Risco = Risco::Medio;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let conn = uow.conexao();
        let (grupo, parcela): (Vec<u8>, i64) = conn
            .query_row(
                "SELECT grupo, parcela FROM financeiro_pessoal
                 WHERE id = ?1 AND empresa = ?2 AND usuario = ?3",
                params![blob(self.lancamento), blob(ctx.empresa), blob(ctx.usuario)],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|_| Erro::nao_encontrado("lançamento pessoal"))?;
        let n = if self.e_seguintes {
            conn.execute(
                "DELETE FROM financeiro_pessoal
                 WHERE grupo = ?1 AND parcela >= ?2 AND empresa = ?3 AND usuario = ?4",
                params![grupo, parcela, blob(ctx.empresa), blob(ctx.usuario)],
            )
        } else {
            conn.execute(
                "DELETE FROM financeiro_pessoal WHERE id = ?1 AND empresa = ?2 AND usuario = ?3",
                params![blob(self.lancamento), blob(ctx.empresa), blob(ctx.usuario)],
            )
        }
        .map_err(persist)?;
        Ok(n)
    }
}
