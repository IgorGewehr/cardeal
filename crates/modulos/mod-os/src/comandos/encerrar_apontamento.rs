//! Encerra um apontamento de tempo em aberto — "parou o cronômetro".

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::repositorio::RepositorioOs;

/// Encerra um apontamento de tempo.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EncerrarApontamento {
    /// O apontamento a encerrar.
    pub apontamento: Id,
}

impl Comando for EncerrarApontamento {
    type Saida = i64;
    const PERMISSAO: &'static str = "os.apontamento.encerrar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let agora = uow.agora();
        let mut repo = RepositorioOs::novo(uow);
        let mut apontamento = repo
            .buscar_apontamento(self.apontamento)?
            .ok_or_else(|| Erro::nao_encontrado("apontamento de tempo"))?;

        // 2. Validar (domínio puro).
        apontamento
            .encerrar(agora)
            .map_err(|e| Erro::de_dominio(&e))?;

        // 3. Persistir.
        let duracao = apontamento.duracao_segundos(agora);
        repo.atualizar_apontamento(&apontamento)?;

        Ok(duracao)
    }
}

// O comportamento de guarda de valor (`ErroOs::ApontamentoJaEncerrado`) já é coberto no
// domínio puro (`crate::apontamento::testes`); a cobertura de ponta a ponta contra o
// `Despachante` real está em `tests/apontamento.rs`.
