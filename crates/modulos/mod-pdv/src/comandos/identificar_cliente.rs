//! Identifica o cliente da venda (`F6`) — antes ou depois de bipar itens, até finalizar.
//! `docs/modulos/pdv.md` §10 ("Cliente: — não identificado (F6)").

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::carregar_cupom;
use crate::repositorio::RepositorioPdv;

/// Identifica (ou, com `None`, remove a identificação do) cliente de um cupom em andamento.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct IdentificarCliente {
    /// O cupom.
    pub cupom: Id,
    /// O cliente; `None` volta a venda para "não identificado".
    pub cliente: Option<Id>,
}

impl Comando for IdentificarCliente {
    type Saida = ();
    const PERMISSAO: &'static str = "pdv.venda.editar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut cupom = carregar_cupom(uow, self.cupom)?;

        // 2. Validar (domínio puro): só em andamento.
        cupom
            .identificar_cliente(self.cliente)
            .map_err(|e| Erro::de_dominio(&e))?;

        // 4. Persistir.
        RepositorioPdv::novo(uow).atualizar_cliente_cupom(&cupom)?;
        Ok(())
    }
}
