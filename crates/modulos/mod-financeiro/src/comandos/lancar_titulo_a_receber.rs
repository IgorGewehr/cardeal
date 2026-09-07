//! Lança um título a receber avulso — cria o [`Titulo`](crate::Titulo), as parcelas e um
//! lançamento `Confirmado` por parcela.
//!
//! Receituário (`docs/modulos/financeiro.md` §7): D Clientes a receber · C Outras receitas
//! (a obrigação existe; o dinheiro ainda não andou).

use cardeal_kernel::{Data, Dinheiro, Id, Resultado};
use cardeal_ledger::{Contraparte, PapelConta};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::{lancar_titulo_comum, DadosLancamentoTitulo};
use crate::titulo::EspecieTitulo;

/// Lança um título a receber com uma ou mais parcelas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LancarTituloAReceber {
    /// O cliente devedor.
    pub cliente: Id,
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
    /// Categoria de relatório, opcional — "Assinatura `SaaS` — Cliente X" (não afeta a
    /// contabilização, só alimenta gráficos de receita por categoria/mês).
    pub categoria: Option<Id>,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct TituloAReceberLancado {
    /// O título criado.
    pub titulo: Id,
    /// As parcelas, em ordem.
    pub parcelas: Vec<Id>,
    /// Os lançamentos `Confirmado` gerados, um por parcela.
    pub lancamentos: Vec<Id>,
}

impl Comando for LancarTituloAReceber {
    type Saida = TituloAReceberLancado;
    const PERMISSAO: &'static str = "financeiro.receber.criar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let g = lancar_titulo_comum(
            DadosLancamentoTitulo {
                especie: EspecieTitulo::Receber,
                contraparte: Contraparte::Cliente(self.cliente),
                valor_total: self.valor_total,
                emissao: self.emissao,
                parcelas: self.parcelas,
                primeiro_vencimento: self.primeiro_vencimento,
                intervalo_dias: self.intervalo_dias,
                observacao: self.observacao,
                categoria: self.categoria,
                origem_modulo: "avulso",
                origem_id: None,
                papel_contraparte: PapelConta::ClientesAReceber,
                papel_resultado: PapelConta::OutrasReceitas,
            },
            ctx,
            uow,
        )?;
        Ok(TituloAReceberLancado {
            titulo: g.titulo,
            parcelas: g.parcelas,
            lancamentos: g.lancamentos,
        })
    }
}
