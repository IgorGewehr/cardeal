//! Acrescenta um item ao cupom, resolvendo o preço vigente na tabela do cupom e congelando-o
//! no item. `docs/modulos/pdv.md` §5 — "não decide preço nem regra de desconto, lê
//! `TabelaPreco`/`RegraPreco` de `vendas` pela mesma porta que o pedido de balcão usa".

use cardeal_kernel::{Erro, Id, Preco, Quantidade, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use mod_estoque::RepositorioEstoque;
use mod_vendas::{preco_vigente, RepositorioVendas};
use serde::{Deserialize, Serialize};

use crate::comandos::carregar_cupom;
use crate::cupom::ItemCupom;
use crate::repositorio::RepositorioPdv;

/// Acrescenta um item ao cupom (só em `EmAndamento`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AdicionarItem {
    /// O cupom.
    pub cupom: Id,
    /// O produto (resolvido a partir do código bipado pelo chamador).
    pub produto: Id,
    /// A variação de grade, se houver.
    pub variacao: Option<Id>,
    /// A quantidade (pode vir da balança — aqui sempre digitada nesta fatia).
    pub quantidade: Quantidade,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct ItemFoiAdicionado {
    /// O item criado.
    pub item: Id,
    /// O preço unitário resolvido.
    pub preco_unitario: Preco,
    /// O novo total do cupom.
    pub total_cupom: cardeal_kernel::Dinheiro,
}

impl Comando for AdicionarItem {
    type Saida = ItemFoiAdicionado;
    const PERMISSAO: &'static str = "pdv.venda.editar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut cupom = carregar_cupom(uow, self.cupom)?;
        let produto = RepositorioEstoque::novo(uow)
            .buscar_produto(self.produto)?
            .ok_or_else(|| Erro::nao_encontrado("produto"))?;
        let tabela = RepositorioVendas::novo(uow)
            .buscar_tabela_preco(cupom.tabela_preco)?
            .ok_or_else(|| Erro::nao_encontrado("tabela de preço"))?;
        let regras = RepositorioVendas::novo(uow).regras_da_tabela(tabela.id)?;

        // 2. Validar (domínio puro): resolve o preço vigente, congela no item.
        let preco_unitario = preco_vigente(
            &tabela,
            &regras,
            self.produto,
            produto.grupo_produto,
            self.quantidade,
            ctx.hoje(),
        )
        .map_err(|e| Erro::de_dominio(&e))?;
        let item = ItemCupom::novo(self.produto, self.variacao, self.quantidade, preco_unitario)
            .map_err(|e| Erro::de_dominio(&e))?;
        cupom
            .adicionar_item(item.clone())
            .map_err(|e| Erro::de_dominio(&e))?;

        // 4. Persistir.
        let mut repo = RepositorioPdv::novo(uow);
        repo.inserir_item(cupom.id, &item)?;
        repo.atualizar_cupom(&cupom)?;

        Ok(ItemFoiAdicionado {
            item: item.id,
            preco_unitario,
            total_cupom: cupom.total,
        })
    }
}
