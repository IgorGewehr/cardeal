//! Completa/corrige dados gerais de uma ordem já aberta: o equipamento (quando não foi
//! informado na abertura, ou veio incompleto) e um complemento ao defeito relatado (quando o
//! cliente lembra de mais detalhe depois). `docs/modulos/os.md` §5 — nasce do pedido do
//! usuário de que só nome do cliente + defeito relatado sejam obrigatórios na abertura; tudo
//! o mais (equipamento, documento, contato do cliente etc.) se completa depois, e este
//! comando é a metade de "depois" que pertence à própria OS.

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::carregar_ordem;
use crate::erros::ErroOs;
use crate::repositorio::RepositorioOs;

/// Completa/corrige o equipamento e/ou complementa o defeito relatado de uma OS já aberta.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditarDadosDaOrdem {
    /// A ordem de serviço.
    pub ordem_servico: Id,
    /// O equipamento, quando ainda não informado (ou para corrigir o que foi digitado).
    /// `None` = não mexer neste campo.
    pub equipamento: Option<String>,
    /// Um complemento ao defeito relatado — **acrescentado** ao relato original, nunca o
    /// substitui. `None` = não mexer neste campo.
    pub complemento_defeito_relatado: Option<String>,
}

impl Comando for EditarDadosDaOrdem {
    type Saida = ();
    const PERMISSAO: &'static str = "os.ordem.editar_dados";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        if self.equipamento.is_none() && self.complemento_defeito_relatado.is_none() {
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
        RepositorioOs::novo(uow).atualizar_ordem(&os)?;

        Ok(())
    }
}
