//! Camada 2 (molecules) — `Dado`: um par rótulo/valor **somente leitura**, o átomo de todo
//! diálogo de consulta ("Vencimento — 14/09/2026").
//!
//! Existia como uma função `kv` copiada em oito telas, com pequenas variações de espaçamento
//! (0, 8 ou 12px) que só apareciam quando duas telas eram postas lado a lado. Valor vazio
//! mostra "—", nunca um buraco.

use egui::Ui;

use crate::atoms::Rotulo;
use crate::tokens::Espaco;

/// Um par rótulo/valor de leitura.
#[must_use]
pub struct Dado {
    rotulo: String,
    valor: String,
    em_linha: bool,
}

impl Dado {
    /// Um dado empilhado: rótulo pequeno em cima, valor embaixo.
    pub fn novo(rotulo: impl Into<String>, valor: impl Into<String>) -> Self {
        Self {
            rotulo: rotulo.into(),
            valor: valor.into(),
            em_linha: false,
        }
    }

    /// Rótulo e valor na mesma linha ("Cliente: Maira"); o valor quebra se for longo. Para
    /// resumos densos, onde empilhar dobraria a altura.
    pub const fn em_linha(mut self) -> Self {
        self.em_linha = true;
        self
    }

    /// Desenha o dado no `ui` corrente.
    pub fn mostrar(self, ui: &mut Ui) {
        let valor = if self.valor.trim().is_empty() {
            "—".to_owned()
        } else {
            self.valor
        };
        if self.em_linha {
            ui.horizontal(|ui| {
                ui.add(Rotulo::campo(format!("{}: ", self.rotulo)));
                ui.add(Rotulo::interface(valor).quebravel());
            });
        } else {
            ui.add(Rotulo::campo(self.rotulo));
            ui.add(Rotulo::interface(valor));
            ui.add_space(Espaco::E8);
        }
    }
}

/// Atalho de [`Dado::novo`]`(..).mostrar(ui)` — o uso de 95% dos diálogos.
pub fn dado(ui: &mut Ui, rotulo: &str, valor: &str) {
    Dado::novo(rotulo, valor).mostrar(ui);
}

/// Atalho de [`Dado::novo`]`(..).em_linha().mostrar(ui)`.
pub fn dado_em_linha(ui: &mut Ui, rotulo: &str, valor: &str) {
    Dado::novo(rotulo, valor).em_linha().mostrar(ui);
}
