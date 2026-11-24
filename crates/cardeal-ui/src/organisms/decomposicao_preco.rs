//! Camada 3 (organisms) — `DecomposicaoDePreco`: "para onde vai cada real" do preço sugerido —
//! custo, cada imposto com a frase de onde veio, operação, cartão, comissão e o lucro. A soma
//! das linhas fecha no preço (quem monta garante; o componente só mostra). Spec do markup §5.5.

use cardeal_kernel::{Dinheiro, Percentual};
use egui::Ui;

use crate::atoms::{Etiqueta, Rotulo, Tom, ValorDinheiro};
use crate::organisms::Painel;
use crate::tokens::Espaco;

/// Uma linha: o nome, o valor e a explicação.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaDecomposicao {
    /// "ICMS", "Operação"...
    pub rotulo: String,
    /// Quanto do preço.
    pub valor: Dinheiro,
    /// De onde veio (vazio = sem frase).
    pub explicacao: String,
}

/// O painel.
#[must_use]
pub struct DecomposicaoDePreco<'a> {
    preco: Dinheiro,
    custo: Dinheiro,
    lucro: Dinheiro,
    linhas: &'a [LinhaDecomposicao],
    por_fora: &'a [LinhaDecomposicao],
}

impl<'a> DecomposicaoDePreco<'a> {
    /// O preço, o custo, o lucro e as parcelas do meio.
    pub const fn novo(
        preco: Dinheiro,
        custo: Dinheiro,
        lucro: Dinheiro,
        linhas: &'a [LinhaDecomposicao],
    ) -> Self {
        Self {
            preco,
            custo,
            lucro,
            linhas,
            por_fora: &[],
        }
    }

    /// Os impostos cobrados por fora do preço (IPI, ST do substituto).
    pub const fn por_fora(mut self, por_fora: &'a [LinhaDecomposicao]) -> Self {
        self.por_fora = por_fora;
        self
    }

    /// A parte do preço (`valor ÷ preço`); zero sem preço.
    #[must_use]
    pub fn parte(&self, valor: Dinheiro) -> Percentual {
        if self.preco.e_positivo() {
            Percentual::da_razao(valor.em_centavos(), self.preco.em_centavos())
        } else {
            Percentual::ZERO
        }
    }

    /// O tom do lucro: negativo é prejuízo.
    #[must_use]
    pub fn tom_do_lucro(&self) -> Tom {
        if self.lucro.em_centavos() < 0 {
            Tom::Negativo
        } else {
            Tom::Positivo
        }
    }

    /// Desenha.
    pub fn mostrar(self, ui: &mut Ui) {
        Painel::novo().compacto().mostrar(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(Rotulo::sobrelinha("Para onde vai cada real"));
                ui.add_space(Espaco::E8);
                ui.add(ValorDinheiro::novo(self.preco).neutro());
            });
            ui.add_space(Espaco::E8);
            self.linha(ui, "Custo", self.custo, "", None);
            for l in self.linhas {
                self.linha(ui, &l.rotulo, l.valor, &l.explicacao, None);
            }
            let tom = self.tom_do_lucro();
            self.linha(ui, "Lucro", self.lucro, "", Some(tom));
            if !self.por_fora.is_empty() {
                ui.add_space(Espaco::E8);
                ui.add(Rotulo::campo("Somado ao preço, pago pelo cliente"));
                for l in self.por_fora {
                    self.linha(ui, &l.rotulo, l.valor, &l.explicacao, None);
                }
            }
        });
    }

    fn linha(
        &self,
        ui: &mut Ui,
        rotulo: &str,
        valor: Dinheiro,
        explicacao: &str,
        tom: Option<Tom>,
    ) {
        ui.horizontal(|ui| {
            match tom {
                Some(t) => {
                    ui.add(Etiqueta::nova(rotulo, t));
                }
                None => {
                    ui.add(Rotulo::campo(rotulo));
                }
            }
            ui.add_space(Espaco::E8);
            ui.add(ValorDinheiro::novo(valor).neutro());
            ui.add(Rotulo::apoio(format!("{}%", self.parte(valor).formatar(1))));
        });
        if !explicacao.is_empty() {
            ui.add(Rotulo::apoio(explicacao.to_owned()).quebravel());
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn parte_e_tom_do_lucro() {
        let linhas = [];
        let d = DecomposicaoDePreco::novo(
            Dinheiro::reais(20),
            Dinheiro::reais(15),
            Dinheiro::reais(3),
            &linhas,
        );
        assert_eq!(d.parte(Dinheiro::reais(3)), Percentual::pontos(15));
        assert_eq!(d.tom_do_lucro(), Tom::Positivo);
        let prejuizo = DecomposicaoDePreco::novo(
            Dinheiro::ZERO,
            Dinheiro::reais(15),
            Dinheiro::centavos(-100),
            &linhas,
        );
        assert_eq!(prejuizo.parte(Dinheiro::reais(1)), Percentual::ZERO);
        assert_eq!(prejuizo.tom_do_lucro(), Tom::Negativo);
    }
}
