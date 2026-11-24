//! A conta do preço sugerido (`cardeal_tributos::PrecoResolvido`) nas linhas do componente
//! `DecomposicaoDePreco` — usada na aba Preços e na peça da OS.

use cardeal_tributos::{ParcelaDoPreco, PrecoResolvido};
use cardeal_ui::organisms::{DecomposicaoDePreco, LinhaDecomposicao};
use eframe::egui;

fn linhas(p: &[ParcelaDoPreco]) -> Vec<LinhaDecomposicao> {
    p.iter()
        .map(|x| LinhaDecomposicao {
            rotulo: x.rotulo.clone(),
            valor: x.valor,
            explicacao: x.explicacao.clone(),
        })
        .collect()
}

/// Desenha "para onde vai cada real" do preço resolvido.
pub fn mostrar(ui: &mut egui::Ui, r: &PrecoResolvido) {
    let (dentro, fora) = (linhas(&r.parcelas), linhas(&r.por_fora));
    DecomposicaoDePreco::novo(r.preco, r.custo, r.lucro, &dentro)
        .por_fora(&fora)
        .mostrar(ui);
}
