//! Testes ponta a ponta da tela de Financeiro: motor real (SQLite temporário), admin logado.

use super::*;
use crate::testes_comum::motor_de_teste;

fn lancar_a_receber(motor: &MotorLocal, sessao: &SessaoLocal, reais: i64) -> Id {
    let hoje = Data::hoje(Fuso::BRASILIA);
    let r: mod_financeiro::TituloAReceberLancado = motor
        .executar(
            sessao,
            "financeiro.lancar_titulo_a_receber.v1",
            &LancarTituloAReceber {
                cliente: None,
                valor_total: Dinheiro::reais(reais),
                emissao: hoje,
                parcelas: 1,
                primeiro_vencimento: hoje,
                intervalo_dias: 0,
                observacao: None,
                categoria: None,
            },
        )
        .expect("título");
    r.parcelas[0]
}

#[test]
fn baixa_vai_para_a_parcela_aberta_mesmo_se_a_lista_mudar_de_ordem() {
    // O diálogo guardava a *posição* da parcela no vetor; reordenar a grade com ele
    // aberto fazia a baixa cair na parcela errada.
    let t = motor_de_teste();
    let _a = lancar_a_receber(&t.motor, &t.sessao, 100);
    let b = lancar_a_receber(&t.motor, &t.sessao, 250);
    let mut estado = EstadoTelaFinanceiro {
        aba: Aba::Receber,
        ..Default::default()
    };
    estado.carregar(&t.motor, &t.sessao);
    assert!(estado.erro.is_none(), "{:?}", estado.erro);
    assert_eq!(estado.parcelas.len(), 2);

    let p = estado
        .parcelas
        .iter()
        .find(|p| p.parcela == b)
        .cloned()
        .expect("parcela b");
    estado.dlg = Dlg::Baixar {
        parcela: b,
        valor: p.saldo().formatar(),
        data: Data::hoje(Fuso::BRASILIA).to_string(),
        pagamento: crate::pagamento::EstadoPagamento::novo(MeioPagamento::Dinheiro),
        baixas: Vec::new(),
        baixas_carregadas: true,
        motivo_estorno: String::new(),
    };
    estado.parcelas.reverse();

    // O diálogo continua na parcela B, e a baixa sai nela.
    let p = parcela_do_dialogo(&estado).expect("parcela do diálogo");
    assert_eq!(p.parcela, b);
    baixar(
        &egui::Context::default(),
        &t.motor,
        &t.sessao,
        &mut estado,
        &p,
    );

    let abertas: Vec<ItemTituloEmAberto> = t
        .motor
        .consultar(
            &t.sessao,
            "financeiro.titulos_a_receber_em_aberto.v1",
            &TitulosAReceberEmAberto,
        )
        .expect("em aberto");
    assert_eq!(abertas.len(), 1);
    assert_eq!(abertas[0].valor_original, Dinheiro::reais(100));
}
