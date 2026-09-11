//! Prova o contrato que sustenta a primeira tela que o usuário vê: o administrador criado por
//! `configurar_inicial` (o assistente de primeiro acesso) tem acesso de verdade — não só um
//! papel com nome bonito — a TODOS os módulos ativos, em particular Financeiro, Estoque e
//! Ordens de Serviço, os três que o dono do Cardeal mais vai usar no dia a dia.
//!
//! Mesma lista de módulos + submódulos que `cardeal-desktop::main::modulos()`/
//! `pedido_ativacao()` ligam de verdade — se um dia esse boot mudar e esquecer de ativar um
//! submódulo não-essencial (ex.: "laudo" de `mod-os`), este teste quebra ANTES do usuário abrir
//! o app e trombar com "Sem permissão para ...".

use cardeal_cliente::MotorLocal;
use cardeal_modkit::{Consulta, Modulo, PedidoAtivacao};
use mod_estoque::ProdutosComSaldo;
use mod_financeiro::TitulosAReceberEmAberto;
use mod_os::OrdensEmAberto;

fn modulos() -> Vec<&'static dyn Modulo> {
    vec![
        &mod_financeiro::ModuloFinanceiro,
        &mod_clientes::ModuloClientes,
        &mod_estoque::ModuloEstoque,
        &mod_os::ModuloOs,
    ]
}

/// Espelha `cardeal_desktop::pedido_ativacao` — liga cada módulo E todos os seus submódulos
/// (essenciais e não-essenciais). Ver o comentário da função original: sem isso, permissões de
/// submódulo não-essencial (ex.: `os.laudo.registrar`, que exige o submódulo "laudo") nunca
/// entram no catálogo, e nem o administrador as recebe.
fn pedido_ativacao() -> PedidoAtivacao {
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

fn consultar<C: Consulta + serde::Serialize>(
    motor: &MotorLocal,
    sessao: &cardeal_cliente::SessaoLocal,
    nome: &str,
    consulta: &C,
) where
    C::Saida: serde::de::DeserializeOwned,
{
    motor
        .consultar(sessao, nome, consulta)
        .unwrap_or_else(|e| panic!("admin sem acesso a '{nome}': {} — {}", e.codigo, e.mensagem));
}

#[test]
fn admin_recem_criado_acessa_financeiro_estoque_e_os() {
    let arquivo = tempfile::NamedTempFile::new().expect("criar arquivo temporário");
    let modulos = modulos();
    let pedido = pedido_ativacao();

    let motor = MotorLocal::abrir(arquivo.path(), &modulos, &pedido).expect("abrir motor local");
    assert!(
        motor.precisa_de_configuracao_inicial().unwrap(),
        "base recém-criada deveria pedir configuração inicial"
    );

    motor
        .configurar_inicial(
            "Assistência Técnica do Igor Ltda",
            "11.222.333/0001-81",
            "igor",
            "Igor Gewehr",
            "senha-forte-123",
        )
        .expect("configurar_inicial deveria funcionar numa base nova");

    let sessao = motor
        .autenticar("igor", "senha-forte-123")
        .expect("login do admin recém-criado");

    // As três áreas que o usuário mais vai usar — se qualquer uma destas retornar erro de
    // permissão, o app abre e a primeira tela que o usuário tocar já mostra "Sem permissão".
    consultar(
        &motor,
        &sessao,
        "financeiro.titulos_a_receber_em_aberto.v1",
        &TitulosAReceberEmAberto,
    );
    consultar(
        &motor,
        &sessao,
        "estoque.produtos_com_saldo.v1",
        &ProdutosComSaldo,
    );
    consultar(&motor, &sessao, "os.ordens_em_aberto.v1", &OrdensEmAberto);
}
