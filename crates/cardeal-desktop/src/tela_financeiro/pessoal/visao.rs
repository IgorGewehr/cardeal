//! Visão pessoal: o mês em foco em números, a projeção dos 12 meses seguintes (entradas,
//! saídas, cartão e o saldo acumulado) e para onde vai o dinheiro por categoria.

use super::*;
use cardeal_ui::atoms::Tom;
use cardeal_ui::organisms::{
    GraficoBarras, GraficoBarrasHorizontais, ItemBarraHorizontal, Painel, SerieBarras,
};
use mod_financeiro::pessoal::{despesas_por_categoria, projecao_mensal};

pub(super) fn painel(ui: &mut egui::Ui, estado: &mut EstadoPessoal) {
    if estado.lancamentos.is_empty() {
        EstadoVazio::novo(
            Icone::Dinheiro,
            "Comece lançando o que você recebe (pró-labore, salário, vendas por fora) e as \
             contas fixas (faculdade, aluguel) como \"Todo mês\"; compras parceladas no cartão \
             como \"Parcelado\". A projeção dos próximos meses aparece aqui.",
        )
        .mostrar(ui);
        return;
    }
    let mes = estado.mes();
    let meses = projecao_mensal(&estado.lancamentos, mes, 12);
    let m = meses[0];
    let tom_saldo = if m.saldo().e_negativo() {
        Tom::Negativo
    } else {
        Tom::Positivo
    };
    FaixaKpi::nova(vec![
        CartaoKpi::novo("Entradas do mês", m.receitas)
            .tom(Tom::Positivo)
            .variacao(if m.a_receber.e_zero() {
                "tudo recebido".to_owned()
            } else {
                format!("{} ainda a receber", m.a_receber.formatar_com_simbolo())
            }),
        CartaoKpi::novo("Saídas do mês", m.despesas)
            .tom(Tom::Negativo)
            .variacao(if m.a_pagar.e_zero() {
                "tudo pago".to_owned()
            } else {
                format!("{} ainda a pagar", m.a_pagar.formatar_com_simbolo())
            }),
        CartaoKpi::novo("Sobra do mês", m.saldo()).tom(tom_saldo),
        CartaoKpi::novo("Faturas de cartão", m.cartao).tom(Tom::Atencao),
    ])
    .mostrar(ui);
    ui.add_space(Espaco::E16);

    let eixo: Vec<String> = meses.iter().map(|m| mes_curto(m.competencia)).collect();
    let cores = ui.cores();
    let series = [
        SerieBarras {
            rotulo: "Entradas".to_owned(),
            cor: cores.positivo,
            valores: meses.iter().map(|m| reais(m.receitas)).collect(),
        },
        SerieBarras {
            rotulo: "Saídas".to_owned(),
            cor: cores.negativo,
            valores: meses.iter().map(|m| reais(m.despesas)).collect(),
        },
        SerieBarras {
            rotulo: "Cartão".to_owned(),
            cor: cores.atencao,
            valores: meses.iter().map(|m| reais(m.cartao)).collect(),
        },
    ];
    let categorias: Vec<ItemBarraHorizontal> = despesas_por_categoria(&estado.lancamentos, mes)
        .into_iter()
        .take(8)
        .map(|(rotulo, v)| ItemBarraHorizontal {
            rotulo,
            valor: -reais(v),
        })
        .collect();
    ui.columns(2, |c| {
        c[0].add(Rotulo::titulo_secao("Próximos 12 meses"));
        c[0].add_space(Espaco::E8);
        Painel::novo().plano().mostrar(&mut c[0], |ui| {
            GraficoBarras::novo(&eixo, &series)
                .altura(240.0)
                .mostrar(ui);
        });
        c[1].add(Rotulo::titulo_secao("Para onde vai o dinheiro no mês"));
        c[1].add_space(Espaco::E8);
        Painel::novo().plano().mostrar(&mut c[1], |ui| {
            if categorias.is_empty() {
                ui.add(Rotulo::campo("Nenhuma despesa neste mês."));
            } else {
                GraficoBarrasHorizontais::novo(&categorias).mostrar(ui);
            }
        });
    });
    ui.add_space(Espaco::E16);

    ui.add(Rotulo::titulo_secao("Projeção mês a mês"));
    ui.add(Rotulo::campo(
        "Saldo acumulado = quanto sobra (ou falta) somando mês a mês a partir do mês em foco, \
         com tudo o que já está lançado: parcelas do cartão, contas fixas e receitas.",
    ));
    ui.add_space(Espaco::E8);
    let mut acumulado = Dinheiro::ZERO;
    let acumulados: Vec<Dinheiro> = meses
        .iter()
        .map(|m| {
            acumulado += m.saldo();
            acumulado
        })
        .collect();
    let resposta = Grade::nova(vec![
        ColunaGrade::nova("Mês").largura(110.0),
        ColunaGrade::nova("Entradas").largura(140.0).numero(),
        ColunaGrade::nova("Saídas").largura(140.0).numero(),
        ColunaGrade::nova("Só cartão").largura(140.0).numero(),
        ColunaGrade::nova("Sobra do mês").largura(140.0).numero(),
        ColunaGrade::nova("Saldo acumulado").numero(),
    ])
    .id_salt("pessoal-projecao")
    .selecionavel(None)
    .mostrar(ui, meses.len(), |i, row| {
        let m = &meses[i];
        row.col(|ui| {
            ui.add(Rotulo::interface(mes_curto(m.competencia)));
        });
        row.col(|ui| {
            ui.add(ValorDinheiro::novo(m.receitas));
        });
        row.col(|ui| {
            ui.add(ValorDinheiro::novo(Dinheiro::ZERO - m.despesas));
        });
        row.col(|ui| {
            if m.cartao.e_zero() {
                ui.add(Rotulo::interface("—").cor(ui.cores().texto_fraco));
            } else {
                ui.add(ValorDinheiro::novo(m.cartao).neutro());
            }
        });
        row.col(|ui| {
            ui.add(ValorDinheiro::novo(m.saldo()));
        });
        row.col(|ui| {
            ui.add(ValorDinheiro::novo(acumulados[i]));
        });
    });
    // Clique num mês: abre os lançamentos dele.
    if let Some(i) = resposta.linha_clicada {
        estado.mes = Some(meses[i].competencia);
        estado.aba = AbaPessoal::Lancamentos;
    }
}
