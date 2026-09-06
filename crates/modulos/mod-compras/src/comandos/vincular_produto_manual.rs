//! Vincula manualmente um item de nota a um produto do estoque, e aprende a regra para a
//! próxima nota do mesmo fornecedor com o mesmo código nunca mais perguntar
//! (`docs/modulos/compras.md` §11.2).

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::casamento::RegraCasamentoAprendida;
use crate::comandos::carregar_nota;
use crate::erros::ErroCompras;
use crate::nota::EstadoNotaEntrada;
use crate::repositorio::RepositorioCompras;

/// Vincula um item de nota a um produto do estoque.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct VincularProdutoManual {
    /// O item a vincular.
    pub item_nota: Id,
    /// O produto do estoque.
    pub produto: Id,
}

impl Comando for VincularProdutoManual {
    type Saida = ();
    const PERMISSAO: &'static str = "compras.entrada.conferir";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut item = RepositorioCompras::novo(uow)
            .buscar_item(self.item_nota)?
            .ok_or_else(|| Erro::nao_encontrado("item de nota"))?;
        let nota = carregar_nota(uow, item.nota_entrada)?;

        // 2. Validar.
        if matches!(
            nota.estado,
            EstadoNotaEntrada::Confirmada | EstadoNotaEntrada::Devolvida
        ) {
            return Err(Erro::de_dominio(&ErroCompras::NotaJaConfirmada));
        }
        item.vincular(self.produto);

        // 4. Persistir: o item, e a regra aprendida para a próxima nota.
        let regra = RegraCasamentoAprendida {
            empresa: ctx.empresa,
            fornecedor: nota.fornecedor,
            codigo_fornecedor: item.codigo_fornecedor.clone(),
            produto: self.produto,
            aprendido_em: ctx.agora,
        };
        let mut repo = RepositorioCompras::novo(uow);
        repo.atualizar_item(&item)?;
        repo.upsert_regra_casamento(&regra)?;

        Ok(())
    }
}
