//! "Já foi pago/recebido": o lançamento avulso que nasce quitado — a conta de luz paga agora,
//! o Pix que o cliente acabou de mandar. Título e baixa no mesmo COMMIT, pelo mesmo caminho
//! de uma baixa normal (Dinheiro no Caixa, Pix/cartão na conta escolhida).

use cardeal_kernel::Id;
#[cfg(feature = "sqlite")]
use cardeal_kernel::{CodigoErro, Data, Dinheiro, Erro, Resultado};
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use super::{baixar_pagamento_comum, baixar_recebimento_comum};
use crate::meio_pagamento::MeioPagamento;
#[cfg(feature = "sqlite")]
use crate::titulo::EspecieTitulo;

/// Como o valor do lançamento já andou.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuitadoAgora {
    /// O meio (Dinheiro entra/sai do Caixa; Pix/cartão da conta bancária).
    pub meio_pagamento: MeioPagamento,
    /// A conta bancária, quando não é a padrão do meio.
    pub conta: Option<Id>,
}

/// Confere a permissão de baixa e a forma do título antes de gravar qualquer coisa.
///
/// # Errors
/// Sem permissão de baixar a espécie, ou mais de uma parcela (quitado agora é pagamento único).
#[cfg(feature = "sqlite")]
pub(super) fn validar(especie: EspecieTitulo, parcelas: u16, ctx: &Ctx) -> Resultado<()> {
    let permissao = match especie {
        EspecieTitulo::Receber => "financeiro.receber.baixar",
        EspecieTitulo::Pagar => "financeiro.pagar.baixar",
    };
    if !ctx.concede(permissao) {
        return Err(Erro::novo(
            CodigoErro::SEM_PERMISSAO,
            "sem permissão para dar baixa — lance a prazo e peça para quem pode baixar",
        ));
    }
    if parcelas != 1 {
        return Err(Erro::novo(
            CodigoErro::REGRA_VIOLADA,
            "um lançamento já quitado tem uma parcela só",
        )
        .no_campo("parcelas"));
    }
    Ok(())
}

/// Baixa a parcela única recém-lançada pelo valor total, na `data` dada.
///
/// # Errors
/// Os da baixa comum (caixa fechado, conta inválida…) — e o COMMIT inteiro é desfeito.
#[cfg(feature = "sqlite")]
pub(super) fn quitar(
    especie: EspecieTitulo,
    parcela: Option<Id>,
    valor: Dinheiro,
    data: Data,
    q: QuitadoAgora,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<()> {
    let parcela = parcela.ok_or_else(|| Erro::nao_encontrado("parcela do lançamento"))?;
    match especie {
        EspecieTitulo::Receber => {
            baixar_recebimento_comum(parcela, valor, data, q.meio_pagamento, q.conta, ctx, uow)
                .map(|_| ())
        }
        EspecieTitulo::Pagar => {
            baixar_pagamento_comum(parcela, valor, data, q.meio_pagamento, q.conta, ctx, uow)
                .map(|_| ())
        }
    }
}
