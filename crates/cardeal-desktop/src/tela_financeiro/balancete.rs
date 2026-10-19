//! O balancete de verificação do mês, na aba Resultado e caixa: cada conta com saldo anterior,
//! débitos, créditos e saldo atual (D/C), as agrupadoras somadas, e a prova de que fecha.

use super::*;
use cardeal_ledger::LinhaBalancete;

/// "1.234,56 D" / "1.234,56 C" / "—".
pub(super) fn com_natureza(v: Dinheiro) -> String {
    if v.e_zero() {
        "—".to_owned()
    } else if v.e_positivo() {
        format!("{} D", v.formatar())
    } else {
        format!("{} C", v.abs().formatar())
    }
}

/// Os totais das contas que recebem lançamento: (anterior, débitos, créditos, atual).
pub(super) fn totais(linhas: &[LinhaBalancete]) -> (Dinheiro, Dinheiro, Dinheiro, Dinheiro) {
    linhas.iter().filter(|l| !l.sintetica).fold(
        (
            Dinheiro::ZERO,
            Dinheiro::ZERO,
            Dinheiro::ZERO,
            Dinheiro::ZERO,
        ),
        |(a, d, c, s), l| {
            (
                a + l.saldo_anterior,
                d + l.debitos,
                c + l.creditos,
                s + l.saldo_atual(),
            )
        },
    )
}

pub(super) fn tabela_balancete(ui: &mut egui::Ui, linhas: &[LinhaBalancete]) {
    if linhas.is_empty() {
        EstadoVazio::novo(Icone::Dinheiro, "Nenhum lançamento até este mês.").mostrar(ui);
        return;
    }
    let (_, debitos, creditos, _) = totais(linhas);
    ui.add(
        Rotulo::apoio(
            "Pela competência de cada lançamento. As contas de resultado acumulam só o ano; o \
             resultado dos anos anteriores aparece no patrimônio líquido, porque o sistema não \
             encerra o exercício. D = saldo devedor, C = credor.",
        )
        .quebravel(),
    );
    ui.add_space(Espaco::E4);
    ui.add(if debitos == creditos {
        Etiqueta::positiva(format!(
            "Fecha: débitos e créditos de {} no mês",
            debitos.formatar_com_simbolo()
        ))
    } else {
        Etiqueta::negativa("Débitos e créditos não batem — avise o suporte")
    });
    ui.add_space(Espaco::E8);
    Grade::nova(vec![
        ColunaGrade::nova("Conta").largura(360.0),
        ColunaGrade::nova("Saldo anterior").largura(150.0).numero(),
        ColunaGrade::nova("Débitos").largura(130.0).numero(),
        ColunaGrade::nova("Créditos").largura(130.0).numero(),
        ColunaGrade::nova("Saldo atual").largura(150.0).numero(),
    ])
    .id_salt("balancete")
    .mostrar(ui, linhas.len(), |i, row| {
        let l = &linhas[i];
        let texto = |ui: &mut egui::Ui, t: String| {
            if l.sintetica {
                ui.add(Rotulo::campo(t));
            } else {
                ui.add(Rotulo::interface(t));
            }
        };
        row.col(|ui| {
            ui.horizontal(|ui| {
                ui.add_space(f32::from(l.nivel.saturating_sub(1)) * Espaco::E12);
                let rotulo = if l.codigo.is_empty() {
                    l.nome.clone()
                } else {
                    format!("{}  {}", l.codigo, l.nome)
                };
                texto(ui, rotulo);
            });
        });
        row.col(|ui| texto(ui, com_natureza(l.saldo_anterior)));
        row.col(|ui| texto(ui, com_natureza(l.debitos)));
        row.col(|ui| texto(ui, com_natureza(-l.creditos)));
        row.col(|ui| texto(ui, com_natureza(l.saldo_atual())));
    });
}
