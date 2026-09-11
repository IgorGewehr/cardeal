//! Camada 3 (organisms) — `FaixaKpi`: uma fileira de [`CartaoKpi`](crate::molecules::CartaoKpi)
//! em largura igual, com o espaçamento padrão entre eles.
//!
//! Nasce da revisão de UI/UX de 2026-09-11: várias telas de listagem já calculam (ou podiam
//! calcular de graça, a partir da própria lista carregada) um agregado que hoje só aparece
//! perdido numa tabela — total de OS em aberto, valor parado, contagem de clientes. Isso
//! também usa a largura da tela que as capturas de tela mostraram sobrando em branco acima
//! da lista.

use egui::Ui;

use crate::molecules::CartaoKpi;
use crate::tokens::Espaco;

/// Uma fileira de cartões de indicador, todos com a mesma largura.
#[must_use]
pub struct FaixaKpi {
    cartoes: Vec<CartaoKpi>,
}

impl FaixaKpi {
    /// Uma fileira com os cartões dados (na ordem).
    pub const fn nova(cartoes: Vec<CartaoKpi>) -> Self {
        Self { cartoes }
    }

    /// Desenha a fileira; nada é desenhado se a lista de cartões estiver vazia (nunca uma
    /// fileira de colunas em branco).
    pub fn mostrar(self, ui: &mut Ui) {
        if self.cartoes.is_empty() {
            return;
        }
        let n = self.cartoes.len();
        ui.columns(n, |colunas| {
            for (coluna, cartao) in colunas.iter_mut().zip(self.cartoes) {
                coluna.add(cartao);
            }
        });
        ui.add_space(Espaco::E8);
    }
}
