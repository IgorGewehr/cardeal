//! Fecha uma sessão de caixa pelo **fechamento cego** (`docs/modulos/financeiro.md` §11.3):
//! o operador informa `valor_contado` sem ver `valor_esperado` antes — o comando é quem
//! calcula o esperado (saldo realizado da conta do caixa no Razão) e só o revela na saída.
//!
//! Receituário: falta → D Quebra de caixa / C Caixa; sobra → D Caixa / C Outras receitas.

use cardeal_kernel::{Dinheiro, Erro, Id, Resultado};
use cardeal_ledger::{Contas, PapelConta, Razao, RepositorioRazao};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::caixa::{ContasCaixa, TOLERANCIA_QUEBRA};
use crate::comandos::{autoria_de, carregar_sessao_e_caixa};
use crate::eventos::CaixaFechado;
use crate::repositorio::RepositorioFinanceiro;

/// Fecha uma sessão de caixa.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FecharCaixa {
    /// A sessão a fechar.
    pub sessao: Id,
    /// O valor que o operador contou — informado **antes** de ver o esperado.
    pub valor_contado: Dinheiro,
    /// O motivo da quebra, obrigatório só se ela passar da tolerância.
    pub motivo: Option<String>,
}

/// O que o comando devolve — aqui, finalmente, `valor_esperado` fica visível.
#[derive(Debug, Serialize, Deserialize)]
pub struct CaixaFoiFechado {
    /// A sessão, agora em `Fechada`.
    pub sessao: Id,
    /// O saldo que o sistema esperava.
    pub valor_esperado: Dinheiro,
    /// `valor_contado - valor_esperado`; negativo = falta.
    pub quebra: Dinheiro,
    /// O lançamento de ajuste da quebra, se ela não foi zero.
    pub lancamento_ajuste: Option<Id>,
}

impl Comando for FecharCaixa {
    type Saida = CaixaFoiFechado;
    const PERMISSAO: &'static str = "financeiro.caixa.fechar";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let (sessao, caixa) = carregar_sessao_e_caixa(uow, self.sessao)?;

        let valor_esperado = RepositorioRazao::novo(uow)
            .saldo_realizado(caixa.conta_razao)
            .map_err(|e| Erro::de_dominio(&e))?;

        let contas = {
            let repo = RepositorioRazao::novo(uow);
            let resolvedor = Contas::nova(&repo, ctx.empresa);
            ContasCaixa {
                caixa: caixa.conta_razao,
                contrapartida: Id::NULO,
                quebra: resolvedor
                    .papel(PapelConta::QuebraCaixa)
                    .map_err(|e| Erro::de_dominio(&e))?,
                sobra: resolvedor
                    .papel(PapelConta::OutrasReceitas)
                    .map_err(|e| Erro::de_dominio(&e))?,
            }
        };

        let f = sessao
            .fechar(
                self.valor_contado,
                valor_esperado,
                TOLERANCIA_QUEBRA,
                self.motivo,
                contas,
                autoria_de(ctx),
            )
            .map_err(|e| Erro::de_dominio(&e))?;

        let lancamento_ajuste = match f.ajuste {
            Some((mut movimento, lanc)) => {
                let id = {
                    let mut repo = RepositorioRazao::novo(uow);
                    Razao::registrar(&mut repo, lanc).map_err(|e| Erro::de_dominio(&e))?
                };
                movimento.lancamento = Some(id);
                RepositorioFinanceiro::novo(uow).inserir_movimento(&movimento)?;
                Some(id)
            }
            None => None,
        };

        RepositorioFinanceiro::novo(uow).atualizar_sessao(&f.sessao)?;

        uow.publicar(CaixaFechado {
            sessao: f.sessao.id,
            valor_esperado,
            valor_contado: self.valor_contado,
            quebra: f.quebra,
        })
        .map_err(|e| Erro::de_dominio(&e))?;

        Ok(CaixaFoiFechado {
            sessao: f.sessao.id,
            valor_esperado,
            quebra: f.quebra,
            lancamento_ajuste,
        })
    }
}
