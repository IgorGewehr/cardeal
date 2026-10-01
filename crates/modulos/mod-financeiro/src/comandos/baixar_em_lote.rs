//! Baixa de várias parcelas de uma vez — o fim do dia no balcão ("recebi estes cinco Pix")
//! ou o pagamento de vários boletos. Cada parcela é quitada pelo **total devido na data**
//! (principal + juros + multa − desconto, o mesmo cálculo da baixa individual), no mesmo meio
//! e conta. **Tudo ou nada**: se uma falhar (já quitada, da espécie errada), nenhuma é baixada.

use cardeal_kernel::{Data, Dinheiro, Id};
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
use super::{baixar_pagamento_comum, baixar_recebimento_comum};
use crate::meio_pagamento::MeioPagamento;
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioFinanceiro;

/// Os dados comuns às duas espécies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DadosBaixaEmLote {
    /// As parcelas a quitar.
    pub parcelas: Vec<Id>,
    /// A data das baixas.
    pub data: Data,
    /// Dinheiro (Caixa) ou Pix/cartão (banco).
    pub meio_pagamento: MeioPagamento,
    /// A conta bancária, quando não é a padrão do meio.
    pub conta_destino: Option<Id>,
}

/// O que as baixas em lote devolvem.
#[derive(Debug, Serialize, Deserialize)]
pub struct BaixasEmLoteFeitas {
    /// Quantas parcelas foram quitadas.
    pub quantidade: usize,
    /// A soma do que entrou/saiu.
    pub total: Dinheiro,
}

/// Quita várias parcelas a receber.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaixarRecebimentosEmLote(pub DadosBaixaEmLote);

/// Quita várias parcelas a pagar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaixarPagamentosEmLote(pub DadosBaixaEmLote);

impl Comando for BaixarRecebimentosEmLote {
    type Saida = BaixasEmLoteFeitas;
    const PERMISSAO: &'static str = "financeiro.receber.baixar";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let d = self.0;
        quitar_todas(&d, uow, |parcela, valor, uow| {
            baixar_recebimento_comum(
                parcela,
                valor,
                d.data,
                d.meio_pagamento,
                d.conta_destino,
                ctx,
                uow,
            )
            .map(|_| ())
        })
    }
}

impl Comando for BaixarPagamentosEmLote {
    type Saida = BaixasEmLoteFeitas;
    const PERMISSAO: &'static str = "financeiro.pagar.baixar";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let d = self.0;
        quitar_todas(&d, uow, |parcela, valor, uow| {
            baixar_pagamento_comum(
                parcela,
                valor,
                d.data,
                d.meio_pagamento,
                d.conta_destino,
                ctx,
                uow,
            )
            .map(|_| ())
        })
    }
}

#[cfg(feature = "sqlite")]
fn quitar_todas(
    d: &DadosBaixaEmLote,
    uow: &mut UnidadeDeTrabalho,
    mut baixar: impl FnMut(Id, Dinheiro, &mut UnidadeDeTrabalho) -> Resultado<()>,
) -> Resultado<BaixasEmLoteFeitas> {
    if d.parcelas.is_empty() {
        return Err(Erro::novo(
            cardeal_kernel::CodigoErro::CAMPO_OBRIGATORIO,
            "nenhuma parcela selecionada",
        ));
    }
    let mut total = Dinheiro::ZERO;
    for &parcela in &d.parcelas {
        let devido = RepositorioFinanceiro::novo(uow)
            .buscar_parcela(parcela)?
            .ok_or_else(|| Erro::nao_encontrado("parcela"))?
            .situacao_em(d.data)
            .total_devido();
        baixar(parcela, devido, uow)?;
        total += devido;
    }
    Ok(BaixasEmLoteFeitas {
        quantidade: d.parcelas.len(),
        total,
    })
}
