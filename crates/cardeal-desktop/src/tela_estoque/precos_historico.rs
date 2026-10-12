//! O diálogo do histórico das últimas compras de um produto, aberto pela aba Preços: o que o
//! fornecedor cobrou a cada entrada, quanto variou e a média das três últimas.

use super::precos::EstadoPrecos;
use super::*;
use cardeal_kernel::Percentual;
use cardeal_ui::atoms::ValorDinheiro;
use mod_estoque::historico_compras::ResumoDeCompras;
use mod_estoque::HistoricoDeCompras;

/// Carrega o histórico do produto; `false` (e nada abre) se a consulta falhar.
pub(super) fn abrir_historico(
    motor: &Motor,
    sessao: &Sessao,
    prc: &mut EstadoPrecos,
    produto: Id,
    nome: String,
) -> bool {
    let r: Result<ResumoDeCompras, _> = motor.consultar(
        sessao,
        "estoque.historico_de_compras.v1",
        &HistoricoDeCompras { produto },
    );
    match r {
        Ok(r) => {
            prc.historico = Some((nome, r));
            true
        }
        Err(_) => false,
    }
}

fn texto_variacao(v: Percentual) -> String {
    let sinal = if v.unidades_internas() > 0 { "+" } else { "" };
    format!("{sinal}{}%", v.formatar(2))
}

pub(super) fn dialogo(ctx: &egui::Context, estado: &mut EstadoTelaEstoque) {
    let titulo = estado
        .prc
        .historico
        .as_ref()
        .map_or_else(String::new, |(n, _)| format!("Compras de {n}"));
    let fechar = Dialogo::nova(&titulo).largura(620.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            let Some((_, h)) = &estado.prc.historico else {
                return;
            };
            if h.compras.is_empty() {
                ui.add(Rotulo::interface(
                    "Este produto ainda não teve compra com custo registrado.",
                ));
                return;
            }
            FaixaKpi::nova(vec![
                CartaoKpi::novo("Último custo", h.ultimo_custo.unwrap_or_default())
                    .tom(Tom::Neutro),
                CartaoKpi::novo("Média das últimas 3", h.media_ultimas_3.unwrap_or_default())
                    .tom(Tom::Neutro),
            ])
            .mostrar(ui);
            ui.add_space(Espaco::E12);
            Grade::nova(vec![
                ColunaGrade::nova("Data").largura(150.0),
                ColunaGrade::nova("Quantidade").largura(110.0).numero(),
                ColunaGrade::nova("Custo").largura(110.0).numero(),
                ColunaGrade::nova("Variação").largura(110.0).numero(),
            ])
            .id_salt("historico-compras")
            .mostrar(ui, h.compras.len(), |i, row| {
                let c = &h.compras[i];
                row.col(|ui| {
                    ui.add(Rotulo::interface(c.quando.formatar(Fuso::BRASILIA)));
                });
                row.col(|ui| {
                    ui.add(Rotulo::interface(c.quantidade.to_string()));
                });
                row.col(|ui| {
                    ui.add(ValorDinheiro::novo(c.custo_unitario).neutro());
                });
                row.col(|ui| match c.variacao {
                    Some(v) if v.unidades_internas() > 0 => {
                        ui.add(Etiqueta::negativa(texto_variacao(v)));
                    }
                    Some(v) if v.unidades_internas() < 0 => {
                        ui.add(Etiqueta::positiva(texto_variacao(v)));
                    }
                    Some(v) => {
                        ui.add(Rotulo::interface(texto_variacao(v)));
                    }
                    None => {
                        ui.add(Rotulo::interface("—").cor(ui.cores().texto_fraco));
                    }
                });
            });
        },
        |ui, estado| {
            if ui.add(Botao::secundario("Fechar")).clicked() {
                estado.dlg = Dlg::Fechado;
            }
        },
    );
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}
