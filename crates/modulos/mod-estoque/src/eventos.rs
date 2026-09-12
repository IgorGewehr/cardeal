//! Os eventos de domínio que o módulo de estoque publica na outbox
//! (`docs/modulos/estoque.md` §8). Nomes versionados — assinantes casam pelo nome, nunca
//! pelo tipo Rust.

use cardeal_kernel::{Id, Quantidade};
use cardeal_storage::EventoDominio;
use serde::Serialize;

/// O disponível de um produto, num local, cruzou abaixo do ponto de pedido depois de uma
/// saída — o gatilho de reposição (`Produto::abaixo_do_ponto`), `docs/modulos/estoque.md`
/// §8.
#[derive(Debug, Serialize)]
pub struct AbaixoPontoPedido {
    /// O produto.
    pub produto: Id,
    /// O local onde a saída ocorreu.
    pub local: Id,
    /// O disponível do local depois da saída.
    pub disponivel: Quantidade,
    /// O ponto de pedido cadastrado.
    pub ponto_pedido: Quantidade,
}

impl EventoDominio for AbaixoPontoPedido {
    const TIPO: &'static str = "estoque.abaixo_ponto_pedido.v1";

    fn agregado(&self) -> Option<Id> {
        Some(self.produto)
    }
}
