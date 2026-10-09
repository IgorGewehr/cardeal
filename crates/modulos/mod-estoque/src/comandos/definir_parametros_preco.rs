//! Define os parâmetros de formação de preço da empresa: regime, custo operacional,
//! comissão, taxa de pagamento e margens. Só quem edita o perfil tributário.

#[cfg(feature = "sqlite")]
use cardeal_kernel::{CodigoErro, Erro, Resultado};
use cardeal_modkit::Comando;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::parametros_preco::ParametrosPreco;
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioEstoque;
#[cfg(feature = "sqlite")]
use crate::repositorio_preco as sql;

/// Grava os parâmetros de preço da empresa (CNPJ) da sessão.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DefinirParametrosPreco {
    /// Os parâmetros.
    pub parametros: ParametrosPreco,
}

impl Comando for DefinirParametrosPreco {
    type Saida = ();
    const PERMISSAO: &'static str = "estoque.tributario.editar";
    const RISCO: Risco = Risco::Alto;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        self.parametros
            .validar()
            .map_err(|m| Erro::novo(CodigoErro::FORA_DA_FAIXA, m))?;
        let repo = RepositorioEstoque::novo(uow);
        sql::salvar_parametros_preco(repo.conn(), ctx.empresa, &self.parametros)
    }
}
