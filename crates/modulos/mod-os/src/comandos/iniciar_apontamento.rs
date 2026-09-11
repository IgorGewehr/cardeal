//! Inicia um apontamento de tempo do técnico numa ordem de serviço — o "bater o ponto" real
//! de quanto tempo o trabalho leva, distinto da mão de obra orçada/cobrada
//! (`crate::ItemMaoDeObra::horas`). Ver `crate::apontamento`.
//!
//! `docs/modulos/os.md` §5-bis. Um técnico não pode estar "trabalhando" em duas ordens ao
//! mesmo tempo — a checagem de apontamento aberto roda sobre TODAS as ordens do técnico, não
//! só esta.

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::apontamento::ApontamentoDeTempo;
use crate::comandos::carregar_ordem;
use crate::erros::ErroOs;
use crate::repositorio::RepositorioOs;

/// Inicia um apontamento de tempo.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct IniciarApontamento {
    /// A ordem de serviço em que o técnico está trabalhando.
    pub ordem_servico: Id,
    /// O técnico que está começando a trabalhar.
    pub tecnico: Id,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct ApontamentoIniciado {
    /// O apontamento criado.
    pub apontamento: Id,
}

impl Comando for IniciarApontamento {
    type Saida = ApontamentoIniciado;
    const PERMISSAO: &'static str = "os.apontamento.iniciar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar / validar que a ordem existe.
        let os = carregar_ordem(uow, self.ordem_servico)?;
        let agora = uow.agora();

        // 2. Validar: o técnico não pode ter outro apontamento aberto em nenhuma OS.
        let mut repo = RepositorioOs::novo(uow);
        if repo.apontamento_aberto_do_tecnico(self.tecnico)?.is_some() {
            return Err(Erro::de_dominio(&ErroOs::TecnicoJaTemApontamentoAberto));
        }

        // 3. Construir (domínio puro) e persistir.
        let apontamento = ApontamentoDeTempo::iniciar(os.id, self.tecnico, agora);
        repo.inserir_apontamento(&apontamento)?;

        Ok(ApontamentoIniciado {
            apontamento: apontamento.id,
        })
    }
}
