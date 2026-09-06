//! Cadastra um recurso — sala, técnico, equipamento ou pessoa.

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::recurso::{Recurso, TipoRecurso};
use crate::repositorio::RepositorioAgenda;

/// Cadastra um recurso novo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriarRecurso {
    /// O nome: "Sala 1", "Carlos".
    pub nome: String,
    /// O tipo.
    pub tipo: TipoRecurso,
    /// Quantas reservas simultâneas o recurso aceita — `None` = uma por vez.
    pub capacidade: Option<u32>,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct RecursoCriado {
    /// O recurso criado.
    pub recurso: Id,
}

impl Comando for CriarRecurso {
    type Saida = RecursoCriado;
    const PERMISSAO: &'static str = "agenda.recurso.gerenciar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Validar (domínio puro + checagem de duplicidade).
        if RepositorioAgenda::novo(uow)
            .recurso_ativo_por_nome(ctx.empresa, &self.nome)?
            .is_some()
        {
            return Err(Erro::de_dominio(&crate::erros::ErroAgenda::NomeDuplicado));
        }
        let recurso = Recurso::novo(ctx.empresa, self.nome, self.tipo, self.capacidade)
            .map_err(|e| Erro::de_dominio(&e))?;

        // 2. Persistir.
        RepositorioAgenda::novo(uow).inserir_recurso(&recurso)?;

        Ok(RecursoCriado {
            recurso: recurso.id,
        })
    }
}
