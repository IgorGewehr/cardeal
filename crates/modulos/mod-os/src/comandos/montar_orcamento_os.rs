//! Acrescenta um item (peça ou mão de obra) ao orçamento de uma ordem de serviço.
//!
//! `docs/modulos/os.md` §5. Chamado uma vez por item — a tela monta o orçamento linha a
//! linha. Só aceita enquanto `OrdemServico::aceita_ajuste_de_itens`.

use cardeal_kernel::{Dinheiro, Id, Preco, Quantidade};
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
#[cfg(feature = "sqlite")]
use crate::execucao::{ItemMaoDeObra, ItemPeca};
#[cfg(feature = "sqlite")]
use crate::ordem::EstadoOs;
#[cfg(feature = "sqlite")]
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
    /// Uma peça que ainda não existe no catálogo: cadastrada na hora só pelo nome
    /// (`mod_estoque::criar_peca_rapida_comum` — grupo e unidade padrão, NCM pendente) e
    /// orçada na mesma transação. Se já existe produto com esse nome, usa ele.
    PecaNova {
        /// O nome da peça, como o balcão chama.
        nome: String,
        /// A quantidade orçada.
        quantidade: Quantidade,
        /// O preço cobrado do cliente por unidade.
        preco_unitario: Preco,
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

#[cfg(feature = "sqlite")]
impl Comando for MontarOrcamentoOs {
    type Saida = Id;
    const PERMISSAO: &'static str = "os.orcamento.montar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut os = carregar_ordem(uow, self.ordem_servico)?;

        // 2. Validar (domínio puro): estado aceita edição, antes de qualquer escrita. Peça
        // nova também pode ser orçada em `EmExecucao` — mesma brecha que
        // `OrdemServico::adicionar_ao_orcamento` já concede a `RegistrarMaoDeObra` — e em
        // `Concluida`, para corrigir uma OS desfaturada sem repetir o trâmite técnico
        // inteiro (`OrdemServico::desfaturar`).
        if !os.estado.aceita_ajuste_de_itens() && os.estado != EstadoOs::EmExecucao {
            return Err(Erro::de_dominio(&ErroOs::EstadoInvalido {
                atual: os.estado.rotulo(),
                esperado: "Aberta, EmDiagnostico, EmExecucao ou Concluida",
            }));
        }

        let (item_id, total_do_item) = match self.item {
            ItemOrcamentoNovo::Peca {
                produto,
                quantidade,
                preco_unitario,
            } => orcar_peca(os.id, produto, quantidade, preco_unitario, uow)?,
            ItemOrcamentoNovo::PecaNova {
                nome,
                quantidade,
                preco_unitario,
            } => {
                // Uma permissão por comando: a de cadastrar produto é conferida aqui, para o
                // orçamento não virar porta dos fundos do catálogo.
                if !ctx.concede("estoque.produto.criar") {
                    return Err(Erro::novo(
                        cardeal_kernel::CodigoErro::SEM_PERMISSAO,
                        "sem permissão para cadastrar peça nova",
                    ));
                }
                let produto = mod_estoque::criar_peca_rapida_comum(&nome, ctx, uow)?.produto;
                orcar_peca(os.id, produto, quantidade, preco_unitario, uow)?
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

/// Grava um item de peça e devolve `(id, total cobrado)`.
#[cfg(feature = "sqlite")]
fn orcar_peca(
    ordem: Id,
    produto: Id,
    quantidade: Quantidade,
    preco_unitario: Preco,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<(Id, Dinheiro)> {
    let item = ItemPeca::novo(ordem, produto, quantidade, preco_unitario);
    let total = item.total_cobrado();
    RepositorioOs::novo(uow).inserir_item_peca(&item)?;
    Ok((item.id, total))
}
