//! Conta a pagar de uma compra feita sem nota (a peça que chegou para uma OS, a compra rápida
//! no balcão): um título de parcela única, sem fornecedor cadastrado, pago na hora ou a prazo.
//! Um só lugar para isso — `mod-os` (chegada de peça) e `mod-compras` (compra rápida)
//! chamavam, cada um, `lancar_titulo_comum` + `baixar_pagamento_comum` do seu jeito.

use cardeal_kernel::{Data, Dinheiro, Id};
#[cfg(feature = "sqlite")]
use cardeal_kernel::{Erro, Resultado};
use cardeal_ledger::PapelConta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use super::{baixar_pagamento_comum, lancar_titulo_comum, DadosLancamentoTitulo};
use crate::meio_pagamento::MeioPagamento;
#[cfg(feature = "sqlite")]
use crate::titulo::EspecieTitulo;

/// Como a compra foi (ou vai ser) paga.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PagamentoAvulso {
    /// Paga agora (Dinheiro sai do Caixa; Pix/cartão da conta escolhida).
    PagoAgora {
        /// O meio.
        meio_pagamento: MeioPagamento,
        /// A conta bancária, quando não é a padrão do meio.
        conta_origem: Option<Id>,
    },
    /// Fica em Contas a Pagar, vencendo na data dada.
    APrazo {
        /// Vencimento.
        vencimento: Data,
    },
}

impl PagamentoAvulso {
    /// Se já sai pago.
    #[must_use]
    pub const fn pago_agora(&self) -> bool {
        matches!(self, Self::PagoAgora { .. })
    }
}

/// O que identifica a conta a pagar avulsa.
pub struct DadosContaAvulsa {
    /// O valor total.
    pub valor: Dinheiro,
    /// A descrição que aparece em Contas a Pagar ("Peça da OS #12 — Loja X").
    pub observacao: String,
    /// O módulo de origem (nunca `"os"`: é a origem do título a **receber** da OS).
    pub origem_modulo: &'static str,
    /// O registro de origem.
    pub origem_id: Option<Id>,
    /// A conta de débito do lançamento (`EstoqueMercadorias` para peça/compra de estoque).
    pub papel_resultado: PapelConta,
}

/// Lança a conta a pagar (D resultado / C Fornecedores) e, se foi paga na hora, a baixa.
/// Devolve o título. Confere as permissões de financeiro: quem chama pode ser de outro módulo.
///
/// # Errors
/// Sem permissão (`financeiro.pagar.criar`, e `.baixar` se pago agora), valor não positivo,
/// ou erro do financeiro.
#[cfg(feature = "sqlite")]
pub fn lancar_conta_a_pagar_avulsa(
    dados: DadosContaAvulsa,
    pagamento: PagamentoAvulso,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<Id> {
    if !ctx.concede("financeiro.pagar.criar")
        || (pagamento.pago_agora() && !ctx.concede("financeiro.pagar.baixar"))
    {
        return Err(Erro::novo(
            cardeal_kernel::CodigoErro::SEM_PERMISSAO,
            "sem permissão para lançar a conta a pagar",
        ));
    }
    let vencimento = match pagamento {
        PagamentoAvulso::APrazo { vencimento } => vencimento,
        PagamentoAvulso::PagoAgora { .. } => ctx.hoje(),
    };
    let titulo = lancar_titulo_comum(
        DadosLancamentoTitulo {
            especie: EspecieTitulo::Pagar,
            contraparte: None,
            valor_total: dados.valor,
            emissao: ctx.hoje(),
            parcelas: 1,
            primeiro_vencimento: vencimento,
            intervalo_dias: 0,
            observacao: Some(dados.observacao),
            categoria: None,
            origem_modulo: dados.origem_modulo,
            origem_id: dados.origem_id,
            papel_contraparte: PapelConta::Fornecedores,
            papel_resultado: dados.papel_resultado,
        },
        ctx,
        uow,
    )?;
    if let PagamentoAvulso::PagoAgora {
        meio_pagamento,
        conta_origem,
    } = pagamento
    {
        let parcela = titulo
            .parcelas
            .first()
            .copied()
            .ok_or_else(|| Erro::nao_encontrado("parcela da conta a pagar"))?;
        baixar_pagamento_comum(
            parcela,
            dados.valor,
            ctx.hoje(),
            meio_pagamento,
            conta_origem,
            ctx,
            uow,
        )?;
    }
    Ok(titulo.titulo)
}
