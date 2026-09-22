//! Camada 1 (atoms) — `ValorDinheiro`. `docs/12-ui-ux.md` §1.6 (números tabulares) e §9
//! (nenhuma informação só por cor: entrada/saída têm sinal **e** seta, não só cor — ~8% dos
//! homens têm daltonismo vermelho/verde e a paleta é vermelha).

use cardeal_kernel::Dinheiro;
use egui::{Ui, Widget};

use crate::tokens::{Papel, TemaUi};

/// De onde vem a cor do valor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Coloracao {
    /// Verde/vermelho/neutro conforme o sinal (o padrão).
    PorSinal,
    /// Sempre a cor de texto — o valor é uma medida, não um ganho ou uma perda.
    Neutra,
    /// Texto fraco — um valor que existe mas não é o foco (saldo zerado, por exemplo).
    Esmaecida,
}

/// Um valor monetário — cor **e** seta de sinal automáticas, nunca só a cor.
#[must_use]
pub struct ValorDinheiro {
    valor: Dinheiro,
    papel: Papel,
    com_sinal: bool,
    coloracao: Coloracao,
}

impl ValorDinheiro {
    /// Tamanho de tabela (`Papel::Numero`), sem seta de sinal.
    pub const fn novo(valor: Dinheiro) -> Self {
        Self {
            valor,
            papel: Papel::Numero,
            com_sinal: false,
            coloracao: Coloracao::PorSinal,
        }
    }

    /// Não pinta pelo sinal: para colunas de medida ("Valor", "Saldo") em que verde
    /// significaria "entrou dinheiro" sem que nada tenha entrado.
    pub const fn neutro(mut self) -> Self {
        self.coloracao = Coloracao::Neutra;
        self
    }

    /// Texto fraco, para o que está na linha mas não é o foco dela.
    pub const fn esmaecido(mut self) -> Self {
        self.coloracao = Coloracao::Esmaecida;
        self
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
        let cor = match (self.coloracao, self.valor.sinal()) {
            (Coloracao::Esmaecida, _) => cores.texto_fraco,
            (Coloracao::Neutra, _) | (Coloracao::PorSinal, Sinal::Zero) => cores.texto,
            (Coloracao::PorSinal, Sinal::Positivo) => cores.positivo,
            (Coloracao::PorSinal, Sinal::Negativo) => cores.negativo,
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
