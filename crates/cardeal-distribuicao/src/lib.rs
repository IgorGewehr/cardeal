//! # cardeal-distribuicao
//!
//! A única lista de módulos que o produto liga. O núcleo nunca conhece um módulo concreto
//! (regra de ouro da arquitetura); quem conhece é a distribuição, e o desktop (monoposto) e o
//! servidor (multi-tenant, ADR-0016) leem daqui — uma empresa não pode ver um conjunto de
//! módulos diferente conforme abre pelo desktop ou pelo navegador.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]

use std::path::Path;
use std::sync::Arc;

use cardeal_kernel::{Erro, Resultado};
use cardeal_modkit::{Modulo, PedidoAtivacao, Recursos};
use cardeal_storage::Cofre;
use mod_fiscal::ProvedorFiscal;

/// A variável de ambiente com a chave do cofre em hexadecimal (64 dígitos) — o servidor a
/// recebe do gerenciador de segredos. Sem ela, a chave mora em `<dados>/cofre.chave`.
pub const VAR_CHAVE_COFRE: &str = "CARDEAL_CHAVE_SEGREDOS";

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
        &mod_fiscal::ModuloFiscal,
    ]
}

/// Os recursos que esta distribuição injeta no motor (`cardeal_modkit::Recursos`): o cofre
/// de segredos (chave de [`VAR_CHAVE_COFRE`] ou do arquivo `cofre.chave` em `pasta_dados`,
/// criado na primeira vez) e o provedor fiscal (o `sefaz-api` se `CARDEAL_FISCAL_TOKEN`
/// estiver definida; senão "não configurado").
///
/// # Errors
/// Chave do cofre inválida ou arquivo da chave ilegível.
pub fn recursos(pasta_dados: &Path) -> Resultado<Recursos> {
    let cofre = match std::env::var(VAR_CHAVE_COFRE) {
        Ok(hex) => Cofre::da_chave_hex(&hex),
        Err(_) => Cofre::do_arquivo(&pasta_dados.join("cofre.chave")),
    }
    .map_err(|e| Erro::de_dominio(&e))?;
    let provedor = ProvedorFiscal::do_ambiente();
    tracing::info!(provedor = provedor.nome(), "provedor fiscal");
    Ok(Recursos::novo()
        .com(Arc::new(cofre))
        .com(Arc::new(provedor)))
}

/// Recursos de demonstração e de teste: cofre com chave aleatória (só vale enquanto o
/// processo vive) e o fiscal simulado.
#[must_use]
pub fn recursos_simulados() -> Recursos {
    Recursos::novo()
        .com(Arc::new(Cofre::aleatorio()))
        .com(Arc::new(ProvedorFiscal::simulado(Arc::new(
            cardeal_fiscal::FiscalSimulado::novo(),
        ))))
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
