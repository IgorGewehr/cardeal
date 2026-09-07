//! Dá baixa (total ou parcial) numa parcela de título **a receber**.
//!
//! Receituário (`docs/modulos/financeiro.md` §7): D Caixa/Bancos (valor recebido) +
//! D Descontos concedidos (desconto) · C Clientes a receber (principal) +
//! C Receita financeira (juros + multa). Juros, multa e desconto são calculados **na data
//! informada** (`docs/modulos/financeiro.md` §11.1).

use cardeal_kernel::{Data, Dinheiro, Id, Resultado};
use cardeal_ledger::PapelConta;
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::{baixar_parcela_comum, DadosBaixa};
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
    /// A conta que recebeu o dinheiro. `None` = a conta de Caixa da empresa.
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
        let b = baixar_parcela_comum(
            DadosBaixa {
                parcela: self.parcela,
                valor: self.valor,
                data: self.data,
                conta_destino: self.conta_destino,
                especie: EspecieTitulo::Receber,
                papel_destino_padrao: PapelConta::Caixa,
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
}
