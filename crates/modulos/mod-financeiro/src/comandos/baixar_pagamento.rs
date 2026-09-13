//! Dá baixa (total ou parcial) numa parcela de título **a pagar** — o espelho de
//! [`BaixarRecebimento`].
//!
//! Receituário (`docs/modulos/financeiro.md` §7): D Fornecedores (principal) +
//! D Despesa financeira (juros + multa) · C Caixa/Bancos (valor pago) +
//! C Outras receitas (desconto por antecipação obtido).
//!
//! [`BaixarRecebimento`]: super::BaixarRecebimento

use cardeal_kernel::{Data, Dinheiro, Id, Resultado};
use cardeal_ledger::PapelConta;
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::{baixar_parcela_comum, DadosBaixa};
use crate::titulo::EspecieTitulo;

/// Baixa uma parcela a pagar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaixarPagamento {
    /// A parcela a baixar.
    pub parcela: Id,
    /// O valor pago.
    pub valor: Dinheiro,
    /// A data do pagamento — define juros, multa e disponibilidade de desconto.
    pub data: Data,
    /// A conta de onde saiu o dinheiro. `None` = a conta de Bancos da empresa.
    pub conta_destino: Option<Id>,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct PagamentoBaixado {
    /// A baixa criada.
    pub baixa: Id,
    /// O lançamento `Realizado` gerado.
    pub lancamento: Id,
    /// Verdadeiro se a baixa quitou a parcela.
    pub parcela_quitada: bool,
    /// O saldo de principal que resta na parcela.
    pub saldo_restante: Dinheiro,
}

impl Comando for BaixarPagamento {
    type Saida = PagamentoBaixado;
    const PERMISSAO: &'static str = "financeiro.pagar.baixar";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        baixar_pagamento_comum(self.parcela, self.valor, self.data, self.conta_destino, ctx, uow)
    }
}

/// Dá baixa completa (ou parcial) numa parcela a pagar, direto-na-transação — mesmo corpo de
/// [`BaixarPagamento`], só chamável sem passar pelo despacho de `Comando`. Para quem já tem a
/// `UnidadeDeTrabalho` em mãos e quer baixar um título que acabou de lançar na mesma
/// transação (ex.: `mod_compras::confirmar_entrada_comum` quando a compra já foi paga no ato
/// — `ConfirmarEntrada::pago_no_ato`), mesmo padrão de `lancar_titulo_comum` para outro
/// módulo chamar direto (`docs/contratos-internos.md` §7 regra 2).
///
/// `conta_destino: None` usa a conta de Bancos da empresa, igual ao comando.
///
/// # Errors
/// Igual a [`BaixarPagamento`]: erro de domínio (parcela não encontrada, título de espécie
/// diferente de `Pagar`, valor inválido ou maior que o devido) ou de infraestrutura.
pub fn baixar_pagamento_comum(
    parcela: Id,
    valor: Dinheiro,
    data: Data,
    conta_destino: Option<Id>,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<PagamentoBaixado> {
    let b = baixar_parcela_comum(
        DadosBaixa {
            parcela,
            valor,
            data,
            conta_destino,
            especie: EspecieTitulo::Pagar,
            papel_destino_padrao: PapelConta::Bancos,
            papel_encargos: PapelConta::DespesaFinanceira,
            papel_desconto: PapelConta::OutrasReceitas,
        },
        ctx,
        uow,
    )?;
    Ok(PagamentoBaixado {
        baixa: b.baixa,
        lancamento: b.lancamento,
        parcela_quitada: b.quitada,
        saldo_restante: b.saldo_restante,
    })
}
