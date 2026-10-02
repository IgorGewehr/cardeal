//! A equipe da empresa: quem entra e com que papel. Adicionar e desativar só para quem
//! gerencia usuários — e o servidor confere de novo, o botão escondido é só conveniência.
//!
//! Também é o protótipo do padrão assíncrono de `docs/20-cliente-web.md` §3: o comando sai
//! no clique, e a continuação (fechar o diálogo, avisar, recarregar) roda quando a resposta
//! chega.

use cardeal_cliente::remoto::protocolo;
use cardeal_kernel::Id;
use cardeal_protocol::{MembroAdicionado, PedidoNovoMembro};
use cardeal_ui::atoms::{Botao, Rotulo};
use cardeal_ui::molecules::{CabecalhoTela, Campo, SeletorOpcao};
use cardeal_ui::organisms::{
    dialogo_confirmacao, notificar, ColunaGrade, Dialogo, Grade, Notificacao,
};
use cardeal_ui::tokens::Espaco;
use mod_empresa::{PapelDeFabrica, UsuarioResumo, UsuariosDaEmpresa};

use crate::rede::{disparar, disparar_sem_corpo, Pendente};

/// O formulário de "adicionar à equipe".
#[derive(Default)]
struct Novo {
    email: String,
    nome: String,
    papel: Option<PapelDeFabrica>,
    senha: String,
    erro: Option<(Option<String>, String)>,
    enviando: Option<Pendente<MembroAdicionado>>,
    cancelar: bool,
}

/// O estado da tela.
pub struct Estado {
    empresa: Id,
    eu: Id,
    pode_gerenciar: bool,
    lista: Option<Vec<UsuarioResumo>>,
    carregando: Option<Pendente<Vec<UsuarioResumo>>>,
    erro: Option<String>,
    novo: Option<Novo>,
    desativar: Option<(Id, String)>,
    desativando: Option<Pendente<()>>,
}

impl Estado {
    /// A equipe de `empresa`, vista pelo usuário `eu`.
    pub fn novo(empresa: Id, eu: Id, pode_gerenciar: bool) -> Self {
        Self {
            empresa,
            eu,
            pode_gerenciar,
            lista: None,
            carregando: None,
            erro: None,
            novo: None,
            desativar: None,
            desativando: None,
        }
    }

    /// Pede a lista de novo.
    pub fn recarregar(&mut self, ctx: &egui::Context) {
        if let Ok(c) = protocolo::carga(&UsuariosDaEmpresa) {
            let p = protocolo::consulta(self.empresa, "empresa.usuarios.v1", c);
            self.carregando = Some(disparar(ctx, p));
        }
    }
}

fn erro_do_campo(novo: &Novo, campo: &str) -> Option<String> {
    novo.erro
        .as_ref()
        .filter(|(c, _)| c.as_deref() == Some(campo))
        .map(|(_, m)| m.clone())
}

/// Desenha a equipe.
pub fn mostrar(ui: &mut egui::Ui, e: &mut Estado) {
    let ctx = ui.ctx().clone();
    if e.lista.is_none() && e.carregando.is_none() {
        e.recarregar(&ctx);
    }
    if let Some(r) = e.carregando.as_ref().and_then(Pendente::pronto) {
        e.carregando = None;
        match r {
            Ok(l) => {
                e.lista = Some(l);
                e.erro = None;
            }
            Err(erro) => e.erro = Some(erro.mensagem),
        }
    }
    if let Some(r) = e.desativando.as_ref().and_then(Pendente::pronto) {
        e.desativando = None;
        match r {
            Ok(()) => {
                notificar(&ctx, Notificacao::sucesso("Acesso removido."));
                e.recarregar(&ctx);
            }
            Err(erro) => notificar(&ctx, Notificacao::erro(erro.mensagem)),
        }
    }

    CabecalhoTela::novo("Equipe").mostrar(ui, |ui| {
        if e.pode_gerenciar && ui.add(Botao::primario("+ Adicionar")).clicked() {
            e.novo = Some(Novo::default());
        }
    });
    ui.add_space(Espaco::E16);
    if let Some(m) = &e.erro {
        ui.add(Rotulo::apoio(m.clone()));
        ui.add_space(Espaco::E8);
    }

    let linhas: &[UsuarioResumo] = e.lista.as_deref().unwrap_or(&[]);
    let mut pedir_desativar = None;
    let mut colunas = vec![
        ColunaGrade::nova("Nome"),
        ColunaGrade::nova("E-mail / login").largura(280.0),
        ColunaGrade::nova("Situação").largura(120.0),
    ];
    if e.pode_gerenciar {
        colunas.push(ColunaGrade::nova("Ações").largura(130.0));
    }
    let grade = Grade::nova(colunas)
        .id_salt("web-equipe")
        .carregando(e.carregando.is_some() && e.lista.is_none())
        .vazio("Ninguém na equipe ainda.");
    let grade = if e.pode_gerenciar {
        grade.com_acoes()
    } else {
        grade
    };
    grade.mostrar(ui, linhas.len(), |i, row| {
        let u = &linhas[i];
        row.col(|ui| {
            ui.add(Rotulo::interface(u.nome.clone()));
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(u.login.clone()));
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(if u.ativo {
                "Ativo"
            } else {
                "Desativado"
            }));
        });
        if e.pode_gerenciar {
            row.col(|ui| {
                if u.ativo && u.id != e.eu && ui.add(Botao::fantasma("Desativar")).clicked() {
                    pedir_desativar = Some((u.id, u.nome.clone()));
                }
            });
        }
    });
    if pedir_desativar.is_some() {
        e.desativar = pedir_desativar;
    }

    if let Some((usuario, nome)) = e.desativar.clone() {
        let r = dialogo_confirmacao(
            &ctx,
            "Remover acesso",
            &format!("{nome} não vai mais conseguir entrar nesta empresa. O histórico continua."),
            "Remover acesso",
        );
        if r.confirmado {
            let p = protocolo::remover_membro(e.empresa, usuario);
            e.desativando = Some(disparar_sem_corpo(&ctx, p));
        }
        if r.fechar {
            e.desativar = None;
        }
    }

    formulario(&ctx, e);
}

fn formulario(ctx: &egui::Context, e: &mut Estado) {
    let Some(novo) = e.novo.as_mut() else {
        return;
    };
    if let Some(r) = novo.enviando.as_ref().and_then(Pendente::pronto) {
        novo.enviando = None;
        match r {
            Ok(feito) => {
                let msg = if feito.conta_nova {
                    "Pessoa adicionada — ela já pode entrar com o e-mail e a senha inicial."
                } else {
                    "Pessoa adicionada — ela entra com a conta que já tinha."
                };
                notificar(ctx, Notificacao::sucesso(msg));
                e.novo = None;
                e.recarregar(ctx);
                return;
            }
            Err(erro) => novo.erro = Some((erro.campo, erro.mensagem)),
        }
    }

    let fechar = Dialogo::nova("Adicionar à equipe")
        .descricao("Quem ainda não tem conta no Cardeal entra com o e-mail e a senha inicial.")
        .medio()
        .mostrar(
            ctx,
            novo,
            |ui, n| {
                let erro_email = erro_do_campo(n, "email");
                ui.add(Campo::novo("E-mail", &mut n.email).erro(erro_email));
                ui.add_space(Espaco::E12);
                ui.add(Campo::novo("Nome", &mut n.nome));
                ui.add_space(Espaco::E12);
                SeletorOpcao::novo("Papel", &mut n.papel)
                    .opcoes(PapelDeFabrica::TODOS.iter().map(|p| (*p, p.nome())))
                    .placeholder("Escolha o que a pessoa pode fazer")
                    .mostrar(ui);
                ui.add_space(Espaco::E12);
                let erro_senha = erro_do_campo(n, "senha_inicial");
                ui.add(
                    Campo::novo("Senha inicial (só para e-mail sem conta)", &mut n.senha)
                        .senha(true)
                        .erro(erro_senha),
                );
                if let Some((None, m)) = &n.erro {
                    ui.add_space(Espaco::E8);
                    ui.add(Rotulo::apoio(m.clone()));
                }
            },
            |ui, n| {
                let rotulo = if n.enviando.is_some() {
                    "Adicionando…"
                } else {
                    "Adicionar"
                };
                if ui.add(Botao::primario(rotulo)).clicked() && n.enviando.is_none() {
                    match n.papel {
                        None => n.erro = Some((None, "escolha o papel".into())),
                        Some(papel) => {
                            let pedido = PedidoNovoMembro {
                                email: n.email.trim().to_owned(),
                                nome: n.nome.trim().to_owned(),
                                papel,
                                senha_inicial: (!n.senha.is_empty()).then(|| n.senha.clone()),
                            };
                            match protocolo::adicionar_membro(e.empresa, &pedido) {
                                Ok(p) => {
                                    n.erro = None;
                                    n.enviando = Some(disparar(ui.ctx(), p));
                                }
                                Err(erro) => n.erro = Some((None, erro.mensagem)),
                            }
                        }
                    }
                }
                if ui.add(Botao::secundario("Cancelar")).clicked() {
                    n.cancelar = true;
                }
            },
        );
    let cancelou = e.novo.as_ref().is_some_and(|n| n.cancelar);
    if fechar || cancelou {
        e.novo = None;
    }
}
