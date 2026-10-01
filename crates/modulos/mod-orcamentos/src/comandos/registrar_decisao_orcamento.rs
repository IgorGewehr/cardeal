//! `Enviado → Aprovado | Recusado` — registra a decisão do cliente (nunca implícita).

use cardeal_kernel::Id;
#[cfg(feature = "sqlite")]
use cardeal_kernel::{Erro, Resultado};
#[cfg(feature = "sqlite")]
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_modkit::{Comando, Ctx};
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::comandos::carregar_orcamento;
#[cfg(feature = "sqlite")]
use crate::eventos::OrcamentoAprovado;
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioOrcamentos;

/// Registra a aprovação ou recusa do cliente.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrarDecisaoOrcamento {
    /// O orçamento.
    pub orcamento: Id,
    /// `true` = aprovado, `false` = recusado.
    pub aprovado: bool,
    /// Nome/documento de quem aprovou — obrigatório para aprovar.
    pub identificacao: Option<String>,
}

#[cfg(feature = "sqlite")]
impl Comando for RegistrarDecisaoOrcamento {
    type Saida = ();
    const PERMISSAO: &'static str = "orcamentos.orcamento.decidir";
    const RISCO: Risco = Risco::Medio;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let mut orcamento = carregar_orcamento(uow, self.orcamento)?;

        orcamento
            .registrar_decisao(self.aprovado, self.identificacao, ctx.hoje())
            .map_err(|e| Erro::de_dominio(&e))?;

        let itens = RepositorioOrcamentos::novo(uow).itens_do_orcamento(orcamento.id)?;
        let total = orcamento.total(&itens);

        RepositorioOrcamentos::novo(uow).atualizar_orcamento(&orcamento)?;

        if self.aprovado {
            uow.publicar(OrcamentoAprovado {
                orcamento: orcamento.id,
                cliente: orcamento.cliente,
                total,
            })
            .map_err(|e| Erro::de_dominio(&e))?;
        }
        Ok(())
    }
}
