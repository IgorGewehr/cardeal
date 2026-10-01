//! Corrige dados gerais de uma ordem já aberta: o equipamento/aparelho (erro de digitação,
//! detalhe que faltou) e um complemento ao defeito relatado (quando o cliente lembra de mais
//! detalhe depois). `docs/modulos/os.md` §5.

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
use crate::comandos::carregar_ordem;
#[cfg(feature = "sqlite")]
use crate::erros::ErroOs;
use crate::ordem::FichaEntrada;
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioOs;

/// Completa/corrige o equipamento e/ou complementa o defeito relatado de uma OS já aberta.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditarDadosDaOrdem {
    /// A ordem de serviço.
    pub ordem_servico: Id,
    /// Correção do equipamento/aparelho (já obrigatório desde a abertura). `None` = não
    /// mexer neste campo.
    pub equipamento: Option<String>,
    /// Um complemento ao defeito relatado — **acrescentado** ao relato original, nunca o
    /// substitui. `None` = não mexer neste campo.
    pub complemento_defeito_relatado: Option<String>,
    /// A ficha de entrada inteira (previsão, nº de série, acessórios), substituindo a atual.
    /// `None` = não mexer.
    pub ficha: Option<FichaEntrada>,
}

#[cfg(feature = "sqlite")]
impl Comando for EditarDadosDaOrdem {
    type Saida = ();
    const PERMISSAO: &'static str = "os.ordem.editar_dados";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        if self.equipamento.is_none()
            && self.complemento_defeito_relatado.is_none()
            && self.ficha.is_none()
        {
            return Err(Erro::de_dominio(&ErroOs::NadaParaAtualizar));
        }

        // 1. Carregar.
        let mut os = carregar_ordem(uow, self.ordem_servico)?;

        // 2. Validar (domínio puro).
        if let Some(equipamento) = self.equipamento {
            os.completar_equipamento(equipamento)
                .map_err(|e| Erro::de_dominio(&e))?;
        }
        if let Some(complemento) = self.complemento_defeito_relatado {
            os.complementar_defeito_relatado(complemento)
                .map_err(|e| Erro::de_dominio(&e))?;
        }

        // 4. Persistir.
        if let Some(ficha) = self.ficha {
            os.atualizar_ficha(ficha)
                .map_err(|e| Erro::de_dominio(&e))?;
        }

        RepositorioOs::novo(uow).atualizar_ordem(&os)?;

        Ok(())
    }
}
