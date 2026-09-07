//! Registra um suprimento (entrada de fundo de troco) numa sessão de caixa aberta.
//!
//! Receituário (`docs/modulos/financeiro.md` §7): D Caixa / C Bancos-Cofre.

use cardeal_kernel::{Dinheiro, Erro, Id, Resultado};
use cardeal_ledger::{Razao, RepositorioRazao};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::{autoria_de, carregar_sessao_e_caixa, contas_caixa_rotina};
use crate::repositorio::RepositorioFinanceiro;

/// Registra um suprimento numa sessão de caixa.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrarSuprimento {
    /// A sessão de caixa aberta.
    pub sessao: Id,
    /// O valor recebido.
    pub valor: Dinheiro,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct SuprimentoFoiRegistrado {
    /// O movimento criado.
    pub movimento: Id,
    /// O lançamento `Realizado` gerado.
    pub lancamento: Id,
}

impl Comando for RegistrarSuprimento {
    type Saida = SuprimentoFoiRegistrado;
    const PERMISSAO: &'static str = "financeiro.caixa.suprimento";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let (sessao, caixa) = carregar_sessao_e_caixa(uow, self.sessao)?;
        let contas = contas_caixa_rotina(uow, ctx, &caixa)?;

        let (mut movimento, lanc) = sessao
            .registrar_suprimento(self.valor, contas, autoria_de(ctx))
            .map_err(|e| Erro::de_dominio(&e))?;

        let lancamento = {
            let mut repo = RepositorioRazao::novo(uow);
            Razao::registrar(&mut repo, lanc).map_err(|e| Erro::de_dominio(&e))?
        };
        movimento.lancamento = Some(lancamento);
        RepositorioFinanceiro::novo(uow).inserir_movimento(&movimento)?;

        Ok(SuprimentoFoiRegistrado {
            movimento: movimento.id,
            lancamento,
        })
    }
}
