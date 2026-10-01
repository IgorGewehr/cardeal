//! E-mail e senha. O token volta só no cookie `HttpOnly` — nunca passa pelo JavaScript.

use cardeal_cliente::remoto::protocolo;
use cardeal_protocol::{PedidoLogin, RespostaLogin, TipoCliente};
use cardeal_ui::atoms::{Botao, Rotulo};
use cardeal_ui::molecules::Campo;
use cardeal_ui::organisms::Cartao;
use cardeal_ui::tokens::Espaco;

use super::empresas;
use crate::app::Tela;
use crate::rede::{disparar, Pendente};

/// O estado do login.
#[derive(Default)]
pub struct Estado {
    email: String,
    senha: String,
    erro: Option<String>,
    enviando: Option<Pendente<RespostaLogin>>,
}

/// Desenha; devolve a próxima tela quando o login é aceito.
pub fn mostrar(ui: &mut egui::Ui, e: &mut Estado) -> Option<Tela> {
    if let Some(r) = e.enviando.as_ref().and_then(Pendente::pronto) {
        e.enviando = None;
        match r {
            Ok(login) => return Some(Tela::Empresas(empresas::Estado::novo(login))),
            Err(erro) => e.erro = Some(erro.mensagem),
        }
    }
    ui.add_space(Espaco::E64);
    ui.vertical_centered(|ui| {
        Cartao::novo().largura(380.0).mostrar(ui, |ui| {
            ui.vertical_centered(|ui| ui.add(Rotulo::titulo_tela("Cardeal")));
            ui.add_space(Espaco::E24);
            ui.add(Campo::novo("E-mail", &mut e.email));
            ui.add_space(Espaco::E12);
            ui.add(
                Campo::novo("Senha", &mut e.senha)
                    .senha(true)
                    .erro(e.erro.clone()),
            );
            ui.add_space(Espaco::E16);
            let rotulo = if e.enviando.is_some() {
                "Entrando…"
            } else {
                "Entrar"
            };
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            let clicou = ui
                .add(Botao::primario(rotulo).atalho("Enter").preenche_largura())
                .clicked();
            if (clicou || enter) && e.enviando.is_none() {
                let pedido = protocolo::login(&PedidoLogin {
                    email: e.email.trim().to_owned(),
                    senha: e.senha.clone(),
                    cliente: TipoCliente::Navegador,
                });
                match pedido {
                    Ok(p) => {
                        e.erro = None;
                        e.enviando = Some(disparar(ui.ctx(), p));
                    }
                    Err(erro) => e.erro = Some(erro.mensagem),
                }
            }
        });
    });
    None
}
