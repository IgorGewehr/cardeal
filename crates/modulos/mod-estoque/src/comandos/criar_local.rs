//! Cadastra um local de estoque (loja, depósito, filial…).

use cardeal_kernel::{CodigoErro, Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::repositorio::RepositorioEstoque;

/// O tipo de um local de estoque.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TipoLocal {
    /// Ponto de venda.
    Loja,
    /// Depósito interno.
    Deposito,
    /// Outra filial.
    Filial,
    /// Tanque (posto de combustível).
    Tanque,
}

impl TipoLocal {
    const fn texto(self) -> &'static str {
        match self {
            Self::Loja => "Loja",
            Self::Deposito => "Deposito",
            Self::Filial => "Filial",
            Self::Tanque => "Tanque",
        }
    }
}

/// Cadastra um local de estoque.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriarLocal {
    /// O nome ("Loja", "Depósito").
    pub nome: String,
    /// O tipo.
    pub tipo: TipoLocal,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct LocalCriado {
    /// O local criado.
    pub local: Id,
}

impl Comando for CriarLocal {
    type Saida = LocalCriado;
    const PERMISSAO: &'static str = "estoque.local.criar";
    const RISCO: Risco = Risco::Medio;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let nome = self.nome.trim();
        if nome.is_empty() {
            return Err(Erro::novo(
                CodigoErro::CAMPO_OBRIGATORIO,
                "o nome do local é obrigatório",
            ));
        }
        let id = Id::novo();
        RepositorioEstoque::novo(uow).inserir_local(ctx.empresa, id, nome, self.tipo.texto())?;
        Ok(LocalCriado { local: id })
    }
}
