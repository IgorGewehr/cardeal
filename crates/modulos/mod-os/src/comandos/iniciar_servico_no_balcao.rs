//! "Iniciar serviço" no balcão — pedido do usuário (2026-11-07): quase sempre o cliente aprova
//! o pré-orçamento ali mesmo, antes de sair; o orçamento formal é a exceção. Este comando faz,
//! numa transação só, o que antes eram três cliques e três telas: envia o orçamento, registra a
//! aprovação (de quem aprovou no balcão) e inicia a execução. Cada passo continua na linha do
//! tempo.

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
use crate::comandos::carregar_ordem;
#[cfg(feature = "sqlite")]
use crate::eventos::OrcamentoAprovado;
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioOs;

/// Aprova no balcão e inicia a execução de uma OS aberta/em diagnóstico com orçamento.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IniciarServicoNoBalcao {
    /// A ordem de serviço.
    pub ordem_servico: Id,
    /// Quem aprovou (o nome do cliente, por padrão).
    pub identificacao_aprovador: String,
}

impl Comando for IniciarServicoNoBalcao {
    type Saida = ();
    const PERMISSAO: &'static str = "os.orcamento.aprovar";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let mut os = carregar_ordem(uow, self.ordem_servico)?;
        os.enviar_para_aprovacao()
            .map_err(|e| Erro::de_dominio(&e))?;
        RepositorioOs::novo(uow).atualizar_ordem(&os)?;
        os.aprovar(self.identificacao_aprovador)
            .map_err(|e| Erro::de_dominio(&e))?;
        RepositorioOs::novo(uow).atualizar_ordem(&os)?;
        os.iniciar_execucao().map_err(|e| Erro::de_dominio(&e))?;
        RepositorioOs::novo(uow).atualizar_ordem(&os)?;
        uow.publicar(OrcamentoAprovado {
            ordem_servico: os.id,
            valor_total: os.valor_total,
        })
        .map_err(|e| Erro::de_dominio(&e))?;
        Ok(())
    }
}
