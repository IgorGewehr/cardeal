//! O cartão **Verificação em duas etapas** (Configurações › Acesso): a pessoa da sessão ativa
//! o código do aplicativo autenticador (QR Code → primeiro código → códigos de recuperação,
//! mostrados uma vez) ou desativa (senha + código). Vale no banco local e na conta do servidor —
//! o `Motor` decide para onde vai.

use cardeal_cliente::{Motor, SegundoFatorProposto, Sessao};
use cardeal_ui::atoms::{Botao, CodigoQr, Etiqueta, Rotulo, Tom};
use cardeal_ui::molecules::Campo;
use cardeal_ui::organisms::{notificar, Notificacao, Painel};
use cardeal_ui::tokens::Espaco;
use eframe::egui;

/// O que o cartão mostra.
#[derive(Default)]
pub struct EstadoSegundoFator {
    /// Ativa? `None`: não deu para saber (sem conexão).
    ativo: Option<bool>,
    /// O segredo proposto, esperando o primeiro código.
    proposta: Option<SegundoFatorProposto>,
    codigo: String,
    /// Os códigos de recuperação recém-gerados — mostrados uma vez.
    recuperacao: Vec<String>,
    /// Pedindo senha e código para desativar.
    desativando: bool,
    senha: String,
}

impl EstadoSegundoFator {
    /// Lê se a pessoa da sessão tem a verificação ativa.
    pub fn carregar(motor: &Motor, sessao: &Sessao) -> Self {
        Self {
            ativo: motor.segundo_fator_ativo(sessao).ok(),
            ..Self::default()
        }
    }
}

/// A chave em grupos de 4 ("GEZD GNBV …"), mais fácil de digitar.
fn em_grupos(segredo: &str) -> String {
    segredo
        .as_bytes()
        .chunks(4)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Desenha o cartão.
pub fn secao(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, e: &mut EstadoSegundoFator) {
    ui.horizontal(|ui| {
        ui.add(Rotulo::titulo_secao("Verificação em duas etapas"));
        match e.ativo {
            Some(true) => {
                ui.add(Etiqueta::positiva("ativa"));
            }
            Some(false) => {
                ui.add(Etiqueta::neutra("desligada"));
            }
            None => {}
        }
    });
    ui.add_space(Espaco::E4);
    ui.add(
        Rotulo::apoio(
            "Além da senha, o código de 6 dígitos de um aplicativo autenticador no celular (Google \
             Authenticator, Microsoft Authenticator, Authy…): quem descobrir a sua senha não entra \
             sem o seu celular.",
        )
        .quebravel(),
    );
    ui.add_space(Espaco::E8);

    if !e.recuperacao.is_empty() {
        Painel::novo().realce(Tom::Atencao).mostrar(ui, |ui| {
            ui.add(Rotulo::interface("Guarde os códigos de recuperação").quebravel());
            ui.add(
                Rotulo::apoio(
                    "Se perder o celular, cada um destes códigos entra uma vez no lugar do código \
                     do aplicativo. Eles não aparecem de novo: imprima ou guarde num lugar seguro, \
                     fora do computador.",
                )
                .quebravel(),
            );
            ui.add_space(Espaco::E4);
            for c in &e.recuperacao {
                ui.add(Rotulo::codigo(c.clone()));
            }
            ui.add_space(Espaco::E8);
            ui.horizontal(|ui| {
                if ui.add(Botao::secundario("Copiar os códigos")).clicked() {
                    ui.ctx().copy_text(e.recuperacao.join("\n"));
                    notificar(ui.ctx(), Notificacao::sucesso("Códigos copiados"));
                }
                if ui.add(Botao::primario("Já guardei")).clicked() {
                    e.recuperacao.clear();
                }
            });
        });
        return;
    }

    if e.ativo == Some(true) {
        if !e.desativando {
            if ui.add(Botao::destrutivo("Desativar")).clicked() {
                e.desativando = true;
            }
            return;
        }
        ui.add(Campo::novo("Sua senha", &mut e.senha).senha(true));
        ui.add_space(Espaco::E8);
        ui.add(Campo::novo(
            "Código do aplicativo (ou de recuperação)",
            &mut e.codigo,
        ));
        ui.add_space(Espaco::E8);
        ui.horizontal(|ui| {
            if ui
                .add(Botao::destrutivo("Desativar a verificação"))
                .clicked()
            {
                match motor.desativar_segundo_fator(sessao, &e.senha, &e.codigo) {
                    Ok(()) => {
                        notificar(
                            ui.ctx(),
                            Notificacao::aviso("Verificação em duas etapas desativada"),
                        );
                        *e = EstadoSegundoFator {
                            ativo: Some(false),
                            ..EstadoSegundoFator::default()
                        };
                    }
                    Err(erro) => notificar(ui.ctx(), Notificacao::erro(erro.mensagem)),
                }
            }
            if ui.add(Botao::fantasma("Cancelar")).clicked() {
                e.desativando = false;
                e.senha.clear();
                e.codigo.clear();
            }
        });
        return;
    }

    let Some(p) = e.proposta.clone() else {
        if e.ativo.is_some()
            && ui
                .add(Botao::primario("Ativar a verificação em duas etapas"))
                .clicked()
        {
            match motor.propor_segundo_fator(sessao) {
                Ok(p) => e.proposta = Some(p),
                Err(erro) => notificar(ui.ctx(), Notificacao::erro(erro.mensagem)),
            }
        }
        return;
    };
    ui.add(
        Rotulo::interface("1. No aplicativo, adicione uma conta lendo este QR Code:").quebravel(),
    );
    ui.add_space(Espaco::E8);
    ui.add(CodigoQr::novo(&p.uri).lado(200.0));
    ui.add_space(Espaco::E4);
    ui.add(Rotulo::apoio(
        "Não consegue ler? Digite esta chave no aplicativo:",
    ));
    ui.add(Rotulo::codigo(em_grupos(&p.segredo)));
    ui.add_space(Espaco::E12);
    ui.add(Rotulo::interface(
        "2. Digite o código de 6 dígitos que o aplicativo mostra:",
    ));
    ui.add_space(Espaco::E4);
    ui.add(Campo::novo("Código", &mut e.codigo).marcador("000000"));
    ui.add_space(Espaco::E8);
    ui.horizontal(|ui| {
        if ui.add(Botao::primario("Ativar")).clicked() {
            match motor.ativar_segundo_fator(sessao, &p.segredo, &e.codigo) {
                Ok(codigos) => {
                    notificar(
                        ui.ctx(),
                        Notificacao::sucesso("Verificação em duas etapas ativada"),
                    );
                    *e = EstadoSegundoFator {
                        ativo: Some(true),
                        recuperacao: codigos,
                        ..EstadoSegundoFator::default()
                    };
                }
                Err(erro) => notificar(ui.ctx(), Notificacao::erro(erro.mensagem)),
            }
        }
        if ui.add(Botao::fantasma("Cancelar")).clicked() {
            e.proposta = None;
            e.codigo.clear();
        }
    });
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::testes_comum::motor_de_teste;

    #[test]
    fn ativa_com_o_codigo_do_aplicativo_e_o_login_passa_a_pedir_o_codigo() {
        let t = motor_de_teste();
        let mut e = EstadoSegundoFator::carregar(&t.motor, &t.sessao);
        assert_eq!(e.ativo, Some(false));
        let ctx = egui::Context::default();
        cardeal_ui::tokens::instalar_fontes(&ctx);
        cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
        let quadro = |e: &mut EstadoSegundoFator| {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    secao(ui, &t.motor, &t.sessao, e);
                });
            });
        };
        quadro(&mut e);
        // O segredo proposto, o código que o "aplicativo" mostraria agora, e a ativação.
        let p = t.motor.propor_segundo_fator(&t.sessao).unwrap();
        assert!(p.uri.starts_with("otpauth://totp/Cardeal:igor?"));
        e.proposta = Some(p.clone());
        quadro(&mut e);
        let segredo = cardeal_auth::totp::SegredoTotp::de_base32(&p.segredo).unwrap();
        let codigo = segredo.codigo_em(cardeal_kernel::Instante::agora());
        let codigos = t
            .motor
            .ativar_segundo_fator(&t.sessao, &p.segredo, &codigo)
            .unwrap();
        assert_eq!(codigos.len(), 8);
        e = EstadoSegundoFator::carregar(&t.motor, &t.sessao);
        e.recuperacao = codigos.clone();
        quadro(&mut e);
        assert_eq!(e.ativo, Some(true));

        // O login agora pede o código; o de recuperação entra uma vez.
        let erro = t
            .motor
            .entrar("igor", "senha-forte-123", None)
            .err()
            .expect("pede o código");
        assert_eq!(
            erro.codigo,
            cardeal_kernel::CodigoErro::SEGUNDO_FATOR_NECESSARIO
        );
        assert!(t
            .motor
            .entrar("igor", "senha-forte-123", Some("000000"))
            .is_err());
        assert!(t
            .motor
            .entrar("igor", "senha-forte-123", Some(&codigos[0]))
            .is_ok());
        assert!(
            t.motor
                .entrar("igor", "senha-forte-123", Some(&codigos[0]))
                .is_err(),
            "o código de recuperação vale uma vez"
        );
        // A senha errada continua errada, com ou sem código.
        assert!(t
            .motor
            .entrar("igor", "outra-senha-qualquer", Some(&codigo))
            .is_err());
        // A liberação por supervisor (só a senha) continua de pé.
        assert!(t.motor.autenticar("igor", "senha-forte-123").is_ok());

        // O desligamento pede a senha e um código.
        e.desativando = true;
        quadro(&mut e);
        assert!(t
            .motor
            .desativar_segundo_fator(&t.sessao, "senha-forte-123", "000000")
            .is_err());
        t.motor
            .desativar_segundo_fator(&t.sessao, "senha-forte-123", &codigos[1])
            .unwrap();
        assert!(t.motor.entrar("igor", "senha-forte-123", None).is_ok());
    }
}
