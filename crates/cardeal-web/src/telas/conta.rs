//! Minha conta: trocar a senha. As outras sessões da conta caem (se a senha vazou, quem
//! entrou com ela sai); esta continua.

use cardeal_cliente::remoto::protocolo;
use cardeal_ui::atoms::{Botao, Rotulo};
use cardeal_ui::molecules::{CabecalhoTela, Campo};
use cardeal_ui::organisms::{notificar, Cartao, Notificacao};
use cardeal_ui::tokens::Espaco;

use crate::rede::{disparar_sem_corpo, Pendente};

/// O formulário de troca de senha.
#[derive(Default)]
pub struct Estado {
    atual: String,
    nova: String,
    confirmar: String,
    erro: Option<(Option<String>, String)>,
    enviando: Option<Pendente<()>>,
}

fn erro_em(e: &Estado, campo: &str) -> Option<String> {
    e.erro
        .as_ref()
        .filter(|(c, _)| c.as_deref() == Some(campo))
        .map(|(_, m)| m.clone())
}

/// Desenha.
pub fn mostrar(ui: &mut egui::Ui, e: &mut Estado) {
    if let Some(r) = e.enviando.as_ref().and_then(Pendente::pronto) {
        e.enviando = None;
        match r {
            Ok(()) => {
                notificar(
                    ui.ctx(),
                    Notificacao::sucesso(
                        "Senha trocada. As outras sessões desta conta foram encerradas.",
                    ),
                );
                *e = Estado::default();
            }
            Err(erro) => e.erro = Some((erro.campo, erro.mensagem)),
        }
    }
    CabecalhoTela::novo("Minha conta").mostrar(ui, |_| {});
    ui.add_space(Espaco::E16);
    Cartao::novo().largura(420.0).mostrar(ui, |ui| {
        ui.add(Rotulo::titulo_secao("Trocar a senha"));
        ui.add_space(Espaco::E12);
        let erro_atual = erro_em(e, "atual");
        ui.add(
            Campo::novo("Senha atual", &mut e.atual)
                .senha(true)
                .erro(erro_atual),
        );
        ui.add_space(Espaco::E12);
        let erro_nova = erro_em(e, "nova");
        ui.add(
            Campo::novo("Senha nova", &mut e.nova)
                .senha(true)
                .erro(erro_nova),
        );
        ui.add_space(Espaco::E12);
        let erro_conf = erro_em(e, "confirmar");
        ui.add(
            Campo::novo("Repita a senha nova", &mut e.confirmar)
                .senha(true)
                .erro(erro_conf),
        );
        ui.add_space(Espaco::E16);
        let rotulo = if e.enviando.is_some() {
            "Trocando…"
        } else {
            "Trocar senha"
        };
        if ui.add(Botao::primario(rotulo)).clicked() && e.enviando.is_none() {
            if e.nova == e.confirmar {
                match protocolo::trocar_senha(&e.atual, &e.nova) {
                    Ok(p) => {
                        e.erro = None;
                        e.enviando = Some(disparar_sem_corpo(ui.ctx(), p));
                    }
                    Err(erro) => e.erro = Some((None, erro.mensagem)),
                }
            } else {
                e.erro = Some((
                    Some("confirmar".into()),
                    "as duas senhas novas não batem".into(),
                ));
            }
        }
    });
}
