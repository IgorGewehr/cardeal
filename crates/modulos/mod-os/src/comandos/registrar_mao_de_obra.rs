//! Registra um item de mão de obra durante a execução (além dos já orçados).
//!
//! `docs/modulos/os.md` §5. Ao contrário da peça, mão de obra registrada em `EmExecucao`
//! não precisa de consumo de estoque — só soma ao total cobrado.

use cardeal_kernel::{Dinheiro, Erro, Id, Quantidade, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::carregar_ordem;
use crate::execucao::ItemMaoDeObra;
use crate::repositorio::RepositorioOs;

/// Registra um item de mão de obra.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrarMaoDeObra {
    /// A ordem de serviço.
    pub ordem_servico: Id,
    /// Descrição do serviço.
    pub descricao: String,
    /// O valor cobrado.
    pub valor: Dinheiro,
    /// O técnico que prestou o serviço.
    pub tecnico: Id,
    /// Horas trabalhadas, quando registradas.
    pub horas: Option<Quantidade>,
}

impl Comando for RegistrarMaoDeObra {
    type Saida = Id;
    const PERMISSAO: &'static str = "os.execucao.registrar_mao_de_obra";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut os = carregar_ordem(uow, self.ordem_servico)?;

        // 2. Validar (domínio puro).
        os.exigir_em_execucao().map_err(|e| Erro::de_dominio(&e))?;
        let item = ItemMaoDeObra::novo(os.id, self.descricao, self.valor, self.tecnico, self.horas)
            .map_err(|e| Erro::de_dominio(&e))?;
        os.adicionar_ao_orcamento(item.valor)
            .map_err(|e| Erro::de_dominio(&e))?;

        // 4. Persistir.
        let mut repo = RepositorioOs::novo(uow);
        repo.inserir_item_mao_de_obra(&item)?;
        repo.atualizar_ordem(&os)?;

        Ok(item.id)
    }
}
