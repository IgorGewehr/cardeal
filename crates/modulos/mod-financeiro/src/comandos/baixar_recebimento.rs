//! Dá baixa (total ou parcial) numa parcela de título **a receber**.
//!
//! Receituário (`docs/modulos/financeiro.md` §7): D Caixa/Bancos (valor recebido) +
//! D Descontos concedidos (desconto) · C Clientes a receber (principal) +
//! C Receita financeira (juros + multa). Juros, multa e desconto são calculados **na data
//! informada** (`docs/modulos/financeiro.md` §11.1). Qual das duas — Caixa ou Bancos — é
//! `meio_pagamento` quem decide (`crate::MeioPagamento::papel`), a menos que `conta_destino`
//! escolha uma conta específica na mão.

use cardeal_kernel::{Data, Dinheiro, Id, Resultado};
use cardeal_ledger::PapelConta;
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::{baixar_parcela_comum, DadosBaixa};
use crate::meio_pagamento::MeioPagamento;
use crate::titulo::EspecieTitulo;

/// Baixa uma parcela a receber.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaixarRecebimento {
    /// A parcela a baixar.
    pub parcela: Id,
    /// O valor recebido.
    pub valor: Dinheiro,
    /// A data do recebimento — define juros, multa e disponibilidade de desconto.
    pub data: Data,
    /// Dinheiro, Pix ou cartão — decide a conta de destino padrão (`MeioPagamento::papel`).
    pub meio_pagamento: MeioPagamento,
    /// Escolhe uma conta específica, por fora do padrão de `meio_pagamento` (ex.: uma das
    /// várias contas bancárias da empresa). `None` = usa o padrão do meio de pagamento.
    pub conta_destino: Option<Id>,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct RecebimentoBaixado {
    /// A baixa criada.
    pub baixa: Id,
    /// O lançamento `Realizado` gerado.
    pub lancamento: Id,
    /// Verdadeiro se a baixa quitou a parcela.
    pub parcela_quitada: bool,
    /// O saldo de principal que resta na parcela.
    pub saldo_restante: Dinheiro,
}

impl Comando for BaixarRecebimento {
    type Saida = RecebimentoBaixado;
    const PERMISSAO: &'static str = "financeiro.receber.baixar";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        baixar_recebimento_comum(
            self.parcela,
            self.valor,
            self.data,
            self.meio_pagamento,
            self.conta_destino,
            ctx,
            uow,
        )
    }
}

/// Dá baixa completa (ou parcial) numa parcela a receber, direto-na-transação — mesmo corpo
/// de [`BaixarRecebimento`], só chamável sem passar pelo despacho de `Comando`. Mesmo padrão
/// de [`super::baixar_pagamento_comum`] para outro módulo chamar direto na mesma transação
/// (`docs/contratos-internos.md` §7 regra 2) — usado por `mod_os::FaturarOrdemServico` quando
/// a OS já foi paga na hora (o caso comum no balcão: fatura e recebe no mesmo clique).
///
/// `conta_destino: None` usa a conta do papel de `meio_pagamento` (`MeioPagamento::papel`).
///
/// # Errors
/// Igual a [`BaixarRecebimento`]: erro de domínio (parcela não encontrada, título de espécie
/// diferente de `Receber`, valor inválido ou maior que o devido) ou de infraestrutura.
pub fn baixar_recebimento_comum(
    parcela: Id,
    valor: Dinheiro,
    data: Data,
    meio_pagamento: MeioPagamento,
    conta_destino: Option<Id>,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<RecebimentoBaixado> {
    let b = baixar_parcela_comum(
        DadosBaixa {
            parcela,
            valor,
            data,
            conta_destino,
            especie: EspecieTitulo::Receber,
            papel_destino_padrao: meio_pagamento.papel(),
            papel_encargos: PapelConta::ReceitaFinanceira,
            papel_desconto: PapelConta::DescontosConcedidos,
        },
        ctx,
        uow,
    )?;
    Ok(RecebimentoBaixado {
        baixa: b.baixa,
        lancamento: b.lancamento,
        parcela_quitada: b.quitada,
        saldo_restante: b.saldo_restante,
    })
}
