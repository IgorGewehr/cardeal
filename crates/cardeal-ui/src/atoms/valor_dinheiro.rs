//! Camada 1 (atoms) — `ValorDinheiro`. `docs/12-ui-ux.md` §1.6 (números tabulares) e §9
//! (nenhuma informação só por cor: entrada/saída têm sinal **e** seta, não só cor — ~8% dos
//! homens têm daltonismo vermelho/verde e a paleta é vermelha).

use cardeal_kernel::Dinheiro;
use egui::{Ui, Widget};

use crate::tokens::{Papel, TemaUi};

/// Um valor monetário — cor **e** seta de sinal automáticas, nunca só a cor.
#[must_use]
pub struct ValorDinheiro {
    valor: Dinheiro,
    papel: Papel,
    com_sinal: bool,
}

impl ValorDinheiro {
    /// Tamanho de tabela (`Papel::Numero`), sem seta de sinal.
    pub const fn novo(valor: Dinheiro) -> Self {
        Self {
            valor,
            papel: Papel::Numero,
            com_sinal: false,
        }
    }

    /// Usa o papel `ValorDestaque` (Pulso) em vez do tamanho padrão de tabela.
    pub const fn destaque(mut self) -> Self {
        self.papel = Papel::ValorDestaque;
        self
    }

    /// Antepõe `▲`/`▼` ao valor, além da cor.
    pub const fn com_sinal(mut self) -> Self {
        self.com_sinal = true;
        self
    }
}

impl Widget for ValorDinheiro {
    fn ui(self, ui: &mut Ui) -> egui::Response {
        use cardeal_kernel::Sinal;

        let cores = ui.cores();
        let cor = match self.valor.sinal() {
            Sinal::Positivo => cores.positivo,
            Sinal::Negativo => cores.negativo,
            Sinal::Zero => cores.texto,
        };

        let texto = if self.com_sinal {
            let seta = match self.valor.sinal() {
                Sinal::Positivo => "\u{25B2} ",
                Sinal::Negativo => "\u{25BC} ",
                Sinal::Zero => "",
            };
            format!("{seta}{}", self.valor.formatar_com_simbolo())
        } else {
            self.valor.formatar_com_simbolo()
        };

        let mut rt = egui::RichText::new(texto)
            .font(self.papel.font_id())
            .color(cor);
        if self.papel.enfatico() {
            rt = rt.strong();
        }
        ui.label(rt)
    }
}
