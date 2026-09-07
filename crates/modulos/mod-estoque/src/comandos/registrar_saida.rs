//! Registra uma saída manual de estoque (o espelho a partir de `os`/`vendas` chama
//! [`registrar_saida_comum`](super::registrar_saida_comum) direto, com o próprio
//! `origem_modulo`).

use cardeal_kernel::{Dinheiro, Id, Preco, Quantidade, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::{registrar_saida_comum, DadosSaida};

/// Registra uma saída manual de estoque.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RegistrarSaida {
    /// O produto.
    pub produto: Id,
    /// O local de onde sai.
    pub local: Id,
    /// A quantidade.
    pub quantidade: Quantidade,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct SaidaRegistrada {
    /// O movimento criado.
    pub movimento: Id,
    /// O custo unitário usado (o médio vigente no momento da saída).
    pub custo_unitario: Preco,
    /// `quantidade × custo_unitario` — o CMV desta saída.
    pub custo_total: Dinheiro,
    /// Verdadeiro se a saída deixou (ou manteve) o disponível negativo.
    pub gerou_divergencia: bool,
}

impl Comando for RegistrarSaida {
    type Saida = SaidaRegistrada;
    const PERMISSAO: &'static str = "estoque.movimento.saida";
    const RISCO: Risco = Risco::Medio;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let g = registrar_saida_comum(
            DadosSaida {
                produto: self.produto,
                local: self.local,
                quantidade: self.quantidade,
                origem_modulo: "estoque",
                origem_id: None,
            },
            ctx,
            uow,
        )?;
        Ok(SaidaRegistrada {
            movimento: g.movimento,
            custo_unitario: g.aplicada.custo_unitario,
            custo_total: g.aplicada.custo_total,
            gerou_divergencia: g.aplicada.gerou_divergencia,
        })
    }
}
