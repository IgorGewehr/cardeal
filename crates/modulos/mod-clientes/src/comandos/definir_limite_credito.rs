//! Define ou ajusta o limite de crédito de um cliente.
//!
//! `docs/modulos/clientes.md` §5. Cria o [`LimiteCredito`] na primeira vez; ajusta o valor
//! nas seguintes — nunca mexe na `situacao` (bloqueio/liberação são comandos à parte).

use cardeal_kernel::{Dinheiro, Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::credito::LimiteCredito;
use crate::repositorio::RepositorioClientes;

/// Define ou ajusta o limite de crédito de um cliente.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DefinirLimiteCredito {
    /// A pessoa (papel `Cliente`).
    pub pessoa: Id,
    /// O novo limite.
    pub limite: Dinheiro,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct LimiteCreditoDefinido {
    /// A pessoa.
    pub pessoa: Id,
    /// O limite vigente após a operação.
    pub limite: Dinheiro,
}

impl Comando for DefinirLimiteCredito {
    type Saida = LimiteCreditoDefinido;
    const PERMISSAO: &'static str = "clientes.credito.definir_limite";
    const RISCO: Risco = Risco::Medio;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let existente = RepositorioClientes::novo(uow).buscar_limite_credito(self.pessoa)?;

        // 2. Validar + 4. Persistir.
        let mut repo = RepositorioClientes::novo(uow);
        if let Some(mut lc) = existente {
            lc.ajustar(self.limite, ctx.agora)
                .map_err(|e| Erro::de_dominio(&e))?;
            repo.atualizar_limite_credito(&lc)?;
        } else {
            let lc = LimiteCredito::definir(self.pessoa, self.limite, ctx.agora)
                .map_err(|e| Erro::de_dominio(&e))?;
            repo.inserir_limite_credito(ctx.empresa, &lc)?;
        }

        Ok(LimiteCreditoDefinido {
            pessoa: self.pessoa,
            limite: self.limite,
        })
    }
}
