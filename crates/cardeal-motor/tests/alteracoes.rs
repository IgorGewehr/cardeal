//! O fluxo de alterações (fonte do tempo real): todo comando confirmado aparece, em ordem, e
//! um ponto que a poda já apagou é dito como lacuna — nunca uma lista incompleta em silêncio.

use cardeal_kernel::Instante;
use cardeal_modkit::PedidoAtivacao;
use cardeal_motor::MotorLocal;
use mod_clientes::{CriarPessoa, ModuloClientes, Papel, TipoPessoa};

fn criar(motor: &MotorLocal, sessao: &cardeal_motor::SessaoLocal, nome: &str) {
    let c = CriarPessoa {
        tipo: TipoPessoa::Fisica,
        nome: nome.into(),
        nome_fantasia: None,
        papel_inicial: Papel::Cliente,
        documento_tipo: None,
        documento_numero: None,
        data_nascimento: None,
        endereco: None,
        contato: None,
    };
    motor
        .executar(sessao, "clientes.criar_pessoa.v1", &c)
        .unwrap();
}

#[test]
fn todo_comando_vira_alteracao_e_a_poda_vira_lacuna_declarada() {
    let arquivo = tempfile::NamedTempFile::new().unwrap();
    let pedido = PedidoAtivacao::nova().com_modulo("clientes");
    let motor = MotorLocal::abrir(arquivo.path(), &[&ModuloClientes], &pedido).unwrap();
    motor
        .configurar_inicial(
            "Loja",
            "11.222.333/0001-81",
            "admin",
            "Admin",
            "senha-forte-123",
        )
        .unwrap();
    let sessao = motor.autenticar("admin", "senha-forte-123").unwrap();
    let empresa = sessao.empresa();

    let antes = motor.ultima_alteracao().unwrap();
    criar(&motor, &sessao, "Ana");
    criar(&motor, &sessao, "Bia");
    let a = motor.alteracoes_desde(empresa, antes, 100).unwrap();
    assert!(!a.lacuna);
    let comandos: Vec<_> = a
        .itens
        .iter()
        .filter(|(_, t)| t == "clientes.criar_pessoa.v1")
        .collect();
    assert_eq!(comandos.len(), 2, "{:?}", a.itens);
    assert!(a.itens.windows(2).all(|w| w[0].0 < w[1].0));
    assert_eq!(a.ultimo, motor.ultima_alteracao().unwrap());

    // Limite respeitado e continuação exata.
    let um = motor.alteracoes_desde(empresa, antes, 1).unwrap();
    assert_eq!(um.itens.len(), 1);
    let resto = motor.alteracoes_desde(empresa, um.itens[0].0, 100).unwrap();
    assert_eq!(resto.itens.len(), a.itens.len() - 1);

    // A poda apaga tudo; o último seq sobrevive, e quem estava atrás é mandado recarregar.
    let ultimo = motor.ultima_alteracao().unwrap();
    assert!(
        motor
            .podar_alteracoes(Instante::agora().mais_segundos(60))
            .unwrap()
            > 0
    );
    assert_eq!(motor.ultima_alteracao().unwrap(), ultimo);
    assert!(motor.alteracoes_desde(empresa, antes, 100).unwrap().lacuna);
    let em_dia = motor.alteracoes_desde(empresa, ultimo, 100).unwrap();
    assert!(!em_dia.lacuna && em_dia.itens.is_empty());
    assert!(
        motor
            .alteracoes_desde(empresa, ultimo + 50, 100)
            .unwrap()
            .lacuna,
        "adiante da base"
    );

    // Depois da poda, o próximo comando continua a sequência sem lacuna para quem estava em dia.
    criar(&motor, &sessao, "Caio");
    let novo = motor.alteracoes_desde(empresa, ultimo, 100).unwrap();
    assert!(!novo.lacuna);
    assert_eq!(novo.itens.first().unwrap().0, ultimo + 1);
}
