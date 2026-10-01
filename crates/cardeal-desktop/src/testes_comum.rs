//! Apoio aos testes das telas: um motor de verdade (SQLite num arquivo temporário), já
//! configurado e com o administrador logado — o mesmo caminho que o app percorre.

use cardeal_cliente::{MotorLocal, SessaoLocal};

/// O motor aberto, o arquivo que o sustenta (apagado ao sair de escopo) e a sessão do admin.
pub struct MotorDeTeste {
    pub _arquivo: tempfile::NamedTempFile,
    pub motor: MotorLocal,
    pub sessao: SessaoLocal,
}

/// Abre um motor novo com todos os módulos do app e faz login como administrador.
pub fn motor_de_teste() -> MotorDeTeste {
    let arquivo = tempfile::NamedTempFile::new().expect("arquivo temporário");
    let motor = MotorLocal::abrir(
        arquivo.path(),
        &cardeal_distribuicao::modulos(),
        &cardeal_distribuicao::pedido_ativacao(),
    )
    .expect("abrir motor");
    motor
        .configurar_inicial(
            "Assistência Teste",
            "11.222.333/0001-81",
            "igor",
            "Igor",
            "senha-forte-123",
        )
        .expect("configuração inicial");
    let sessao = motor.autenticar("igor", "senha-forte-123").expect("login");
    MotorDeTeste {
        _arquivo: arquivo,
        motor,
        sessao,
    }
}
