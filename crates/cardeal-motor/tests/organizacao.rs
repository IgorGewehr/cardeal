//! Uma organização com vários CNPJs numa base (ADR-0017): cada CNPJ com os próprios dados,
//! sessões por CNPJ, só quem pode cria CNPJ, e a matriz continua a principal ao reabrir.

use cardeal_kernel::CodigoErro;
use cardeal_modkit::{Modulo, PedidoAtivacao};
use cardeal_motor::MotorLocal;
use mod_clientes::{CriarPessoa, ItemPessoa, ModuloClientes, Papel, PessoasPorPapel, TipoPessoa};
use mod_empresa::ModuloEmpresa;

const SENHA: &str = "senha-forte-123";

fn modulos() -> Vec<&'static dyn Modulo> {
    vec![&ModuloEmpresa, &ModuloClientes]
}

fn abrir(arquivo: &std::path::Path) -> MotorLocal {
    let pedido = PedidoAtivacao::nova()
        .com_modulo("empresa")
        .com_modulo("clientes");
    MotorLocal::abrir(arquivo, &modulos(), &pedido).unwrap()
}

fn criar(motor: &MotorLocal, s: &cardeal_motor::SessaoLocal, nome: &str) {
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
    motor.executar(s, "clientes.criar_pessoa.v1", &c).unwrap();
}

fn clientes(motor: &MotorLocal, s: &cardeal_motor::SessaoLocal) -> Vec<String> {
    let q = PessoasPorPapel {
        papel: Papel::Cliente,
        busca: None,
    };
    let l: Vec<ItemPessoa> = motor
        .consultar(s, "clientes.pessoas_por_papel.v1", &q)
        .unwrap();
    l.into_iter().map(|p| p.nome).collect()
}

#[test]
fn cada_cnpj_tem_os_proprios_dados_e_o_admin_entra_nos_dois() {
    let arquivo = tempfile::NamedTempFile::new().unwrap();
    let motor = abrir(arquivo.path());
    let admin = motor
        .configurar_inicial("Matriz Ltda", "11.222.333/0001-81", "dono", "Dono", SENHA)
        .unwrap();
    let na_matriz = motor.autenticar("dono", SENHA).unwrap();
    let matriz = na_matriz.empresa();
    assert_eq!(matriz, motor.empresa());

    let filial = motor
        .adicionar_empresa(&na_matriz, "Filial Centro Ltda", "11.222.333/0002-62")
        .unwrap();
    let erro = motor
        .adicionar_empresa(&na_matriz, "Repetida", "11222333000262")
        .unwrap_err();
    assert_eq!(
        erro.codigo,
        CodigoErro::DUPLICADO,
        "o mesmo CNPJ, com ou sem máscara"
    );

    let empresas = motor.empresas().unwrap();
    assert_eq!(empresas.len(), 2);
    assert!(empresas[0].matriz && empresas[0].id == matriz);
    assert!(!empresas[1].matriz && empresas[1].id == filial);

    let na_filial = motor.autenticar_em(filial, "dono", SENHA).unwrap();
    assert_eq!(na_filial.empresa(), filial);
    assert!(
        na_filial.concede("clientes.pessoa.criar"),
        "admin da filial com o catálogo inteiro"
    );
    let por_conta = motor
        .sessao_do_usuario_em(filial, admin, cardeal_kernel::Id::novo())
        .unwrap();
    assert_eq!(por_conta.empresa(), filial);

    // Dados isolados por CNPJ.
    criar(&motor, &na_matriz, "Cliente da Matriz");
    criar(&motor, &na_filial, "Cliente da Filial");
    assert_eq!(clientes(&motor, &na_matriz), vec!["Cliente da Matriz"]);
    assert_eq!(clientes(&motor, &na_filial), vec!["Cliente da Filial"]);

    // Dados cadastrais de cada um.
    let dados = |s| -> mod_empresa::EmpresaResumo {
        motor
            .consultar(s, "empresa.dados.v1", &mod_empresa::DadosDaEmpresa)
            .unwrap()
    };
    assert_eq!(dados(&na_matriz).razao_social, "Matriz Ltda");
    assert_eq!(dados(&na_filial).razao_social, "Filial Centro Ltda");

    // Empresa de fora da organização não abre sessão.
    let fora = motor
        .autenticar_em(cardeal_kernel::Id::novo(), "dono", SENHA)
        .err()
        .expect("empresa de fora recusada");
    assert_eq!(fora.codigo, CodigoErro::NAO_ENCONTRADO);

    // Ao reabrir, a matriz continua a principal.
    drop(motor);
    let motor = abrir(arquivo.path());
    assert_eq!(motor.empresa(), matriz);
    assert_eq!(motor.empresas().unwrap().len(), 2);
}

#[test]
fn so_quem_tem_a_permissao_cria_cnpj_e_quem_nao_tem_papel_nao_entra() {
    let arquivo = tempfile::NamedTempFile::new().unwrap();
    let motor = abrir(arquivo.path());
    motor
        .configurar_inicial("Matriz Ltda", "11.222.333/0001-81", "dono", "Dono", SENHA)
        .unwrap();
    let dono = motor.autenticar("dono", SENHA).unwrap();
    let vendedor: cardeal_kernel::Id = motor
        .executar(
            &dono,
            "empresa.adicionar_usuario.v1",
            &mod_empresa::AdicionarUsuario {
                login: "vendedor".into(),
                nome: "Vendedor".into(),
                papel: mod_empresa::PapelDeFabrica::Vendedor,
            },
        )
        .unwrap();
    let vendedor_na_matriz = motor
        .sessao_do_usuario(vendedor, cardeal_kernel::Id::novo())
        .unwrap();
    let erro = motor
        .adicionar_empresa(&vendedor_na_matriz, "Filial", "11.222.333/0002-62")
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::SEM_PERMISSAO);

    let filial = motor
        .adicionar_empresa(&dono, "Filial", "11.222.333/0002-62")
        .unwrap();
    // O vendedor tem papel só na matriz: na filial, não entra.
    let erro = motor
        .sessao_do_usuario_em(filial, vendedor, cardeal_kernel::Id::novo())
        .err()
        .expect("sem papel na filial, sem sessão");
    assert_eq!(erro.codigo, CodigoErro::SEM_PERMISSAO);
    // E a lista de usuários de cada CNPJ é a de quem tem papel nele.
    let na_filial = motor.autenticar_em(filial, "dono", SENHA).unwrap();
    let usuarios = |s| -> Vec<String> {
        let l: Vec<mod_empresa::UsuarioResumo> = motor
            .consultar(s, "empresa.usuarios.v1", &mod_empresa::UsuariosDaEmpresa)
            .unwrap();
        l.into_iter().map(|u| u.login).collect()
    };
    assert_eq!(usuarios(&dono), vec!["dono", "vendedor"]);
    assert_eq!(usuarios(&na_filial), vec!["dono"]);
}
