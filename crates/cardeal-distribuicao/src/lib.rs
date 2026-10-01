//! # cardeal-distribuicao
//!
//! A única lista de módulos que o produto liga. O núcleo nunca conhece um módulo concreto
//! (regra de ouro da arquitetura); quem conhece é a distribuição, e o desktop (monoposto) e o
//! servidor (multi-tenant, ADR-0016) leem daqui — uma empresa não pode ver um conjunto de
//! módulos diferente conforme abre pelo desktop ou pelo navegador.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]

use cardeal_modkit::{Modulo, PedidoAtivacao};

/// Os módulos compilados nesta distribuição, na ordem em que aparecem no menu.
#[must_use]
pub fn modulos() -> Vec<&'static dyn Modulo> {
    vec![
        &mod_empresa::ModuloEmpresa,
        &mod_financeiro::ModuloFinanceiro,
        &mod_clientes::ModuloClientes,
        &mod_estoque::ModuloEstoque,
        &mod_compras::ModuloCompras,
        &mod_vendas::ModuloVendas,
        &mod_os::ModuloOs,
        &mod_orcamentos::ModuloOrcamentos,
        &mod_agenda::ModuloAgenda,
        &mod_pdv::ModuloPdv,
    ]
}

/// Liga todos os módulos **e todos os seus submódulos** — as telas usam funcionalidade de
/// submódulos não-essenciais (contas a pagar, contas bancárias, fluxo, recurso de agenda, laudo
/// de OS, limite de crédito...) e sem isso as permissões correspondentes nem entram no
/// catálogo, travando o admin com "Sem permissão para ...".
#[must_use]
pub fn pedido_ativacao() -> PedidoAtivacao {
    let mut pedido = PedidoAtivacao::nova();
    for m in modulos() {
        let manifesto = m.manifesto();
        let id = manifesto.id.como_str();
        pedido = pedido.com_modulo(id);
        for sub in manifesto.submodulos {
            pedido = pedido.com_submodulo(id, sub.id);
        }
    }
    pedido
}
