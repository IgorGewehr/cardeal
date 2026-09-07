//! Estorna uma baixa: reverte o lançamento `Realizado` no Razão e reabre a parcela.
//!
//! `docs/modulos/financeiro.md` §5. Idempotência: uma baixa já estornada recusa um segundo
//! estorno (`ErroFinanceiro::BaixaJaEstornada`, checado contra `financeiro_baixa.estornada_em`).
//! Não há, ainda, um conceito de "período fechado" no sistema — a checagem que o spec original
//! previa (`PeriodoFechado`) fica para quando existir fechamento de competência.

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_ledger::{Razao, RepositorioRazao};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::erros::ErroFinanceiro;
use crate::eventos::BaixaEstornada;
use crate::repositorio::RepositorioFinanceiro;

/// Estorna uma baixa já aplicada.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EstornarBaixa {
    /// A baixa a estornar.
    pub baixa: Id,
    /// O motivo do estorno — exigido pelo próprio `Razao::estornar`.
    pub motivo: String,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct BaixaFoiEstornada {
    /// A parcela reaberta.
    pub parcela: Id,
    /// O lançamento de estorno gerado no Razão.
    pub lancamento_estorno: Id,
}

impl Comando for EstornarBaixa {
    type Saida = BaixaFoiEstornada;
    const PERMISSAO: &'static str = "financeiro.receber.estornar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let baixa = RepositorioFinanceiro::novo(uow)
            .buscar_baixa(self.baixa)?
            .ok_or_else(|| Erro::nao_encontrado("baixa"))?;
        let mut parcela = RepositorioFinanceiro::novo(uow)
            .buscar_parcela(baixa.parcela)?
            .ok_or_else(|| Erro::nao_encontrado("parcela"))?;

        // 2. Validar (domínio puro): idempotência do estorno.
        if baixa.estornada_em.is_some() {
            return Err(Erro::de_dominio(&ErroFinanceiro::BaixaJaEstornada));
        }

        // 3. Lançar: estorna o `Realizado` original.
        let lancamento_estorno = {
            let mut repo = RepositorioRazao::novo(uow);
            Razao::estornar(&mut repo, baixa.lancamento, &self.motivo, ctx.hoje())
                .map_err(|e| Erro::de_dominio(&e))?
        };

        // 4. Persistir: reabre a parcela, marca a baixa como estornada.
        parcela.reverter_baixa(baixa.principal);
        {
            let mut repo = RepositorioFinanceiro::novo(uow);
            repo.atualizar_parcela(&parcela)?;
            repo.marcar_baixa_estornada(baixa.id)?;
        }

        // 5. Publicar.
        uow.publicar(BaixaEstornada {
            baixa: baixa.id,
            parcela: parcela.id,
            lancamento_estorno,
        })
        .map_err(|e| Erro::de_dominio(&e))?;

        Ok(BaixaFoiEstornada {
            parcela: parcela.id,
            lancamento_estorno,
        })
    }
}
