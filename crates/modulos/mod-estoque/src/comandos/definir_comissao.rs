//! Define a comissão de vendedor própria de um grupo da estrutura mercadológica ou de um
//! produto. Como a margem, ela é herdada: produto → grupo mais próximo acima → comissão padrão
//! da empresa (`ParametrosPreco::comissao`). `None` volta a herdar.

#[cfg(feature = "sqlite")]
use cardeal_kernel::{CodigoErro, Erro, Resultado};
use cardeal_kernel::{Id, Percentual};
use cardeal_modkit::Comando;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioEstoque;
#[cfg(feature = "sqlite")]
use crate::repositorio_preco as sql;

/// Define a comissão do grupo (vale para os subgrupos e produtos que não tenham a sua).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DefinirComissaoDoGrupo {
    /// O grupo.
    pub grupo: Id,
    /// A comissão; `None` limpa.
    pub comissao: Option<Percentual>,
}

impl Comando for DefinirComissaoDoGrupo {
    type Saida = ();
    const PERMISSAO: &'static str = "estoque.produto.editar";
    const RISCO: Risco = Risco::Medio;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        validar_comissao(self.comissao)?;
        let repo = RepositorioEstoque::novo(uow);
        if !sql::definir_comissao_do_grupo(repo.conn(), ctx.organizacao, self.grupo, self.comissao)?
        {
            return Err(Erro::nao_encontrado("grupo de produto"));
        }
        Ok(())
    }
}

/// Define a comissão própria do produto, que vale mais que a do grupo.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DefinirComissaoDoProduto {
    /// O produto.
    pub produto: Id,
    /// A comissão; `None` limpa.
    pub comissao: Option<Percentual>,
}

impl Comando for DefinirComissaoDoProduto {
    type Saida = ();
    const PERMISSAO: &'static str = "estoque.produto.editar";
    const RISCO: Risco = Risco::Medio;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        validar_comissao(self.comissao)?;
        let repo = RepositorioEstoque::novo(uow);
        if !sql::definir_comissao_do_produto(
            repo.conn(),
            ctx.organizacao,
            self.produto,
            self.comissao,
        )? {
            return Err(Erro::nao_encontrado("produto"));
        }
        Ok(())
    }
}

#[cfg(feature = "sqlite")]
pub(crate) fn validar_comissao(c: Option<Percentual>) -> Resultado<()> {
    match c {
        Some(p) if !crate::precificacao::comissao_valida(p) => Err(Erro::novo(
            CodigoErro::FORA_DA_FAIXA,
            "a comissão precisa estar entre 0% e 100%",
        )),
        _ => Ok(()),
    }
}
