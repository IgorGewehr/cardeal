//! Camada 3 (organisms) — `PainelMargem`: o resumo de rentabilidade de um pedido, orçamento
//! ou OS — receita, custo, margem bruta e líquida — com um semáforo que compara a margem
//! líquida com o mínimo que a empresa aceita. `docs/12-ui-ux.md` §7.
//!
//! O semáforo sempre vem com texto ("prejuízo", "abaixo do mínimo", "margem saudável"): a cor
//! nunca é a única informação (§9).

use cardeal_kernel::{Dinheiro, Percentual};
use egui::Ui;

use crate::atoms::{Etiqueta, Rotulo, Tom, ValorDinheiro};
use crate::organisms::Painel;
use crate::tokens::Espaco;

/// Os números do painel.
#[must_use]
pub struct PainelMargem {
    receita: Dinheiro,
    custo: Dinheiro,
    margem_bruta: Dinheiro,
    margem_liquida: Dinheiro,
    percentual: Option<Percentual>,
    minimo: Percentual,
    itens_sem_custo: u32,
}

impl PainelMargem {
    /// O painel com receita, custo, margens e o percentual líquido (`None` = nenhum item com
    /// custo conhecido).
    pub const fn novo(
        receita: Dinheiro,
        custo: Dinheiro,
        margem_bruta: Dinheiro,
        margem_liquida: Dinheiro,
        percentual: Option<Percentual>,
    ) -> Self {
        Self {
            receita,
            custo,
            margem_bruta,
            margem_liquida,
            percentual,
            minimo: Percentual::ZERO,
            itens_sem_custo: 0,
        }
    }

    /// A margem líquida mínima aceita (abaixo dela o painel avisa).
    pub const fn minimo(mut self, minimo: Percentual) -> Self {
        self.minimo = minimo;
        self
    }

    /// Quantos itens ficaram de fora por falta de custo.
    pub const fn itens_sem_custo(mut self, n: u32) -> Self {
        self.itens_sem_custo = n;
        self
    }

    /// O tom e o texto do semáforo.
    #[must_use]
    pub fn situacao(&self) -> (Tom, &'static str) {
        match self.percentual {
            None => (Tom::Neutro, "sem custo conhecido"),
            Some(p) if p.unidades_internas() < 0 => (Tom::Negativo, "prejuízo"),
            Some(p) if p < self.minimo => (Tom::Atencao, "abaixo do mínimo"),
            Some(_) => (Tom::Positivo, "margem saudável"),
        }
    }

    /// Desenha o painel.
    pub fn mostrar(self, ui: &mut Ui) {
        let (tom, texto) = self.situacao();
        Painel::novo().compacto().realce(tom).mostrar(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(Rotulo::sobrelinha("Margem"));
                ui.add_space(Espaco::E8);
                ui.add(Etiqueta::nova(texto, tom));
                if let Some(p) = self.percentual {
                    ui.add(Rotulo::titulo_secao(format!("{}%", p.formatar(1))));
                }
            });
            ui.add_space(Espaco::E8);
            ui.columns(4, |c| {
                coluna(&mut c[0], "Receita", self.receita);
                coluna(&mut c[1], "Custo", self.custo);
                coluna(&mut c[2], "Margem bruta", self.margem_bruta);
                coluna(&mut c[3], "Margem líquida", self.margem_liquida);
            });
            if self.itens_sem_custo > 0 {
                ui.add_space(Espaco::E4);
                ui.add(Rotulo::apoio(format!(
                    "{} item(ns) sem custo conhecido ficaram fora do cálculo.",
                    self.itens_sem_custo
                )));
            }
        });
    }
}

fn coluna(ui: &mut Ui, rotulo: &str, valor: Dinheiro) {
    ui.add(Rotulo::campo(rotulo));
    ui.add(ValorDinheiro::novo(valor).neutro());
}

#[cfg(test)]
mod testes {
    use super::*;

    fn painel(pct: Option<i64>, minimo: i64) -> PainelMargem {
        PainelMargem::novo(
            Dinheiro::reais(100),
            Dinheiro::reais(50),
            Dinheiro::reais(50),
            Dinheiro::reais(20),
            pct.map(Percentual::pontos),
        )
        .minimo(Percentual::pontos(minimo))
    }

    #[test]
    fn semaforo_compara_a_margem_com_o_minimo() {
        assert_eq!(painel(None, 10).situacao().0, Tom::Neutro);
        assert_eq!(painel(Some(-5), 10).situacao(), (Tom::Negativo, "prejuízo"));
        assert_eq!(
            painel(Some(8), 10).situacao(),
            (Tom::Atencao, "abaixo do mínimo")
        );
        assert_eq!(painel(Some(10), 10).situacao().0, Tom::Positivo);
        // Sem mínimo configurado, qualquer lucro é saudável; só o prejuízo avisa.
        assert_eq!(painel(Some(1), 0).situacao().0, Tom::Positivo);
        assert_eq!(painel(Some(0), 0).situacao().0, Tom::Positivo);
    }
}
