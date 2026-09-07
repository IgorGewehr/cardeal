//! Cria uma categoria financeira — o rótulo livre de custo/receita usado para agrupar
//! títulos em relatórios (`docs/modulos/financeiro.md` §11.9), fora do plano de contas.

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::categoria::CategoriaFinanceira;
use crate::repositorio::RepositorioFinanceiro;
use crate::titulo::EspecieTitulo;

/// Cria uma categoria financeira nova.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriarCategoria {
    /// O nome livre: "Aluguel", "Assinatura `SaaS` — Cliente X".
    pub nome: String,
    /// Restringe a categoria a uma espécie; `None` serve para custo e receita.
    pub especie: Option<EspecieTitulo>,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct CategoriaCriada {
    /// A categoria criada.
    pub categoria: Id,
}

impl Comando for CriarCategoria {
    type Saida = CategoriaCriada;
    const PERMISSAO: &'static str = "financeiro.categoria.criar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Validar (domínio puro): nome não vazio.
        let categoria = CategoriaFinanceira::nova(ctx.empresa, self.nome, self.especie)
            .map_err(|e| Erro::de_dominio(&e))?;

        // 2. Persistir.
        RepositorioFinanceiro::novo(uow).inserir_categoria(&categoria)?;

        Ok(CategoriaCriada {
            categoria: categoria.id,
        })
    }
}
