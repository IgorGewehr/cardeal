//! Abre uma sessão de caixa — cria a [`SessaoCaixa`] e, se houver suprimento inicial, o
//! lançamento e o [`MovimentoCaixa`] correspondentes.
//!
//! Receituário (`docs/modulos/financeiro.md` §7): D Caixa / C Bancos-Cofre, só quando
//! `valor_abertura > 0`.

use cardeal_kernel::{Dinheiro, Erro, Id, Resultado};
use cardeal_ledger::{Razao, RepositorioRazao};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::caixa::{MovimentoCaixa, SessaoCaixa, TipoMovimento};
use crate::comandos::{autoria_de, contas_caixa_rotina};
use crate::erros::ErroFinanceiro;
use crate::eventos::CaixaAberto;
use crate::repositorio::RepositorioFinanceiro;

/// Abre uma sessão para um caixa físico.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbrirCaixa {
    /// O caixa físico a abrir.
    pub caixa: Id,
    /// O suprimento inicial — pode ser zero.
    pub valor_abertura: Dinheiro,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct CaixaFoiAberto {
    /// A sessão criada.
    pub sessao: Id,
    /// O lançamento `Realizado` do suprimento inicial, se houve.
    pub lancamento_suprimento: Option<Id>,
}

impl Comando for AbrirCaixa {
    type Saida = CaixaFoiAberto;
    const PERMISSAO: &'static str = "financeiro.caixa.abrir";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let caixa = RepositorioFinanceiro::novo(uow)
            .buscar_caixa(self.caixa)?
            .ok_or_else(|| Erro::nao_encontrado("caixa"))?;
        if !caixa.ativo {
            return Err(Erro::de_dominio(&ErroFinanceiro::CaixaInativo));
        }
        if RepositorioFinanceiro::novo(uow)
            .sessao_aberta_do_caixa(caixa.id)?
            .is_some()
        {
            return Err(Erro::de_dominio(&ErroFinanceiro::CaixaJaAberto));
        }

        let contas = contas_caixa_rotina(uow, ctx, &caixa)?;
        let a = SessaoCaixa::abrir(
            &caixa,
            ctx.usuario,
            ctx.dispositivo,
            self.valor_abertura,
            contas,
            autoria_de(ctx),
        );

        // A sessão precisa existir primeiro: `financeiro_movimento_caixa.sessao` referencia
        // `financeiro_sessao_caixa(id)`.
        RepositorioFinanceiro::novo(uow).inserir_sessao(&a.sessao)?;

        let lancamento_suprimento = match a.lancamento_suprimento {
            Some(lanc) => {
                let id = {
                    let mut repo = RepositorioRazao::novo(uow);
                    Razao::registrar(&mut repo, lanc).map_err(|e| Erro::de_dominio(&e))?
                };
                let movimento = MovimentoCaixa {
                    id: Id::novo(),
                    empresa: ctx.empresa,
                    sessao: a.sessao.id,
                    tipo: TipoMovimento::Suprimento,
                    valor: self.valor_abertura,
                    forma_pagamento: None,
                    lancamento: Some(id),
                    motivo: None,
                    criado_em: ctx.agora,
                    criado_por: ctx.usuario,
                };
                RepositorioFinanceiro::novo(uow).inserir_movimento(&movimento)?;
                Some(id)
            }
            None => None,
        };

        uow.publicar(CaixaAberto {
            sessao: a.sessao.id,
            caixa: caixa.id,
            operador: ctx.usuario,
            valor_abertura: self.valor_abertura,
        })
        .map_err(|e| Erro::de_dominio(&e))?;

        Ok(CaixaFoiAberto {
            sessao: a.sessao.id,
            lancamento_suprimento,
        })
    }
}
