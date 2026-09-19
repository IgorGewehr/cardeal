//! Desfatura a ordem de serviço: reverte o título/lançamento gerados no financeiro e volta a
//! `Concluida` — o oposto de [`crate::comandos::FaturarOrdemServico`], para corrigir uma OS
//! faturada com valor/serviço errado sem repetir o trâmite técnico inteiro.
//!
//! `docs/modulos/os.md` §5 e §7. Reaproveita `mod_financeiro::estornar_baixa_comum` (reverte
//! qualquer baixa já dada — seja `pago_no_ato` no faturamento, seja uma baixa manual dada
//! depois pela tela de Financeiro) e `Razao::estornar` (reverte o lançamento combinado
//! receita+CMV) — mesma decisão de arquitetura documentada em `faturar_ordem_servico.rs`: a
//! OS chama `mod_financeiro` direto, na mesma transação, em vez de ir pelo despacho de
//! `Comando` (`docs/contratos-internos.md` §7 regra 2).
//!
//! Nunca apaga título/parcela/lançamento — só cancela/estorna, preservando o rastro contábil
//! (mesma convenção de `EstornarBaixa`/`RenegociarTitulo`). Não mexe em estoque: a peça já foi
//! baixada do estoque físico em `AplicarPeca`, no momento da aplicação — o faturamento (e
//! portanto o desfaturamento) nunca tocou nisso.

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_ledger::{Razao, RepositorioRazao};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use mod_financeiro::{estornar_baixa_comum, titulo_da_origem, RepositorioFinanceiro};
use serde::{Deserialize, Serialize};

use crate::comandos::carregar_ordem;
use crate::eventos::OrdemDesfaturada;
use crate::repositorio::RepositorioOs;

/// Desfatura a ordem de serviço.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DesfaturarOrdemServico {
    /// A ordem de serviço.
    pub ordem_servico: Id,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct OrdemServicoDesfaturada {
    /// O título que foi cancelado, se havia um (uma OS de cortesia não gera título).
    pub titulo: Option<Id>,
    /// Quantas baixas foram estornadas antes de cancelar o título.
    pub baixas_estornadas: usize,
}

impl Comando for DesfaturarOrdemServico {
    type Saida = OrdemServicoDesfaturada;
    const PERMISSAO: &'static str = "os.desfaturar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut os = carregar_ordem(uow, self.ordem_servico)?;
        let titulo = titulo_da_origem(uow.conexao(), ctx.empresa, "os", os.id)?
            .filter(|t| !t.esta_cancelado());

        // 2. Validar (domínio puro): transiciona Faturada -> Concluida.
        os.desfaturar().map_err(|e| Erro::de_dominio(&e))?;

        // 3. Reverter o financeiro, se houve cobrança.
        let mut baixas_estornadas = 0usize;
        let titulo_id = if let Some(mut titulo) = titulo {
            let parcelas = RepositorioFinanceiro::novo(uow).parcelas_do_titulo(titulo.id)?;

            for parcela in &parcelas {
                for baixa in
                    RepositorioFinanceiro::novo(uow).baixas_nao_estornadas_da_parcela(parcela.id)?
                {
                    estornar_baixa_comum(baixa, "Desfaturamento da ordem de serviço", ctx, uow)?;
                    baixas_estornadas += 1;
                }
            }

            // Reverte o lançamento combinado (receita + CMV) — um só lançamento cobre todas
            // as parcelas dessa OS (`FaturarOrdemServico` nunca cria um por parcela, ver o
            // doc daquele comando), então basta reverter o da primeira.
            if let Some(lancamento) = parcelas.first().and_then(|p| p.lancamento) {
                let mut repo = RepositorioRazao::novo(uow);
                Razao::estornar(
                    &mut repo,
                    lancamento,
                    "Desfaturamento da ordem de serviço",
                    ctx.hoje(),
                )
                .map_err(|e| Erro::de_dominio(&e))?;
            }

            // Cancela as parcelas e o título — nunca apaga a linha.
            let mut repo = RepositorioFinanceiro::novo(uow);
            for mut parcela in parcelas {
                parcela.cancelar();
                repo.atualizar_parcela(&parcela)?;
            }
            titulo.cancelar(ctx.agora);
            repo.atualizar_titulo_cancelado(&titulo)?;

            Some(titulo.id)
        } else {
            None
        };

        // 4. Persistir.
        RepositorioOs::novo(uow).atualizar_ordem(&os)?;

        // 5. Publicar.
        uow.publicar(OrdemDesfaturada {
            ordem_servico: os.id,
            titulo: titulo_id,
        })
        .map_err(|e| Erro::de_dominio(&e))?;

        Ok(OrdemServicoDesfaturada {
            titulo: titulo_id,
            baixas_estornadas,
        })
    }
}
