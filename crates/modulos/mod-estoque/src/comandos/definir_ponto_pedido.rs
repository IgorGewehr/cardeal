//! Define o ponto de pedido e o estoque mínimo de um produto — o gatilho de reposição que
//! `Produto::abaixo_do_ponto`/`produtos_abaixo_do_ponto_pedido` consultam depois
//! (`docs/modulos/estoque.md` §5). Não toca nome/NCM/detalhes técnicos, que têm seus
//! próprios comandos.

use cardeal_kernel::{Erro, Id, Quantidade, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::repositorio::RepositorioEstoque;

/// Define o ponto de pedido e o estoque mínimo de um produto.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DefinirPontoPedido {
    /// O produto.
    pub produto: Id,
    /// O novo ponto de pedido — dispara o alerta quando o disponível cruzar abaixo. `None`
    /// desliga o alerta.
    pub ponto_pedido: Option<Quantidade>,
    /// O novo estoque mínimo (curva ABC, sugestão de compra). `None` remove a sugestão.
    pub estoque_minimo: Option<Quantidade>,
}

impl Comando for DefinirPontoPedido {
    type Saida = ();
    const PERMISSAO: &'static str = "estoque.produto.editar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut repo = RepositorioEstoque::novo(uow);
        let mut produto = repo
            .buscar_produto(self.produto)?
            .ok_or_else(|| Erro::nao_encontrado("produto"))?;

        // 2. Validar (domínio puro) — nenhuma regra além de gravar os campos informados.
        produto.ponto_pedido = self.ponto_pedido;
        produto.estoque_minimo = self.estoque_minimo;

        // 4. Persistir.
        repo.atualizar_ponto_pedido(&produto)?;

        Ok(())
    }
}
