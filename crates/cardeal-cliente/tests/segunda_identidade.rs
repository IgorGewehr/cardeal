//! Prova o que a tela de PDV usa para a **segunda identidade** do `F8` (cancelar cupom,
//! `docs/modulos/pdv.md` §11 regra 3): autenticar um supervisor de verdade e conferir que ele
//! tem a permissão antes de agir em nome dele.

use cardeal_cliente::MotorLocal;
use cardeal_modkit::{Modulo, PedidoAtivacao};

fn modulos() -> Vec<&'static dyn Modulo> {
    vec![
        &mod_financeiro::ModuloFinanceiro,
        &mod_clientes::ModuloClientes,
        &mod_estoque::ModuloEstoque,
        &mod_vendas::ModuloVendas,
        &mod_pdv::ModuloPdv,
    ]
}

fn pedido() -> PedidoAtivacao {
    let mut pedido = PedidoAtivacao::nova();
    for m in modulos() {
        let manifesto = m.manifesto();
        let id = manifesto.id.como_str();
        pedido = pedido.com_modulo(id);
        for sub in manifesto.submodulos {
            pedido = pedido.com_submodulo(id, sub.id);
        }
    }
    pedido
}

fn motor_com_admin(arquivo: &tempfile::NamedTempFile) -> MotorLocal {
    let motor = MotorLocal::abrir(arquivo.path(), &modulos(), &pedido()).expect("abrir motor");
    motor
        .configurar_inicial(
            "Padaria do Igor Ltda",
            "11.222.333/0001-81",
            "supervisor",
            "Igor Gewehr",
            "senha-forte-123",
        )
        .expect("configurar_inicial");
    motor
}

#[test]
fn supervisor_com_senha_certa_e_a_permissao_autoriza() {
    let arquivo = tempfile::NamedTempFile::new().unwrap();
    let motor = motor_com_admin(&arquivo);
    let sessao = motor
        .autenticar("supervisor", "senha-forte-123")
        .expect("login do supervisor");
    assert!(
        sessao.concede("pdv.cupom.cancelar"),
        "o administrador tem que poder autorizar o cancelamento de cupom"
    );
}

#[test]
fn senha_errada_nao_autentica() {
    let arquivo = tempfile::NamedTempFile::new().unwrap();
    let motor = motor_com_admin(&arquivo);
    assert!(motor.autenticar("supervisor", "senha-errada").is_err());
    assert!(motor.autenticar("ninguem", "senha-forte-123").is_err());
}

#[test]
fn permissao_que_nao_existe_nao_e_concedida() {
    let arquivo = tempfile::NamedTempFile::new().unwrap();
    let motor = motor_com_admin(&arquivo);
    let sessao = motor.autenticar("supervisor", "senha-forte-123").unwrap();
    assert!(!sessao.concede("pdv.nao.existe"));
    assert!(!sessao.concede(""));
}
