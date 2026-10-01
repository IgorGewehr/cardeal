//! Define uma janela recorrente de disponibilidade para um recurso.

use cardeal_kernel::Id;
#[cfg(feature = "sqlite")]
use cardeal_kernel::{Erro, Resultado};
use cardeal_modkit::Comando;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::recurso::DisponibilidadeRecurso;
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioAgenda;

/// Define uma regra de disponibilidade para um recurso.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DefinirDisponibilidade {
    /// O recurso.
    pub recurso: Id,
    /// Dia da semana: 0 (domingo) a 6 (sábado).
    pub dia_semana: u8,
    /// Minuto de início do dia (0..1440).
    pub hora_inicio: u16,
    /// Minuto de fim do dia (0..1440).
    pub hora_fim: u16,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct DisponibilidadeDefinida {
    /// A regra criada.
    pub disponibilidade: Id,
}

impl Comando for DefinirDisponibilidade {
    type Saida = DisponibilidadeDefinida;
    const PERMISSAO: &'static str = "agenda.recurso.gerenciar";
    const RISCO: Risco = Risco::Baixo;

    #[cfg(feature = "sqlite")]
    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar: o recurso precisa existir.
        RepositorioAgenda::novo(uow)
            .buscar_recurso(self.recurso)?
            .ok_or_else(|| Erro::nao_encontrado("recurso"))?;

        // 2. Validar (domínio puro).
        let disponibilidade = DisponibilidadeRecurso::nova(
            self.recurso,
            self.dia_semana,
            self.hora_inicio,
            self.hora_fim,
        )
        .map_err(|e| Erro::de_dominio(&e))?;

        // 3. Persistir.
        RepositorioAgenda::novo(uow).inserir_disponibilidade(&disponibilidade)?;

        Ok(DisponibilidadeDefinida {
            disponibilidade: disponibilidade.id,
        })
    }
}
