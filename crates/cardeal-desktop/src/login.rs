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
    /// Login no servidor aceito, conta com mais de uma empresa: falta escolher.
    escolhendo: Option<Remoto>,
}

/// O que a tela de login decidiu.
pub enum Entrada {
    /// Sessão no motor local.
    Local(Sessao),
    /// Motor remoto + sessão numa empresa.
    Remota(Motor, Sessao),
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
                                Motor::Remoto(remoto),
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
        }
        ui.add_space(Espaco::E16);

        let pediu = match e.onde {
            Onde::Local => {
                ui.add(Campo::novo("Login", &mut e.login));
                ui.add_space(Espaco::E12);
                ui.add(
                    Campo::novo("Senha", &mut e.senha)
                        .senha(true)
                        .erro(e.erro.clone()),
                );
                botao_entrar(ui)
            }
            Onde::Servidor => {
                ui.add(
                    Campo::novo("Servidor", &mut e.servidor).marcador("https://app.cardeal.com.br"),
                );
                ui.add_space(Espaco::E12);
                ui.add(Campo::novo("E-mail", &mut e.email));
                ui.add_space(Espaco::E12);
                ui.add(
                    Campo::novo("Senha", &mut e.senha)
                        .senha(true)
                        .erro(e.erro.clone()),
                );
                botao_entrar(ui)
            }
        };
        if !pediu {
            return;
        }
        match e.onde {
            Onde::Local => match motor_local.map(|m| m.autenticar(&e.login, &e.senha)) {
                Some(Ok(sessao)) => entrada = Some(Entrada::Local(sessao)),
                Some(Err(erro)) => e.erro = Some(erro.mensagem),
                None => e.erro = Some("o banco local não está aberto".into()),
            },
            Onde::Servidor => match entrar_no_servidor(&e.servidor, &e.email, &e.senha) {
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
                                    Motor::Remoto(remoto),
                                    Sessao::Remota(sessao),
                                ));
                            }
                            Err(erro) => e.erro = Some(erro.mensagem),
                        },
                        None => e.escolhendo = Some(remoto),
                    }
                }
                Err(erro) => e.erro = Some(erro),
            },
        }
    });
    entrada
}

fn botao_entrar(ui: &mut egui::Ui) -> bool {
    ui.add_space(Espaco::E16);
    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
    let clicou = ui
        .add(Botao::primario("Entrar").atalho("Enter").preenche_largura())
        .clicked();
    clicou || enter
}

fn entrar_no_servidor(servidor: &str, email: &str, senha: &str) -> Result<Remoto, String> {
    let base = servidor.trim();
    if base.is_empty() {
        return Err("informe o endereço do servidor".into());
    }
    let base = if base.starts_with("http://") || base.starts_with("https://") {
        base.to_owned()
    } else {
        format!("https://{base}")
    };
    let transporte = TransporteHttp::novo(&base)?;
    Remoto::entrar(Box::new(transporte), email.trim(), senha).map_err(|e| e.mensagem)
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
