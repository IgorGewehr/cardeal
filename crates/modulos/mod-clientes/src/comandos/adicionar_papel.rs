//! Acrescenta um papel a uma pessoa já cadastrada.
//!
//! `docs/modulos/clientes.md` §5, §11.2: papéis coexistem na mesma pessoa — nunca dois
//! cadastros. Reativar um papel antes desativado, em vez de duplicar, é regra do domínio
//! ([`Pessoa::adicionar_papel`]).

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::eventos::PapelAdicionado;
use crate::pessoa::Papel;
use crate::repositorio::{self, RepositorioClientes};

/// Acrescenta um papel a uma pessoa.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AdicionarPapel {
    /// A pessoa.
    pub pessoa: Id,
    /// O papel a acrescentar.
    pub papel: Papel,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct PapelFoiAdicionado {
    /// A pessoa.
    pub pessoa: Id,
    /// O papel acrescentado.
    pub papel: Papel,
}

impl Comando for AdicionarPapel {
    type Saida = PapelFoiAdicionado;
    const PERMISSAO: &'static str = "clientes.papel.gerenciar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut pessoa = RepositorioClientes::novo(uow)
            .buscar_pessoa(self.pessoa)?
            .ok_or_else(|| Erro::nao_encontrado("pessoa"))?;

        // 2. Validar (domínio puro).
        pessoa
            .adicionar_papel(self.papel, ctx.hoje())
            .map_err(|e| Erro::de_dominio(&e))?;

        // 4. Persistir.
        let papel_pessoa = pessoa
            .papeis
            .iter()
            .find(|p| p.papel == self.papel)
            .copied()
            .expect("acabou de ser adicionado");
        {
            let mut repo = RepositorioClientes::novo(uow);
            repo.inserir_papel(pessoa.id, ctx.empresa, &papel_pessoa)?;
            repo.atualizar_pessoa(&pessoa)?;
        }

        // 5. Publicar.
        uow.publicar(PapelAdicionado {
            pessoa: pessoa.id,
            papel: repositorio::papel_txt(self.papel),
        })
        .map_err(|e| Erro::de_dominio(&e))?;

        Ok(PapelFoiAdicionado {
            pessoa: pessoa.id,
            papel: self.papel,
        })
    }
}
