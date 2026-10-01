//! Cartões de crédito: a fatura de cada cartão no mês em foco (o que compõe, quanto falta
//! pagar, "marcar fatura paga") e quanto ainda está comprometido nos meses seguintes pelas
//! compras parceladas.

use super::*;
use cardeal_ui::organisms::Painel;
use mod_financeiro::pessoal::faturas_do_mes;

pub(super) fn painel(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoPessoal,
) {
    let mes = estado.mes();
    let faturas = faturas_do_mes(&estado.lancamentos, mes);
    if faturas.is_empty() {
        EstadoVazio::novo(
            Icone::Dinheiro,
            "Nenhuma fatura neste mês. Ao lançar uma despesa, informe o cartão: compras \
             parceladas entram mês a mês na fatura certa.",
        )
        .mostrar(ui);
        return;
    }
    let total = faturas.iter().fold(Dinheiro::ZERO, |a, f| a + f.1);
    let aberto = faturas.iter().fold(Dinheiro::ZERO, |a, f| a + f.2);
    let futuro = comprometido_depois(&estado.lancamentos, mes, None);
    FaixaKpi::nova(vec![
        CartaoKpi::novo("Faturas do mês", total).tom(cardeal_ui::atoms::Tom::Atencao),
        CartaoKpi::novo("Ainda a pagar", aberto).tom(cardeal_ui::atoms::Tom::Negativo),
        CartaoKpi::novo("Já comprometido nos meses seguintes", futuro)
            .tom(cardeal_ui::atoms::Tom::Neutro),
    ])
    .mostrar(ui);
    ui.add_space(Espaco::E16);

    let hoje = Data::hoje(Fuso::BRASILIA);
    let mut pagar: Option<Vec<Id>> = None;
    for (cartao, total, aberto) in &faturas {
        let itens: Vec<LancamentoPessoal> = estado
            .do_mes()
            .filter(|l| l.cartao.as_deref() == Some(cartao.as_str()))
            .cloned()
            .collect();
        let depois = comprometido_depois(&estado.lancamentos, mes, Some(cartao));
        ui.horizontal(|ui| {
            ui.add(Rotulo::titulo_secao(cartao.clone()));
            ui.add(Rotulo::campo(format!(
                "fatura {} · {}",
                total.formatar_com_simbolo(),
                if aberto.e_zero() {
                    "paga".to_owned()
                } else {
                    format!("falta {}", aberto.formatar_com_simbolo())
                }
            )));
            if !aberto.e_zero()
                && ui
                    .add(Botao::primario("Marcar fatura paga").pequeno())
                    .clicked()
            {
                pagar = Some(itens.iter().filter(|l| !l.pago()).map(|l| l.id).collect());
            }
        });
        if depois.e_positivo() {
            ui.add(Rotulo::campo(format!(
                "Depois deste mês ainda há {} em parcelas neste cartão.",
                depois.formatar_com_simbolo()
            )));
        }
        ui.add_space(Espaco::E8);
        // Uma grade por cartão: cada uma no seu escopo de id.
        ui.push_id(cartao.as_str(), |ui| {
            Painel::novo().plano().mostrar(ui, |ui| {
                let resposta = Grade::nova(vec![
                    ColunaGrade::nova("Compra"),
                    ColunaGrade::nova("Categoria").largura(150.0),
                    ColunaGrade::nova("Parcela").largura(90.0),
                    ColunaGrade::nova("Valor").largura(140.0).numero(),
                    ColunaGrade::nova("Situação").largura(110.0),
                ])
                .selecionavel(None)
                .mostrar(ui, itens.len(), |i, row| {
                    let l = &itens[i];
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
                        ui.add(ValorDinheiro::novo(l.valor).neutro());
                    });
                    row.col(|ui| {
                        ui.add(etiqueta_situacao(l, hoje));
                    });
                });
                if let Some(i) = resposta.linha_clicada {
                    estado.dlg = DlgPessoal::Ver(itens[i].id);
                }
            })
        });
        ui.add_space(Espaco::E16);
    }
    if let Some(ids) = pagar {
        estado.marcar(ui.ctx(), motor, sessao, ids, true);
    }
}

/// O que ainda cai no cartão (ou em todos) depois do mês dado.
fn comprometido_depois(
    lancamentos: &[LancamentoPessoal],
    mes: Competencia,
    cartao: Option<&str>,
) -> Dinheiro {
    lancamentos
        .iter()
        .filter(|l| {
            l.tipo == TipoPessoal::Despesa
                && l.vencimento.competencia() > mes
                && l.cartao.is_some()
                && cartao.is_none_or(|c| l.cartao.as_deref() == Some(c))
        })
        .fold(Dinheiro::ZERO, |a, l| a + l.valor)
}
