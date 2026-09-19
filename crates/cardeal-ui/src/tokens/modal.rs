//! Camada 0 (ions) — "há um modal aberto?": o estado que atalhos globais precisam consultar.
//!
//! Um atalho de tela (`Ctrl+N` → "novo") não pode disparar enquanto um diálogo está aberto por
//! cima: reabriria o formulário e apagaria o que o usuário estava digitando. O `Dialogo` se
//! marca a cada quadro em que é desenhado; [`modal_aberto`] olha esse marcador.
//!
//! O marcador é o número do quadro, não um booleano: o diálogo é desenhado **depois** do
//! cabeçalho da tela no mesmo quadro, então quem pergunta só o enxerga pelo quadro anterior.
//! Um modal que fecha deixa de se marcar e, no quadro seguinte, o atalho volta a valer.

use egui::{Context, Id};

fn chave() -> Id {
    Id::new("cardeal-ui/modal-aberto")
}

/// Marca que um modal foi desenhado neste quadro. Chamado por `Dialogo::mostrar`.
pub fn marcar_modal(ctx: &Context) {
    let quadro = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| d.insert_temp(chave(), quadro));
}

/// Se um modal foi desenhado neste quadro ou no anterior.
#[must_use]
pub fn modal_aberto(ctx: &Context) -> bool {
    let agora = ctx.cumulative_pass_nr();
    ctx.data(|d| d.get_temp::<u64>(chave()))
        .is_some_and(|q| q.saturating_add(1) >= agora)
}
