//! Define (ou corrige) o preço de uma peça já na OS — pedido do usuário (2026-11-07): a peça
//! entra na OS sem preço quando ainda não se sabe quanto vai custar (encomendada, sem
//! orçamento do fornecedor); quando o preço chega, ele se define aqui e o total da OS acompanha.

#[cfg(feature = "sqlite")]
use cardeal_kernel::{Erro, Resultado};
use cardeal_kernel::{Id, Preco};
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
use crate::repositorio::RepositorioOs;

/// O preço unitário de uma peça da OS.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DefinirPrecoDaPeca {
    /// A ordem de serviço.
    pub ordem_servico: Id,
    /// O item de peça.
    pub item_peca: Id,
    /// O preço por unidade cobrado do cliente.
    pub preco_unitario: Preco,
    /// O custo previsto por unidade (peça fora do estoque: o que vou pagar por ela). Vale para
    /// a margem até a peça ser aplicada, quando o custo real do estoque o substitui. `None` =
    /// não mexer.
    #[serde(default)]
    pub custo_previsto: Option<Preco>,
}

impl Comando for DefinirPrecoDaPeca {
    type Saida = ();
    const PERMISSAO: &'static str = "os.orcamento.montar";
    const RISCO: Risco = Risco::Baixo;

    #[cfg(feature = "sqlite")]
    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut os = carregar_ordem(uow, self.ordem_servico)?;
        let mut item = RepositorioOs::novo(uow)
            .buscar_item_peca(self.item_peca)?
            .filter(|i| i.ordem_servico == os.id)
            .ok_or_else(|| Erro::nao_encontrado("item de peça"))?;

        // 2. Validar (domínio puro) e 3. calcular o novo total.
        let antes = item.total_cobrado();
        item.preco_unitario = self.preco_unitario;
        if let Some(custo) = self.custo_previsto {
            if !item.aplicada {
                item.custo_unitario = custo;
            }
        }
        os.trocar_valor_de_item(antes, item.total_cobrado())
            .map_err(|e| Erro::de_dominio(&e))?;

        // 4. Persistir.
        let mut repo = RepositorioOs::novo(uow);
        repo.atualizar_preco_item_peca(&item)?;
        repo.atualizar_ordem(&os)?;
        Ok(())
    }
}
