//! Registra uma entrada manual de estoque (o espelho a partir de `compras` chama
//! [`registrar_entrada_comum`](super::registrar_entrada_comum) direto, com
//! `origem_modulo: "compras"`, quando esse módulo existir).

use cardeal_kernel::{Id, Preco, Quantidade, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::{registrar_entrada_comum, DadosEntrada};

/// Registra uma entrada manual de estoque.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RegistrarEntrada {
    /// O produto.
    pub produto: Id,
    /// O local que recebe.
    pub local: Id,
    /// A quantidade.
    pub quantidade: Quantidade,
    /// O custo unitário desta entrada.
    pub custo_unitario: Preco,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct EntradaRegistrada {
    /// O movimento criado.
    pub movimento: Id,
    /// O custo médio resultante após a entrada.
    pub custo_medio: Preco,
}

impl Comando for RegistrarEntrada {
    type Saida = EntradaRegistrada;
    const PERMISSAO: &'static str = "estoque.movimento.entrada";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let g = registrar_entrada_comum(
            DadosEntrada {
                produto: self.produto,
                local: self.local,
                quantidade: self.quantidade,
                custo_unitario: self.custo_unitario,
                origem_modulo: "estoque",
                origem_id: None,
            },
            ctx,
            uow,
        )?;
        Ok(EntradaRegistrada {
            movimento: g.movimento,
            custo_medio: g.custo_medio,
        })
    }
}
