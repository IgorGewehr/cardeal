//! Acrescenta um item ao pedido, resolvendo o preço vigente na tabela do pedido e
//! congelando-o no item. `docs/modulos/vendas.md` §5 e §11.1.
//!
//! **Nota sobre permissão**: o spec original previa `vendas.pedido.descontar` (Médio) como
//! permissão própria para conceder desconto, separada de `vendas.pedido.editar` (Baixo) para
//! só adicionar item. Nesta fatia, desconto é só um parâmetro de `AdicionarItemPedido` — não
//! há como o despacho checar duas permissões num só `Comando` (`cardeal_modkit::Comando` tem
//! uma única `PERMISSAO`, o mesmo motivo que separa `LancarTituloAReceber`/
//! `LancarTituloAPagar` em `mod-financeiro`). Decisão: quem edita o pedido também pode
//! descontar, sempre limitado a `ctx.limite("vendas.desconto_maximo")` do próprio papel — a
//! permissão `vendas.pedido.descontar` do manifesto fica sem uso até valer a pena separar
//! `AdicionarItemPedido` em dois comandos.

use cardeal_kernel::{Erro, Id, Percentual, Preco, Quantidade, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use mod_estoque::RepositorioEstoque;
use serde::{Deserialize, Serialize};

use crate::comandos::{carregar_pedido, limite_desconto};
use crate::pedido::ItemVenda;
use crate::preco::preco_vigente;
use crate::repositorio::RepositorioVendas;

/// Acrescenta um item ao pedido (só em `Rascunho`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AdicionarItemPedido {
    /// O pedido.
    pub pedido: Id,
    /// O produto.
    pub produto: Id,
    /// A variação de grade, se houver.
    pub variacao: Option<Id>,
    /// A quantidade.
    pub quantidade: Quantidade,
    /// O desconto pedido, em pontos percentuais.
    pub desconto_percentual: Percentual,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct ItemFoiAdicionado {
    /// O item criado.
    pub item: Id,
    /// O preço unitário resolvido.
    pub preco_unitario: Preco,
    /// O novo total do pedido.
    pub total_pedido: cardeal_kernel::Dinheiro,
}

impl Comando for AdicionarItemPedido {
    type Saida = ItemFoiAdicionado;
    const PERMISSAO: &'static str = "vendas.pedido.editar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut pedido = carregar_pedido(uow, self.pedido)?;
        let produto = RepositorioEstoque::novo(uow)
            .buscar_produto(self.produto)?
            .ok_or_else(|| Erro::nao_encontrado("produto"))?;
        let tabela = RepositorioVendas::novo(uow)
            .buscar_tabela_preco(pedido.tabela_preco)?
            .ok_or_else(|| Erro::nao_encontrado("tabela de preço"))?;
        let regras = RepositorioVendas::novo(uow).regras_da_tabela(tabela.id)?;

        // 2. Validar (domínio puro): resolve o preço vigente, congela no item.
        let preco_unitario = preco_vigente(
            &tabela,
            &regras,
            self.produto,
            produto.grupo_produto,
            self.quantidade,
            pedido.data,
        )
        .map_err(|e| Erro::de_dominio(&e))?;
        let item = ItemVenda::novo(
            self.produto,
            self.variacao,
            self.quantidade,
            preco_unitario,
            self.desconto_percentual,
            limite_desconto(ctx),
        )
        .map_err(|e| Erro::de_dominio(&e))?;
        pedido
            .adicionar_item(item.clone())
            .map_err(|e| Erro::de_dominio(&e))?;

        // 4. Persistir.
        let mut repo = RepositorioVendas::novo(uow);
        repo.inserir_item(pedido.id, &item)?;
        repo.atualizar_pedido(&pedido)?;

        Ok(ItemFoiAdicionado {
            item: item.id,
            preco_unitario,
            total_pedido: pedido.total,
        })
    }
}
