//! Cadastra uma unidade de medida ("UN", "KG", "CX"…).

use cardeal_kernel::{CodigoErro, Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::produto::Unidade;
use crate::repositorio::RepositorioEstoque;

/// Cadastra uma unidade de medida.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriarUnidade {
    /// Sigla, única na empresa ("UN", "KG").
    pub sigla: String,
    /// Nome por extenso.
    pub nome: String,
    /// Se aceita fração (ex.: KG aceita 0,450; UN normalmente não).
    pub fracionavel: bool,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct UnidadeCriada {
    /// A unidade criada.
    pub unidade: Id,
}

impl Comando for CriarUnidade {
    type Saida = UnidadeCriada;
    const PERMISSAO: &'static str = "estoque.produto.criar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let sigla = self.sigla.trim();
        let nome = self.nome.trim();
        if sigla.is_empty() || nome.is_empty() {
            return Err(Erro::novo(
                CodigoErro::CAMPO_OBRIGATORIO,
                "sigla e nome da unidade são obrigatórios",
            ));
        }
        let unidade = Unidade {
            id: Id::novo(),
            empresa: ctx.empresa,
            sigla: sigla.to_string(),
            nome: nome.to_string(),
            fracionavel: self.fracionavel,
        };
        RepositorioEstoque::novo(uow).inserir_unidade(&unidade)?;
        Ok(UnidadeCriada {
            unidade: unidade.id,
        })
    }
}
