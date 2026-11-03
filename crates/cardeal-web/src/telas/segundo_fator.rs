//! Minha conta › **Verificação em duas etapas** no navegador: o mesmo fluxo do desktop (QR
//! Code → primeiro código → códigos de recuperação, uma vez; desativar com senha + código),
//! falando com o servidor pelas rotas `/v1/sessao/segundo-fator`.

use cardeal_cliente::remoto::protocolo;
use cardeal_protocol::SegundoFatorProposto;
use cardeal_ui::atoms::{Botao, CodigoQr, Etiqueta, Rotulo, Tom};
use cardeal_ui::molecules::Campo;
use cardeal_ui::organisms::{notificar, Notificacao, Painel};
use cardeal_ui::tokens::Espaco;

use crate::rede::{disparar, disparar_sem_corpo, Pendente};

/// O cartão.
#[derive(Default)]
pub struct Estado {
    ativo: Option<bool>,
    carregando: Option<Pendente<bool>>,
    proposta: Option<SegundoFatorProposto>,
    propondo: Option<Pendente<SegundoFatorProposto>>,
    codigo: String,
    ativando: Option<Pendente<Vec<String>>>,
    recuperacao: Vec<String>,
    desativando: bool,
    senha: String,
    desligando: Option<Pendente<()>>,
}

fn grupos(s: &str) -> String {
    s.as_bytes()
        .chunks(4)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Recebe o que chegou do servidor.
fn receber(ui: &egui::Ui, e: &mut Estado) {
    if let Some(r) = e.carregando.as_ref().and_then(Pendente::pronto) {
        e.carregando = None;
        e.ativo = r.ok();
    }
    if let Some(r) = e.propondo.as_ref().and_then(Pendente::pronto) {
        e.propondo = None;
        match r {
            Ok(p) => e.proposta = Some(p),
            Err(erro) => notificar(ui.ctx(), Notificacao::erro(erro.mensagem)),
        }
    }
    if let Some(r) = e.ativando.as_ref().and_then(Pendente::pronto) {
        e.ativando = None;
        match r {
            Ok(codigos) => {
                notificar(
                    ui.ctx(),
                    Notificacao::sucesso("Verificação em duas etapas ativada"),
                );
                *e = Estado {
                    ativo: Some(true),
                    recuperacao: codigos,
                    ..Estado::default()
                };
            }
            Err(erro) => notificar(ui.ctx(), Notificacao::erro(erro.mensagem)),
        }
    }
    if let Some(r) = e.desligando.as_ref().and_then(Pendente::pronto) {
        e.desligando = None;
        match r {
            Ok(()) => {
                notificar(
                    ui.ctx(),
                    Notificacao::aviso("Verificação em duas etapas desativada"),
                );
                *e = Estado {
                    ativo: Some(false),
                    ..Estado::default()
                };
            }
            Err(erro) => notificar(ui.ctx(), Notificacao::erro(erro.mensagem)),
        }
    }
}

/// Desenha o cartão.
pub fn mostrar(ui: &mut egui::Ui, e: &mut Estado) {
    receber(ui, e);
    if e.ativo.is_none() && e.carregando.is_none() {
        e.carregando = Some(disparar(ui.ctx(), protocolo::segundo_fator_ativo()));
    }
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
            "Além da senha, o código de 6 dígitos de um aplicativo autenticador no celular: quem \
             descobrir a sua senha não entra sem o seu celular.",
        )
        .quebravel(),
    );
    ui.add_space(Espaco::E8);
    if !e.recuperacao.is_empty() {
        Painel::novo().realce(Tom::Atencao).mostrar(ui, |ui| {
            ui.add(Rotulo::interface("Guarde os códigos de recuperação").quebravel());
            ui.add(
                Rotulo::apoio(
                    "Cada um entra uma vez no lugar do código, se perder o celular. Eles não \
                     aparecem de novo.",
                )
                .quebravel(),
            );
            for c in &e.recuperacao {
                ui.add(Rotulo::codigo(c.clone()));
            }
            ui.horizontal(|ui| {
                if ui.add(Botao::secundario("Copiar os códigos")).clicked() {
                    ui.ctx().copy_text(e.recuperacao.join("\n"));
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
                .add(
                    Botao::destrutivo("Desativar a verificação").carregando(e.desligando.is_some()),
                )
                .clicked()
                && e.desligando.is_none()
            {
                match protocolo::desativar_segundo_fator(&e.senha, &e.codigo) {
                    Ok(p) => e.desligando = Some(disparar_sem_corpo(ui.ctx(), p)),
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
        if e.ativo == Some(false)
            && ui
                .add(
                    Botao::primario("Ativar a verificação em duas etapas")
                        .carregando(e.propondo.is_some()),
                )
                .clicked()
            && e.propondo.is_none()
        {
            e.propondo = Some(disparar(ui.ctx(), protocolo::propor_segundo_fator()));
        }
        return;
    };
    ui.add(Rotulo::interface(
        "1. No aplicativo, adicione uma conta lendo este QR Code:",
    ));
    ui.add_space(Espaco::E8);
    ui.add(CodigoQr::novo(&p.uri).lado(200.0));
    ui.add(Rotulo::apoio("Não consegue ler? Digite esta chave:"));
    ui.add(Rotulo::codigo(grupos(&p.segredo)));
    ui.add_space(Espaco::E12);
    ui.add(Rotulo::interface(
        "2. Digite o código de 6 dígitos que o aplicativo mostra:",
    ));
    ui.add(Campo::novo("Código", &mut e.codigo).marcador("000000"));
    ui.add_space(Espaco::E8);
    ui.horizontal(|ui| {
        if ui
            .add(Botao::primario("Ativar").carregando(e.ativando.is_some()))
            .clicked()
            && e.ativando.is_none()
        {
            match protocolo::ativar_segundo_fator(&p.segredo, &e.codigo) {
                Ok(pedido) => e.ativando = Some(disparar(ui.ctx(), pedido)),
                Err(erro) => notificar(ui.ctx(), Notificacao::erro(erro.mensagem)),
            }
        }
        if ui.add(Botao::fantasma("Cancelar")).clicked() {
            e.proposta = None;
            e.codigo.clear();
        }
    });
}
