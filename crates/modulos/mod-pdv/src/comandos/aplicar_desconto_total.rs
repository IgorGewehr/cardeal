//! Dá (ou tira) o desconto no total da venda (`Shift+F5`). `docs/modulos/pdv.md` §5.
//!
//! "10% na compra" ou "fecha em R$ 100": vale sobre o que os itens somam enquanto a venda está
//! aberta e, ao finalizar, é rateado entre eles ([`crate::Cupom::ratear_desconto_no_total`]).
//! O teto do papel (`vendas.desconto_maximo`) vale para a venda inteira — itens e total.

use cardeal_kernel::{Dinheiro, Id};
#[cfg(feature = "sqlite")]
use cardeal_kernel::{Erro, Resultado};
use cardeal_modkit::Comando;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::comandos::{carregar_cupom, limite_desconto};
use crate::cupom::DescontoNoTotal;
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioPdv;

/// Dá, troca ou tira (`None`) o desconto no total.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AplicarDescontoNoTotal {
    /// O cupom.
    pub cupom: Id,
    /// O desconto; `None` tira.
    pub desconto: Option<DescontoNoTotal>,
}

/// O cupom depois do desconto.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DescontoNoTotalAplicado {
    /// Quanto o desconto no total tira agora.
    pub desconto_no_total: Dinheiro,
    /// O desconto da venda inteira (itens + total).
    pub desconto: Dinheiro,
    /// O total a pagar.
    pub total: Dinheiro,
}

impl Comando for AplicarDescontoNoTotal {
    type Saida = DescontoNoTotalAplicado;
    const PERMISSAO: &'static str = "pdv.desconto.aplicar";
    const RISCO: Risco = Risco::Medio;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut cupom = carregar_cupom(uow, self.cupom)?;

        // 2. Validar (domínio puro): estado, valor e teto do papel.
        cupom
            .definir_desconto_no_total(self.desconto, limite_desconto(ctx))
            .map_err(|e| Erro::de_dominio(&e))?;

        // 4. Persistir.
        RepositorioPdv::novo(uow).atualizar_cupom(&cupom)?;
        Ok(DescontoNoTotalAplicado {
            desconto_no_total: cupom.valor_do_desconto_no_total(),
            desconto: cupom.desconto,
            total: cupom.total,
        })
    }
}
