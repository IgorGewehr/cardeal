//! Registra a aprovação do orçamento pelo cliente: `AguardandoAprovacao` → `Aprovada`.
//!
//! `docs/modulos/os.md` §11.2: a aprovação é sempre identificada, nunca implícita.

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::carregar_ordem;
use crate::eventos::OrcamentoAprovado;
use crate::repositorio::RepositorioOs;

/// Registra a aprovação do orçamento.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AprovarOrcamentoOs {
    /// A ordem de serviço.
    pub ordem_servico: Id,
    /// Nome (e documento, quando houver) de quem aprovou.
    pub identificacao_aprovador: String,
}

impl Comando for AprovarOrcamentoOs {
    type Saida = ();
    const PERMISSAO: &'static str = "os.orcamento.aprovar";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let mut os = carregar_ordem(uow, self.ordem_servico)?;
        os.aprovar(self.identificacao_aprovador)
            .map_err(|e| Erro::de_dominio(&e))?;
        RepositorioOs::novo(uow).atualizar_ordem(&os)?;
        uow.publicar(OrcamentoAprovado {
            ordem_servico: os.id,
            valor_total: os.valor_total,
        })
        .map_err(|e| Erro::de_dominio(&e))?;
        Ok(())
    }
}
