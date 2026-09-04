//! Os comandos do financeiro (`docs/modulos/financeiro.md` §5). Um arquivo, um comando,
//! sempre nas cinco etapas de `docs/15-convencoes-codigo.md` §3: **carregar, validar, lançar
//! no razão, persistir, publicar**.
//!
//! `LancarTitulo`/`BaixarParcela` são declarados em pares por espécie, porque a permissão
//! difere (`financeiro.receber.criar` × `financeiro.pagar.criar`) e um `Comando` tem uma só
//! `PERMISSAO`. Este lote traz o lado **a receber**; o lado a pagar é o espelho.

mod baixar_recebimento;
mod lancar_titulo_a_receber;

pub use baixar_recebimento::{BaixarRecebimento, RecebimentoBaixado};
pub use lancar_titulo_a_receber::{LancarTituloAReceber, TituloAReceberLancado};

use cardeal_modkit::Ctx;

use crate::receituario::Autoria;

/// Extrai a [`Autoria`] de um [`Ctx`] de comando.
fn autoria_de(ctx: &Ctx) -> Autoria {
    Autoria {
        usuario: ctx.usuario,
        dispositivo: ctx.dispositivo,
        agora: ctx.agora,
        fuso: ctx.fuso,
    }
}
