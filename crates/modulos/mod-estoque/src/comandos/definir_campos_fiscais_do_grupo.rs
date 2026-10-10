//! Define o perfil fiscal próprio do grupo (origem, CST/CSOSN, CFOPs, alíquotas, MVA-ST, CEST, benefício). Campo em branco herda do grupo acima; produtos e subgrupos herdam o que o grupo define.

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
pub struct DefinirCamposFiscaisDoGrupo {
    /// O grupo.
    pub grupo: Id,
    /// Os campos; todos em branco = voltar a herdar tudo.
    pub campos: CamposFiscais,
}

impl Comando for DefinirCamposFiscaisDoGrupo {
    type Saida = ();
    const PERMISSAO: &'static str = "estoque.tributario.editar";
    const RISCO: Risco = Risco::Alto;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        self.campos
            .validar()
            .map_err(|m| Erro::novo(CodigoErro::ENTRADA_INVALIDA, m))?;
        let repo = RepositorioEstoque::novo(uow);
        if !sql::definir_campos_fiscais_do_grupo(
            repo.conn(),
            ctx.organizacao,
            self.grupo,
            &self.campos,
        )? {
            return Err(Erro::nao_encontrado("grupo de produto"));
        }
        Ok(())
    }
}
