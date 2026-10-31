//! A tela de login: **neste computador** (o banco local, monoposto) ou **no servidor** (o
//! Cardeal online, ADR-0016 — o mesmo programa, as mesmas telas, o motor do outro lado da rede).

use std::path::PathBuf;

use cardeal_cliente::remoto::{Remoto, TransporteHttp};
use cardeal_cliente::{Motor, Sessao};
use cardeal_kernel::Id;
use cardeal_ui::atoms::{Botao, Rotulo};
use cardeal_ui::molecules::{Abas, Campo};
use cardeal_ui::organisms::Cartao;
use cardeal_ui::tokens::{Espaco, TemaUi};
use eframe::egui;

/// Onde entrar.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Onde {
    /// O banco deste computador.
    #[default]
    Local,
    /// Um servidor Cardeal.
    Servidor,
}

/// O estado da tela de login.
#[derive(Default)]
pub struct EstadoLogin {
    onde: Onde,
    login: String,
    senha: String,
    servidor: String,
    email: String,
    erro: Option<String>,
    /// A senha conferiu e a conta tem verificação em duas etapas: falta o código.
    pede_codigo: bool,
    codigo: String,
    /// Login no servidor aceito, conta com mais de uma empresa: falta escolher.
    escolhendo: Option<Remoto>,
}

/// O que a tela de login decidiu.
pub enum Entrada {
    /// Sessão no motor local.
    Local(Sessao),
    /// Motor remoto + sessão numa empresa.
    Remota(Box<Motor>, Sessao),
}

impl EstadoLogin {
    /// Login local já com o usuário preenchido (logo depois do primeiro acesso).
    pub fn com_login(login: String) -> Self {
        Self {
            login,
            ..Self::lembrado()
        }
    }

    /// Já na aba do servidor, preenchida — para a captura de demonstração.
    #[cfg(feature = "demo")]
    pub fn no_servidor(servidor: &str, email: &str) -> Self {
        Self {
            onde: Onde::Servidor,
            servidor: servidor.to_owned(),
            email: email.to_owned(),
            ..Self::default()
        }
    }

    /// Estado inicial com o último servidor/e-mail usados.
    pub fn lembrado() -> Self {
        let mut e = Self::default();
        if let Ok(texto) = std::fs::read_to_string(caminho_lembrete()) {
            let mut linhas = texto.lines();
            e.servidor = linhas.next().unwrap_or_default().to_owned();
            e.email = linhas.next().unwrap_or_default().to_owned();
            if !e.servidor.is_empty() {
                e.onde = Onde::Servidor;
            }
        }
        e
    }
}

fn caminho_lembrete() -> PathBuf {
    crate::caminho_da_base().with_file_name("servidor.txt")
}

/// Desenha o login. Devolve a entrada quando a pessoa conseguiu entrar.
pub fn mostrar(
    ui: &mut egui::Ui,
    motor_local: Option<&Motor>,
    e: &mut EstadoLogin,
) -> Option<Entrada> {
    let mut entrada = None;
    ui.add_space(Espaco::E64);
    Cartao::novo().largura(380.0).mostrar(ui, |ui| {
        ui.vertical_centered(|ui| {
            ui.add(Rotulo::titulo_tela("Cardeal"));
        });
        ui.add_space(Espaco::E16);

        if let Some(remoto) = &e.escolhendo {
            if let Some(empresa) = escolher_empresa(ui, remoto, e.erro.as_deref()) {
                if let Some(mut remoto) = e.escolhendo.take() {
                    match remoto.escolher_empresa(empresa) {
                        Ok(sessao) => {
                            entrada = Some(Entrada::Remota(
                                Box::new(Motor::Remoto(remoto)),
                                Sessao::Remota(sessao),
                            ));
                        }
                        Err(erro) => {
                            e.erro = Some(erro.mensagem);
                            e.escolhendo = Some(remoto);
                        }
                    }
                }
            }
            return;
        }

        let onde = ui
            .vertical_centered(|ui| {
                Abas::nova(&[
                    (Onde::Local, "Neste computador"),
                    (Onde::Servidor, "No servidor"),
                ])
                .selecionada(e.onde)
                .id_salt("login-onde")
                .mostrar(ui)
            })
            .inner;
        if let Some(onde) = onde {
            e.onde = onde;
            e.erro = None;
            e.pede_codigo = false;
            e.codigo.clear();
        }
        ui.add_space(Espaco::E16);

        let pediu = match e.onde {
            Onde::Local => {
                if ui.add(Campo::novo("Login", &mut e.login)).changed() {
                    e.pede_codigo = false;
                }
                ui.add_space(Espaco::E12);
                ui.add(
                    Campo::novo("Senha", &mut e.senha)
                        .senha(true)
                        .erro(e.erro.clone().filter(|_| !e.pede_codigo)),
                );
                campo_codigo(ui, e);
                botao_entrar(ui)
            }
            Onde::Servidor => {
                ui.add(
                    Campo::novo("Servidor", &mut e.servidor).marcador("https://app.cardeal.com.br"),
                );
                ui.add_space(Espaco::E12);
                if ui.add(Campo::novo("E-mail", &mut e.email)).changed() {
                    e.pede_codigo = false;
                }
                ui.add_space(Espaco::E12);
                ui.add(
                    Campo::novo("Senha", &mut e.senha)
                        .senha(true)
                        .erro(e.erro.clone().filter(|_| !e.pede_codigo)),
                );
                campo_codigo(ui, e);
                botao_entrar(ui)
            }
        };
        if !pediu {
            return;
        }
        let codigo = e.pede_codigo.then(|| e.codigo.clone());
        match e.onde {
            Onde::Local => {
                match motor_local.map(|m| m.entrar(&e.login, &e.senha, codigo.as_deref())) {
                    Some(Ok(sessao)) => entrada = Some(Entrada::Local(sessao)),
                    Some(Err(erro)) => recusado(e, &erro),
                    None => e.erro = Some("o banco local não está aberto".into()),
                }
            }
            Onde::Servidor => {
                match entrar_no_servidor(&e.servidor, &e.email, &e.senha, codigo.as_deref()) {
                    Ok(mut remoto) => {
                        let _ = std::fs::write(
                            caminho_lembrete(),
                            format!("{}\n{}\n", e.servidor.trim(), e.email.trim()),
                        );
                        e.senha.clear();
                        match remoto.empresa() {
                            Some(empresa) => match remoto.escolher_empresa(empresa) {
                                Ok(sessao) => {
                                    entrada = Some(Entrada::Remota(
                                        Box::new(Motor::Remoto(remoto)),
                                        Sessao::Remota(sessao),
                                    ));
                                }
                                Err(erro) => e.erro = Some(erro.mensagem),
                            },
                            None => e.escolhendo = Some(remoto),
                        }
                    }
                    Err(erro) => recusado(e, &erro),
                }
            }
        }
    });
    entrada
}

/// O login recusado: a senha conferiu e falta o código (a tela passa a pedi-lo), ou o erro.
fn recusado(e: &mut EstadoLogin, erro: &cardeal_kernel::Erro) {
    if erro.codigo == cardeal_kernel::CodigoErro::SEGUNDO_FATOR_NECESSARIO {
        e.pede_codigo = true;
        e.codigo.clear();
    }
    e.erro = Some(erro.mensagem.clone());
}

/// O campo do código, quando a conta tem verificação em duas etapas.
fn campo_codigo(ui: &mut egui::Ui, e: &mut EstadoLogin) {
    if !e.pede_codigo {
        return;
    }
    ui.add_space(Espaco::E12);
    ui.add(
        Campo::novo("Código do aplicativo autenticador", &mut e.codigo)
            .marcador("000000 — ou um código de recuperação")
            .erro(e.erro.clone()),
    );
}

fn botao_entrar(ui: &mut egui::Ui) -> bool {
    ui.add_space(Espaco::E16);
    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
    let clicou = ui
        .add(Botao::primario("Entrar").atalho("Enter").preenche_largura())
        .clicked();
    clicou || enter
}

fn entrar_no_servidor(
    servidor: &str,
    email: &str,
    senha: &str,
    codigo: Option<&str>,
) -> Result<Remoto, cardeal_kernel::Erro> {
    use cardeal_kernel::{CodigoErro, Erro};
    let base = servidor.trim();
    if base.is_empty() {
        return Err(Erro::novo(
            CodigoErro::ENTRADA_INVALIDA,
            "informe o endereço do servidor",
        ));
    }
    let base = if base.starts_with("http://") || base.starts_with("https://") {
        base.to_owned()
    } else {
        format!("https://{base}")
    };
    let transporte =
        TransporteHttp::novo(&base).map_err(|e| Erro::novo(CodigoErro::ENTRADA_INVALIDA, e))?;
    match codigo {
        Some(c) => Remoto::entrar_com_codigo(Box::new(transporte), email.trim(), senha, c),
        None => Remoto::entrar(Box::new(transporte), email.trim(), senha),
    }
}

fn escolher_empresa(ui: &mut egui::Ui, remoto: &Remoto, erro: Option<&str>) -> Option<Id> {
    ui.add(Rotulo::apoio(format!(
        "Olá, {}. Em qual empresa?",
        remoto.nome()
    )));
    ui.add_space(Espaco::E12);
    let mut escolhida = None;
    for empresa in remoto.empresas() {
        if ui
            .add(Botao::secundario(empresa.nome.clone()).preenche_largura())
            .clicked()
        {
            escolhida = Some(empresa.id);
        }
        ui.add_space(Espaco::E8);
    }
    if let Some(msg) = erro {
        ui.add(Rotulo::interface(msg.to_owned()).cor(ui.cores().negativo));
    }
    escolhida
}
