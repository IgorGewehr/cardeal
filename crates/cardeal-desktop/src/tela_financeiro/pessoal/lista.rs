//! Os lançamentos do mês em foco e o diálogo de um lançamento (pagar, excluir).

use super::*;
use cardeal_ui::molecules::{dado, BarraFiltros};
use cardeal_ui::organisms::Dialogo;

pub(super) fn painel(ui: &mut egui::Ui, estado: &mut EstadoPessoal) {
    let hoje = Data::hoje(Fuso::BRASILIA);
    BarraFiltros::nova(&mut estado.busca)
        .marcador("Buscar por descrição, categoria ou cartão")
        .mostrar(ui);
    let termo = estado.busca.trim().to_lowercase();
    let itens: Vec<LancamentoPessoal> = estado
        .do_mes()
        .filter(|l| {
            termo.is_empty()
                || format!(
                    "{} {} {}",
                    l.descricao,
                    l.categoria,
                    l.cartao.as_deref().unwrap_or_default()
                )
                .to_lowercase()
                .contains(&termo)
        })
        .cloned()
        .collect();
    if itens.is_empty() {
        EstadoVazio::novo(Icone::Dinheiro, "Nada lançado neste mês.").mostrar(ui);
        return;
    }
    let (entra, sai) = itens
        .iter()
        .fold((Dinheiro::ZERO, Dinheiro::ZERO), |(e, s), l| match l.tipo {
            TipoPessoal::Receita => (e + l.valor, s),
            TipoPessoal::Despesa => (e, s + l.valor),
        });
    ui.add(Rotulo::campo(format!(
        "{} lançamento(s) · entram {} · saem {}",
        itens.len(),
        entra.formatar_com_simbolo(),
        sai.formatar_com_simbolo()
    )));
    ui.add_space(Espaco::E8);
    let resposta = Grade::nova(vec![
        ColunaGrade::nova("Vencimento").largura(110.0),
        ColunaGrade::nova("Descrição"),
        ColunaGrade::nova("Categoria").largura(150.0),
        ColunaGrade::nova("Parcela").largura(80.0),
        ColunaGrade::nova("Cartão").largura(120.0),
        ColunaGrade::nova("Valor").largura(140.0).numero(),
        ColunaGrade::nova("Situação").largura(110.0),
    ])
    .id_salt("pessoal-lancamentos")
    .selecionavel(None)
    .mostrar(ui, itens.len(), |i, row| {
        let l = &itens[i];
        row.col(|ui| {
            ui.add(Rotulo::interface(l.vencimento.to_string()));
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(l.descricao.clone()));
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(l.categoria.clone()).cor(ui.cores().texto_medio));
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(l.rotulo_parcela().unwrap_or_default()));
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(l.cartao.clone().unwrap_or_default()));
        });
        row.col(|ui| {
            ui.add(ValorDinheiro::novo(valor_com_sinal(l)));
        });
        row.col(|ui| {
            ui.add(etiqueta_situacao(l, hoje));
        });
    });
    if let Some(i) = resposta.linha_clicada {
        estado.dlg = DlgPessoal::Ver(itens[i].id);
    }
}

pub(super) fn dialogo_ver(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoPessoal,
    id: Id,
) {
    let Some(l) = estado.lancamentos.iter().find(|l| l.id == id).cloned() else {
        estado.dlg = DlgPessoal::Fechado;
        return;
    };
    let receita = l.tipo == TipoPessoal::Receita;
    let fechar = Dialogo::nova(l.descricao.clone()).largura(520.0).mostrar(
        ctx,
        estado,
        |ui, _| {
            ui.columns(2, |c| {
                dado(&mut c[0], "Valor", &l.valor.formatar_com_simbolo());
                dado(&mut c[1], "Vencimento", &l.vencimento.to_string());
            });
            ui.columns(2, |c| {
                dado(&mut c[0], "Categoria", &l.categoria);
                dado(
                    &mut c[1],
                    "Parcela",
                    &l.rotulo_parcela().unwrap_or_else(|| "Única".to_owned()),
                );
            });
            if let Some(cartao) = &l.cartao {
                dado(ui, "Cartão", cartao);
            }
            ui.add_space(Espaco::E8);
            ui.add(etiqueta_situacao(&l, Data::hoje(Fuso::BRASILIA)));
        },
        |ui, estado| {
            let rot = match (l.pago(), receita) {
                (true, _) => "Voltar para previsto",
                (false, true) => "Marcar recebido",
                (false, false) => "Marcar pago",
            };
            if ui.add(Botao::primario(rot)).clicked() {
                estado.dlg = DlgPessoal::Fechado;
                estado.marcar(ui.ctx(), motor, sessao, vec![l.id], !l.pago());
            }
            if l.parcelas > 1
                && l.parcela < l.parcelas
                && ui
                    .add(Botao::secundario("Excluir este e os seguintes"))
                    .clicked()
            {
                excluir(ui.ctx(), motor, sessao, estado, l.id, true);
            }
            if ui.add(Botao::secundario("Excluir só este")).clicked() {
                excluir(ui.ctx(), motor, sessao, estado, l.id, false);
            }
        },
    );
    if fechar {
        estado.dlg = DlgPessoal::Fechado;
    }
}
