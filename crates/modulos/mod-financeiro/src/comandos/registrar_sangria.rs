//! Registra uma sangria (retirada de dinheiro do caixa para o cofre/banco) numa sessão
//! aberta.
//!
//! Receituário (`docs/modulos/financeiro.md` §7): D Bancos-Cofre / C Caixa. O saldo que a
//! sangria não pode ultrapassar (salvo `Caixa::permite_negativo`) vem do saldo **realizado**
//! da própria conta do caixa no Razão — não é um contador à parte.

use cardeal_kernel::{Dinheiro, Erro, Id, Resultado};
use cardeal_ledger::{Razao, RepositorioRazao};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::{autoria_de, carregar_sessao_e_caixa, contas_caixa_rotina};
use crate::eventos::SangriaRegistrada;
use crate::repositorio::RepositorioFinanceiro;

/// Registra uma sangria numa sessão de caixa.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrarSangria {
    /// A sessão de caixa aberta.
    pub sessao: Id,
    /// O valor retirado.
    pub valor: Dinheiro,
    /// Por que saiu — sempre obrigatório numa sangria.
    pub motivo: String,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct SangriaFoiRegistrada {
    /// O movimento criado.
    pub movimento: Id,
    /// O lançamento `Realizado` gerado.
    pub lancamento: Id,
}

impl Comando for RegistrarSangria {
    type Saida = SangriaFoiRegistrada;
    const PERMISSAO: &'static str = "financeiro.caixa.sangria";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let (sessao, caixa) = carregar_sessao_e_caixa(uow, self.sessao)?;
        let contas = contas_caixa_rotina(uow, ctx, &caixa)?;
        let saldo_atual = RepositorioRazao::novo(uow)
            .saldo_realizado(caixa.conta_razao)
            .map_err(|e| Erro::de_dominio(&e))?;

        let (mut movimento, lanc) = sessao
            .registrar_sangria(
                self.valor,
                saldo_atual,
                caixa.permite_negativo,
                self.motivo.clone(),
                contas,
                autoria_de(ctx),
            )
            .map_err(|e| Erro::de_dominio(&e))?;

        let lancamento = {
            let mut repo = RepositorioRazao::novo(uow);
            Razao::registrar(&mut repo, lanc).map_err(|e| Erro::de_dominio(&e))?
        };
        movimento.lancamento = Some(lancamento);
        RepositorioFinanceiro::novo(uow).inserir_movimento(&movimento)?;

        uow.publicar(SangriaRegistrada {
            sessao: sessao.id,
            valor: self.valor,
            motivo: self.motivo,
        })
        .map_err(|e| Erro::de_dominio(&e))?;

        Ok(SangriaFoiRegistrada {
            movimento: movimento.id,
            lancamento,
        })
    }
}
