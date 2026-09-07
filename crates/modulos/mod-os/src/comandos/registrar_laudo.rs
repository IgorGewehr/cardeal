//! Registra o laudo técnico de uma ordem de serviço: `Aberta` → `EmDiagnostico`.

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::carregar_ordem;
use crate::laudo::LaudoTecnico;
use crate::repositorio::RepositorioOs;

/// Registra o laudo técnico.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrarLaudo {
    /// A ordem de serviço.
    pub ordem_servico: Id,
    /// O problema relatado.
    pub descricao_problema: String,
    /// O diagnóstico, quando já concluído.
    pub diagnostico: Option<String>,
    /// O técnico responsável pelo laudo.
    pub tecnico: Id,
}

impl Comando for RegistrarLaudo {
    type Saida = Id;
    const PERMISSAO: &'static str = "os.laudo.registrar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut os = carregar_ordem(uow, self.ordem_servico)?;

        // 2. Validar (domínio puro).
        os.registrar_laudo().map_err(|e| Erro::de_dominio(&e))?;
        let laudo = LaudoTecnico::novo(
            os.id,
            self.descricao_problema,
            self.diagnostico,
            self.tecnico,
            ctx.agora,
        )
        .map_err(|e| Erro::de_dominio(&e))?;

        // 4. Persistir.
        let mut repo = RepositorioOs::novo(uow);
        repo.inserir_laudo(&laudo)?;
        repo.atualizar_ordem(&os)?;

        Ok(laudo.id)
    }
}
