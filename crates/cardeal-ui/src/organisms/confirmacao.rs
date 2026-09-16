//! Camada 3 (organisms) — `dialogo_confirmacao`: o "tem certeza?" de toda ação que não dá
//! pra desfazer com um clique (excluir cliente/produto, cancelar OS). Fino sobre [`Dialogo`],
//! só padroniza mensagem + par de botões Confirmar/Cancelar — sem isso cada tela reinventava
//! o próprio modal de confirmação.

use crate::atoms::{Botao, Rotulo};

use super::Dialogo;

/// O que aconteceu neste frame no dialog de confirmação.
#[derive(Debug, Clone, Copy, Default)]
pub struct RespostaConfirmacao {
    /// O botão de confirmar foi clicado.
    pub confirmado: bool,
    /// O dialog deve fechar (confirmado, cancelado, Esc, clique fora ou ✕) — o chamador zera
    /// o seu próprio estado de "pendente de confirmação".
    pub fechar: bool,
}

/// Mostra um dialog modal de confirmação simples: título, mensagem e um par de botões
/// Confirmar/Cancelar. Chamado a cada frame enquanto a tela mantiver algo "pendente de
/// confirmação" no seu próprio estado.
pub fn dialogo_confirmacao(
    ctx: &egui::Context,
    titulo: &str,
    mensagem: &str,
    rotulo_confirmar: &str,
) -> RespostaConfirmacao {
    let mut confirmado = false;
    let mut cancelado = false;
    let fechou_pelo_dialogo = Dialogo::nova(titulo).largura(420.0).mostrar(
        ctx,
        &mut (),
        |ui, ()| {
            ui.add(Rotulo::interface(mensagem.to_owned()).quebravel());
        },
        |ui, ()| {
            if ui.add(Botao::destrutivo(rotulo_confirmar)).clicked() {
                confirmado = true;
            }
            if ui.add(Botao::secundario("Cancelar")).clicked() {
                cancelado = true;
            }
        },
    );
    RespostaConfirmacao {
        confirmado,
        fechar: confirmado || cancelado || fechou_pelo_dialogo,
    }
}
