//! Testes ponta a ponta do financeiro pessoal: motor real, admin logado.

use super::lancar::{lancar, FormPessoal};
use super::*;
use crate::testes_comum::motor_de_teste;
use mod_financeiro::pessoal::{faturas_do_mes, projecao_mensal};

#[test]
fn cartao_parcelado_e_pro_labore_viram_projecao_e_fatura() {
    let t = motor_de_teste();
    let ctx = egui::Context::default();
    let mut estado = EstadoPessoal::default();
    estado.carregar(&t.motor, &t.sessao);
    assert!(estado.erro.is_none(), "{:?}", estado.erro);

    let mut f = FormPessoal::novo(TipoPessoal::Despesa);
    f.preencher("Parcelamentos anteriores", "800,00", "Compras", "Nubank");
    f.mensal(10);
    estado.dlg = DlgPessoal::Novo(Box::new(f));
    lancar(&ctx, &t.motor, &t.sessao, &mut estado, true);
    // "Lançar e continuar": o formulário volta limpo.
    assert!(matches!(&estado.dlg, DlgPessoal::Novo(f) if f.vazio()));

    let mut f = FormPessoal::novo(TipoPessoal::Receita);
    f.preencher("Pró-labore", "5000,00", "Pró-labore", "");
    f.mensal(12);
    estado.dlg = DlgPessoal::Novo(Box::new(f));
    lancar(&ctx, &t.motor, &t.sessao, &mut estado, false);
    assert!(matches!(estado.dlg, DlgPessoal::Fechado));

    let mes = estado.mes();
    let p = projecao_mensal(&estado.lancamentos, mes, 12);
    assert_eq!(p[0].cartao, Dinheiro::reais(800));
    assert_eq!(p[0].saldo(), Dinheiro::reais(4200));
    assert!(p[10].cartao.e_zero(), "o parcelamento acaba no 10º mês");

    // Marcar a fatura do mês como paga.
    let ids: Vec<Id> = estado
        .do_mes()
        .filter(|l| l.cartao.is_some())
        .map(|l| l.id)
        .collect();
    estado.marcar(&ctx, &t.motor, &t.sessao, ids, true);
    let f = faturas_do_mes(&estado.lancamentos, mes);
    assert_eq!(
        f,
        vec![("Nubank".to_owned(), Dinheiro::reais(800), Dinheiro::ZERO)]
    );
}
