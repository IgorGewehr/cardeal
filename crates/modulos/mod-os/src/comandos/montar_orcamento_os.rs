//! Acrescenta um item (peça ou mão de obra) ao orçamento de uma ordem de serviço.
//!
//! `docs/modulos/os.md` §5. Chamado uma vez por item — a tela monta o orçamento linha a
//! linha. Só aceita enquanto `OrdemServico::aceita_edicao_de_orcamento`.

use cardeal_kernel::{Dinheiro, Erro, Id, Preco, Quantidade, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::carregar_ordem;
use crate::erros::ErroOs;
use crate::execucao::{ItemMaoDeObra, ItemPeca};
use crate::ordem::EstadoOs;
use crate::repositorio::RepositorioOs;

/// Um item novo para o orçamento — peça ou mão de obra.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ItemOrcamentoNovo {
    /// Uma peça do estoque.
    Peca {
        /// O produto.
        produto: Id,
        /// A quantidade orçada.
        quantidade: Quantidade,
        /// O preço cobrado do cliente por unidade.
        preco_unitario: Preco,
    },
    /// Um serviço de mão de obra.
    MaoDeObra {
        /// Descrição do serviço.
        descricao: String,
        /// O valor cobrado.
        valor: Dinheiro,
        /// O técnico que prestou o serviço.
        tecnico: Id,
        /// Horas trabalhadas, quando registradas.
        horas: Option<Quantidade>,
    },
}

/// Acrescenta um item ao orçamento.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MontarOrcamentoOs {
    /// A ordem de serviço.
    pub ordem_servico: Id,
    /// O item a acrescentar.
    pub item: ItemOrcamentoNovo,
}

impl Comando for MontarOrcamentoOs {
    type Saida = Id;
    const PERMISSAO: &'static str = "os.orcamento.montar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut os = carregar_ordem(uow, self.ordem_servico)?;

        // 2. Validar (domínio puro): estado aceita edição, antes de qualquer escrita. Peça
        // nova também pode ser orçada em `EmExecucao` — mesma brecha que
        // `OrdemServico::adicionar_ao_orcamento` já concede a `RegistrarMaoDeObra`.
        if !os.estado.aceita_edicao_de_orcamento() && os.estado != EstadoOs::EmExecucao {
            return Err(Erro::de_dominio(&ErroOs::EstadoInvalido {
                atual: os.estado.rotulo(),
                esperado: "Aberta, EmDiagnostico ou EmExecucao",
            }));
        }

        let (item_id, total_do_item) = match self.item {
            ItemOrcamentoNovo::Peca {
                produto,
                quantidade,
                preco_unitario,
            } => {
                let item = ItemPeca::novo(os.id, produto, quantidade, preco_unitario);
                let total = item.total_cobrado();
                RepositorioOs::novo(uow).inserir_item_peca(&item)?;
                (item.id, total)
            }
            ItemOrcamentoNovo::MaoDeObra {
                descricao,
                valor,
                tecnico,
                horas,
            } => {
                let item = ItemMaoDeObra::novo(os.id, descricao, valor, tecnico, horas)
                    .map_err(|e| Erro::de_dominio(&e))?;
                let total = item.valor;
                RepositorioOs::novo(uow).inserir_item_mao_de_obra(&item)?;
                (item.id, total)
            }
        };

        // 4. Persistir: acrescenta o item ao total da OS.
        os.adicionar_ao_orcamento(total_do_item)
            .map_err(|e| Erro::de_dominio(&e))?;
        RepositorioOs::novo(uow).atualizar_ordem(&os)?;

        Ok(item_id)
    }
}
