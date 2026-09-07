//! Lança um título a pagar avulso — o espelho de [`LancarTituloAReceber`].
//!
//! Receituário (`docs/modulos/financeiro.md` §7): D Despesa administrativa · C Fornecedores
//! (a obrigação existe; o dinheiro ainda não saiu).
//!
//! [`LancarTituloAReceber`]: super::LancarTituloAReceber

use cardeal_kernel::{Data, Dinheiro, Id, Resultado};
use cardeal_ledger::{Contraparte, PapelConta};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::{lancar_titulo_comum, DadosLancamentoTitulo};
use crate::titulo::EspecieTitulo;

/// Lança um título a pagar com uma ou mais parcelas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LancarTituloAPagar {
    /// O fornecedor credor.
    pub fornecedor: Id,
    /// O valor total (soma das parcelas).
    pub valor_total: Dinheiro,
    /// A data de emissão.
    pub emissao: Data,
    /// Quantas parcelas (1..=360).
    pub parcelas: u16,
    /// O vencimento da primeira parcela.
    pub primeiro_vencimento: Data,
    /// Dias entre parcelas consecutivas.
    pub intervalo_dias: i32,
    /// Observação livre.
    pub observacao: Option<String>,
    /// Categoria de relatório, opcional — "Aluguel", "Internet" (não afeta a contabilização,
    /// só alimenta gráficos de custo por categoria/mês).
    pub categoria: Option<Id>,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct TituloAPagarLancado {
    /// O título criado.
    pub titulo: Id,
    /// As parcelas, em ordem.
    pub parcelas: Vec<Id>,
    /// Os lançamentos `Confirmado` gerados, um por parcela.
    pub lancamentos: Vec<Id>,
}

impl Comando for LancarTituloAPagar {
    type Saida = TituloAPagarLancado;
    const PERMISSAO: &'static str = "financeiro.pagar.criar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let g = lancar_titulo_comum(
            DadosLancamentoTitulo {
                especie: EspecieTitulo::Pagar,
                contraparte: Contraparte::Fornecedor(self.fornecedor),
                valor_total: self.valor_total,
                emissao: self.emissao,
                parcelas: self.parcelas,
                primeiro_vencimento: self.primeiro_vencimento,
                intervalo_dias: self.intervalo_dias,
                observacao: self.observacao,
                categoria: self.categoria,
                origem_modulo: "avulso",
                origem_id: None,
                papel_contraparte: PapelConta::Fornecedores,
                papel_resultado: PapelConta::DespesaAdministrativa,
            },
            ctx,
            uow,
        )?;
        Ok(TituloAPagarLancado {
            titulo: g.titulo,
            parcelas: g.parcelas,
            lancamentos: g.lancamentos,
        })
    }
}
