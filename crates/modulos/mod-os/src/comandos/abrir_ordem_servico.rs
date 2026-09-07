//! Abre uma ordem de serviço.
//!
//! `docs/modulos/os.md` §5. O vínculo com `agenda.CriarCompromisso` fica para quando o
//! módulo `agenda` existir — por ora `compromisso` não é gravado.

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::eventos::OrdemAberta;
use crate::ordem::OrdemServico;
use crate::repositorio::RepositorioOs;

/// Abre uma ordem de serviço.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbrirOrdemServico {
    /// O cliente (papel `Cliente` em `clientes_pessoa`).
    pub cliente: Id,
    /// Descrição livre do equipamento.
    pub equipamento: String,
    /// O técnico responsável.
    pub tecnico_responsavel: Id,
    /// Prazo de garantia em dias sobre as peças aplicadas (padrão sugerido: 90).
    pub garantia_dias: u16,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct OrdemServicoAberta {
    /// A ordem criada.
    pub ordem_servico: Id,
    /// O número sequencial atribuído.
    pub numero: u64,
}

impl Comando for AbrirOrdemServico {
    type Saida = OrdemServicoAberta;
    const PERMISSAO: &'static str = "os.ordem.criar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Numerar.
        let numero = RepositorioOs::novo(uow).proximo_numero()?;

        // 2. Validar (domínio puro).
        let os = OrdemServico::abrir(
            ctx.empresa,
            numero,
            self.cliente,
            self.equipamento,
            self.tecnico_responsavel,
            ctx.hoje(),
            self.garantia_dias,
        )
        .map_err(|e| Erro::de_dominio(&e))?;

        // 4. Persistir.
        RepositorioOs::novo(uow).inserir_ordem(&os)?;

        // 5. Publicar.
        uow.publicar(OrdemAberta {
            ordem_servico: os.id,
            cliente: os.cliente,
        })
        .map_err(|e| Erro::de_dominio(&e))?;

        Ok(OrdemServicoAberta {
            ordem_servico: os.id,
            numero: os.numero,
        })
    }
}
