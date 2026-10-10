//! Define os campos fiscais próprios do produto, que valem mais que os do grupo. Em branco, herda.

use cardeal_kernel::Id;
#[cfg(feature = "sqlite")]
use cardeal_kernel::{CodigoErro, Erro, Resultado};
use cardeal_modkit::Comando;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::fiscal::CamposFiscais;
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioEstoque;
#[cfg(feature = "sqlite")]
use crate::repositorio_preco as sql;

/// Grava os campos fiscais próprios (substitui o que havia).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefinirCamposFiscaisDoProduto {
    /// O produto.
    pub produto: Id,
    /// Os campos; todos em branco = voltar a herdar tudo.
    pub campos: CamposFiscais,
}

impl Comando for DefinirCamposFiscaisDoProduto {
    type Saida = ();
    const PERMISSAO: &'static str = "estoque.tributario.editar";
    const RISCO: Risco = Risco::Alto;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        self.campos
            .validar()
            .map_err(|m| Erro::novo(CodigoErro::ENTRADA_INVALIDA, m))?;
        let repo = RepositorioEstoque::novo(uow);
        if !sql::definir_campos_fiscais_do_produto(
            repo.conn(),
            ctx.organizacao,
            self.produto,
            &self.campos,
        )? {
            return Err(Erro::nao_encontrado("produto"));
        }
        Ok(())
    }
}
