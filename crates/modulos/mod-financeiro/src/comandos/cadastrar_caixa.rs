//! Cadastra um caixa físico — uma gaveta, um cofre, um terminal.
//!
//! Não está no `docs/modulos/financeiro.md` §5 original (a nota do doc já sinalizava a
//! decisão pendente); a decisão tomada aqui: o comando **não cria** a conta analítica do
//! caixa — recebe o [`Id`] de uma conta já existente no plano da empresa (Ativo, analítica,
//! ativa) e só vincula. Criar contas do plano em runtime é uma feature própria (edição do
//! plano de contas), fora do escopo deste comando; até ela existir, a conta é escolhida entre
//! as que já existem (ex.: `1.1.01` do plano padrão, ou uma sub-conta cadastrada à mão).

use cardeal_kernel::{Erro, Id, Resultado, Versao};
use cardeal_ledger::{PortaRazao, RepositorioRazao, TipoConta};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::caixa::Caixa;
use crate::erros::ErroFinanceiro;
use crate::repositorio::RepositorioFinanceiro;

/// Cadastra um caixa físico.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CadastrarCaixa {
    /// O nome exibido: "Caixa 1", "Cofre".
    pub nome: String,
    /// A filial/PDV a que pertence, quando aplicável.
    pub local_operacao: Option<Id>,
    /// A conta analítica do plano da empresa que representa este caixa.
    pub conta_razao: Id,
    /// Se o caixa pode ficar com saldo negativo. Padrão: não.
    pub permite_negativo: bool,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct CaixaCadastrado {
    /// O caixa criado.
    pub caixa: Id,
}

impl Comando for CadastrarCaixa {
    type Saida = CaixaCadastrado;
    const PERMISSAO: &'static str = "financeiro.caixa.cadastrar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let nome = self.nome.trim();
        if nome.is_empty() {
            return Err(Erro::de_dominio(&ErroFinanceiro::NomeDeCaixaVazio));
        }

        {
            let repo = RepositorioRazao::novo(uow);
            let info = repo
                .info_conta(self.conta_razao)
                .map_err(|e| Erro::de_dominio(&e))?
                .ok_or_else(|| Erro::nao_encontrado("conta"))?;
            if info.empresa != ctx.empresa || info.tipo != TipoConta::Analitica || !info.ativa {
                return Err(Erro::de_dominio(&ErroFinanceiro::ContaDeCaixaInvalida));
            }
        }

        let caixa = Caixa {
            id: Id::novo(),
            empresa: ctx.empresa,
            nome: nome.to_string(),
            local_operacao: self.local_operacao,
            conta_razao: self.conta_razao,
            permite_negativo: self.permite_negativo,
            ativo: true,
            versao: Versao::INICIAL,
        };
        RepositorioFinanceiro::novo(uow).inserir_caixa(&caixa)?;

        Ok(CaixaCadastrado { caixa: caixa.id })
    }
}
