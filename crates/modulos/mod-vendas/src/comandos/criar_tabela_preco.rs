//! Cria uma tabela de preço. `docs/modulos/vendas.md` §5.

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
use cardeal_kernel::{Data, Id};
use cardeal_modkit::Comando;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::preco::TabelaPreco;
use crate::preco::TipoTabela;
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioVendas;

/// Cria uma tabela de preço.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriarTabelaPreco {
    /// O nome.
    pub nome: String,
    /// O tipo.
    pub tipo: TipoTabela,
    /// Início da vigência.
    pub vigente_de: Data,
    /// Fim da vigência (aberta se `None`).
    pub vigente_ate: Option<Data>,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct TabelaPrecoCriada {
    /// A tabela criada.
    pub tabela_preco: Id,
}

impl Comando for CriarTabelaPreco {
    type Saida = TabelaPrecoCriada;
    const PERMISSAO: &'static str = "vendas.tabela_preco.criar";
    const RISCO: Risco = Risco::Baixo;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let tabela = TabelaPreco {
            id: Id::novo(),
            empresa: ctx.empresa,
            nome: self.nome,
            tipo: self.tipo,
            vigente_de: self.vigente_de,
            vigente_ate: self.vigente_ate,
            ativa: true,
            versao: cardeal_kernel::Versao::INICIAL,
        };
        RepositorioVendas::novo(uow).inserir_tabela_preco(&tabela)?;
        Ok(TabelaPrecoCriada {
            tabela_preco: tabela.id,
        })
    }
}
