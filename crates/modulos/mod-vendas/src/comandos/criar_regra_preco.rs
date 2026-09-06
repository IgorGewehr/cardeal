//! Cria uma regra de preço dentro de uma tabela. `docs/modulos/vendas.md` §5.

use cardeal_kernel::{Data, Erro, Id, Preco, Quantidade, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::erros::ErroVendas;
use crate::preco::{AlvoRegra, RegraPreco};
use crate::repositorio::RepositorioVendas;

/// Cria uma regra de preço. `alvo_produto` **xor** `alvo_grupo` deve vir preenchido.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriarRegraPreco {
    /// A tabela dona.
    pub tabela_preco: Id,
    /// O produto alvo, se a regra for por produto.
    pub alvo_produto: Option<Id>,
    /// O grupo alvo, se a regra for por grupo.
    pub alvo_grupo: Option<Id>,
    /// Quantidade mínima para a regra valer.
    pub quantidade_minima: Option<Quantidade>,
    /// O preço unitário.
    pub preco: Preco,
    /// Início do período (só em tabela `Promocional`).
    pub periodo_de: Option<Data>,
    /// Fim do período (só em tabela `Promocional`).
    pub periodo_ate: Option<Data>,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct RegraPrecoCriada {
    /// A regra criada.
    pub regra_preco: Id,
}

impl Comando for CriarRegraPreco {
    type Saida = RegraPrecoCriada;
    const PERMISSAO: &'static str = "vendas.tabela_preco.editar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let alvo = match (self.alvo_produto, self.alvo_grupo) {
            (Some(p), None) => AlvoRegra::Produto(p),
            (None, Some(g)) => AlvoRegra::Grupo(g),
            _ => return Err(Erro::de_dominio(&ErroVendas::RegraPrecoAmbigua)),
        };
        let regra = RegraPreco {
            id: Id::novo(),
            empresa: ctx.empresa,
            tabela_preco: self.tabela_preco,
            alvo,
            quantidade_minima: self.quantidade_minima,
            preco: self.preco,
            periodo_de: self.periodo_de,
            periodo_ate: self.periodo_ate,
        };
        RepositorioVendas::novo(uow).inserir_regra_preco(&regra)?;
        Ok(RegraPrecoCriada {
            regra_preco: regra.id,
        })
    }
}
