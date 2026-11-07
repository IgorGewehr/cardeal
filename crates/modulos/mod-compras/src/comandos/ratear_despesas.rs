//! Escolhe **quais itens** da nota levam o frete, o seguro e as outras despesas — pedido do
//! usuário (2026-11-07): numa compra de 10 itens em que o frete só faz sentido sobre 5 (as
//! peças daquela OS, por exemplo), o rateio vai só para esses 5, na base das preferências
//! (valor ou quantidade); os outros ficam com rateio zero. O custo final de cada item — e,
//! depois da entrada, o custo médio do estoque e o custo da peça na OS — já sai com isso.
//!
//! Só antes da entrada ser confirmada: depois, o custo já foi para o estoque.

use cardeal_kernel::Id;
#[cfg(feature = "sqlite")]
use cardeal_kernel::{Dinheiro, Erro, Resultado};
use cardeal_modkit::Comando;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::comandos::carregar_nota;
#[cfg(feature = "sqlite")]
use crate::erros::ErroCompras;
#[cfg(feature = "sqlite")]
use crate::nota::EstadoNotaEntrada;
#[cfg(feature = "sqlite")]
use crate::preferencias::RateioPor;
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioCompras;

/// Rateia as despesas acessórias da nota só entre os itens dados.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RatearDespesasEntre {
    /// A nota de entrada.
    pub nota_entrada: Id,
    /// Os itens que recebem o rateio (ao menos um). Os demais ficam com rateio zero.
    pub itens: Vec<Id>,
}

impl Comando for RatearDespesasEntre {
    type Saida = ();
    const PERMISSAO: &'static str = "compras.entrada.conferir";
    const RISCO: Risco = Risco::Baixo;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let nota = carregar_nota(uow, self.nota_entrada)?;
        let mut repo = RepositorioCompras::novo(uow);
        let mut itens = repo.itens_da_nota(nota.id)?;
        let preferencias = repo.preferencias(ctx.empresa)?;

        // 2. Validar.
        if matches!(
            nota.estado,
            EstadoNotaEntrada::Confirmada | EstadoNotaEntrada::Devolvida
        ) {
            return Err(Erro::de_dominio(&ErroCompras::NotaJaConfirmada));
        }
        if self.itens.is_empty() || !self.itens.iter().all(|i| itens.iter().any(|x| x.id == *i)) {
            return Err(Erro::novo(
                cardeal_kernel::CodigoErro::VALOR_INVALIDO,
                "Escolha ao menos um item desta nota para receber o frete.",
            ));
        }

        // 3. Calcular: peso zero para quem não foi escolhido.
        let despesas = nota.valor_frete + nota.valor_seguro + nota.valor_outras_despesas;
        let pesos: Vec<i64> = itens
            .iter()
            .map(|i| {
                if !self.itens.contains(&i.id) {
                    return 0;
                }
                match preferencias.rateio_por {
                    RateioPor::Valor => i.valor_bruto().em_centavos(),
                    RateioPor::Peso => i.quantidade.unidades_internas(),
                }
            })
            .collect();
        let rateios = if despesas.e_positivo() && pesos.iter().any(|&p| p > 0) {
            despesas.ratear_por_pesos(&pesos)
        } else {
            vec![Dinheiro::ZERO; itens.len()]
        };

        // 4. Persistir.
        for (item, rateio) in itens.iter_mut().zip(rateios) {
            if item.valor_rateio != rateio {
                item.valor_rateio = rateio;
                repo.atualizar_item(item)?;
            }
        }
        Ok(())
    }
}
