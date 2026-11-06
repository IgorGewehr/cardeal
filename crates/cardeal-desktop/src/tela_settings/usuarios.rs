//! Configurações › Acesso › **Usuários**: quem trabalha na empresa e com que papel — cadastrar
//! (nome, login, papel e a senha inicial, trocada no primeiro acesso), trocar o papel, dar uma
//! senha provisória a quem esqueceu, desativar e reativar, e desligar a verificação em duas
//! etapas de quem perdeu o celular. E **Minha senha**, para a pessoa da sessão.
//!
//! No servidor, "cadastrar" põe a conta do e-mail na empresa e "desativar" tira o vínculo; a
//! senha de cada conta é dela (a provisória e a reativação são do banco local).

use cardeal_cliente::{Motor, Sessao, UsuarioResumo};
use cardeal_kernel::Id;
use cardeal_ui::atoms::{Botao, Etiqueta, Rotulo};
use cardeal_ui::molecules::{AcaoRegistro, AcoesRegistro, Campo, SeletorOpcao};
use cardeal_ui::organisms::{notificar, ColunaGrade, Dialogo, Grade, Notificacao};
use cardeal_ui::tokens::{Espaco, TemaUi};
use eframe::egui;
use mod_empresa::PapelDeFabrica;

/// O cadastro de alguém.
#[derive(Default)]
struct FormNovo {
    nome: String,
    login: String,
    papel: Option<PapelDeFabrica>,
    senha: String,
    confirmar: String,
}

/// A edição de um usuário.
struct FormEdicao {
    usuario: Id,
    nome: String,
    ativo: bool,
    segundo_fator: bool,
    papel: Option<PapelDeFabrica>,
    papel_atual: Option<PapelDeFabrica>,
    senha: String,
}

/// O estado do cartão.
#[derive(Default)]
pub struct EstadoUsuarios {
    novo: Option<FormNovo>,
    edicao: Option<FormEdicao>,
    /// A troca da própria senha.
    atual: String,
    nova: String,
    confirmar: String,
}

fn papel_de(nome: Option<&str>) -> Option<PapelDeFabrica> {
    PapelDeFabrica::TODOS
        .into_iter()
        .find(|p| Some(p.nome()) == nome)
}

fn opcoes() -> Vec<(PapelDeFabrica, String)> {
    PapelDeFabrica::TODOS
        .into_iter()
        .map(|p| (p, format!("{} — {}", p.nome(), p.descricao())))
        .collect()
}

/// O cartão dos usuários. Devolve `true` quando a lista precisa recarregar.
pub fn secao(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    e: &mut EstadoUsuarios,
    usuarios: &[UsuarioResumo],
) -> bool {
    let gerencia = sessao.concede("empresa.usuarios.gerenciar");
    let local = matches!(motor, Motor::Local(_));
    let mut recarregar = false;
    ui.horizontal(|ui| {
        ui.add(Rotulo::titulo_secao("Usuários"));
        if gerencia
            && e.novo.is_none()
            && ui
                .add(Botao::secundario("+ Novo usuário").pequeno())
                .clicked()
        {
            e.novo = Some(FormNovo::default());
        }
    });
    ui.add_space(Espaco::E8);
    let mut fechar_novo = false;
    if let Some(f) = &mut e.novo {
        let (rotulo_login, apoio) = if local {
            (
                "Login",
                "A pessoa entra com este login e a senha inicial, e troca a senha no primeiro \
                 acesso.",
            )
        } else {
            (
                "E-mail",
                "Quem já tem conta no Lunno online só ganha acesso a esta empresa (a senha \
                 inicial é ignorada); um e-mail novo vira conta com ela.",
            )
        };
        ui.add(Rotulo::apoio(apoio).quebravel());
        ui.add_space(Espaco::E4);
        ui.columns(2, |c| {
            c[0].add(Campo::novo("Nome", &mut f.nome));
            c[1].add(Campo::novo(rotulo_login, &mut f.login));
        });
        ui.add_space(Espaco::E8);
        SeletorOpcao::novo("Papel", &mut f.papel)
            .opcoes(opcoes())
            .placeholder("O que a pessoa pode fazer…")
            .mostrar(ui);
        ui.add_space(Espaco::E8);
        ui.columns(2, |c| {
            c[0].add(Campo::novo("Senha inicial", &mut f.senha).senha(true));
            c[1].add(Campo::novo("Repita a senha", &mut f.confirmar).senha(true));
        });
        ui.add_space(Espaco::E8);
        ui.horizontal(|ui| {
            if ui.add(Botao::primario("Cadastrar")).clicked() {
                match (f.papel, f.senha == f.confirmar) {
                    (None, _) => notificar(ui.ctx(), Notificacao::aviso("Escolha o papel.")),
                    (_, false) => {
                        notificar(ui.ctx(), Notificacao::aviso("As duas senhas não batem."))
                    }
                    (Some(papel), true) => {
                        match motor.cadastrar_usuario(sessao, &f.login, &f.nome, papel, &f.senha) {
                            Ok(_) => {
                                notificar(
                                    ui.ctx(),
                                    Notificacao::sucesso(format!("{} cadastrado", f.nome.trim())),
                                );
                                fechar_novo = true;
                                recarregar = true;
                            }
                            Err(erro) => notificar(ui.ctx(), Notificacao::erro(erro.mensagem)),
                        }
                    }
                }
            }
            if ui.add(Botao::fantasma("Cancelar")).clicked() {
                fechar_novo = true;
            }
        });
        ui.add_space(Espaco::E12);
    }
    if fechar_novo {
        e.novo = None;
    }
    if usuarios.is_empty() {
        ui.add(Rotulo::campo("Nenhum usuário."));
        return recarregar;
    }
    let eu = sessao.usuario();
    let mut colunas = vec![
        ColunaGrade::nova("Nome"),
        ColunaGrade::nova("Login").largura(150.0),
        ColunaGrade::nova("Papel").largura(130.0),
        ColunaGrade::nova("Situação").largura(80.0),
        ColunaGrade::nova("Duas etapas").largura(100.0),
    ];
    if gerencia {
        colunas.push(ColunaGrade::nova("").largura(80.0));
    }
    let mut editar = None;
    let grade = if gerencia {
        Grade::nova(colunas).com_acoes()
    } else {
        Grade::nova(colunas)
    };
    grade
        .id_salt("usuarios")
        .mostrar(ui, usuarios.len(), |i, row| {
            let u = &usuarios[i];
            row.col(|ui| {
                ui.horizontal(|ui| {
                    ui.add(Rotulo::interface(u.nome.clone()));
                    if u.id == eu {
                        ui.add(Etiqueta::info("você"));
                    }
                });
            });
            row.col(|ui| {
                ui.add(Rotulo::campo(u.login.clone()));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(
                    u.papel.clone().unwrap_or_else(|| "—".to_owned()),
                ));
            });
            row.col(|ui| {
                let (t, c) = if u.ativo {
                    ("ativo", ui.cores().positivo)
                } else {
                    ("inativo", ui.cores().texto_fraco)
                };
                ui.add(Rotulo::interface(t).cor(c));
            });
            row.col(|ui| {
                if u.segundo_fator {
                    ui.add(Etiqueta::positiva("ativa"));
                } else {
                    ui.add(Rotulo::campo("—"));
                }
            });
            if gerencia {
                row.col(|ui| {
                    if u.id != eu
                        && AcoesRegistro::novo().excluir(false).mostrar(ui)
                            == Some(AcaoRegistro::Editar)
                    {
                        editar = Some(i);
                    }
                });
            }
        });
    if let Some(i) = editar {
        let u = &usuarios[i];
        let papel = papel_de(u.papel.as_deref());
        e.edicao = Some(FormEdicao {
            usuario: u.id,
            nome: u.nome.clone(),
            ativo: u.ativo,
            segundo_fator: u.segundo_fator,
            papel,
            papel_atual: papel,
            senha: String::new(),
        });
    }
    if dialogo_edicao(ui.ctx(), motor, sessao, e, local) {
        recarregar = true;
    }
    recarregar
}

/// O que o diálogo de edição pediu.
enum Pedido {
    Papel(PapelDeFabrica),
    Senha(String),
    Desativar,
    Reativar,
    DesligarDuasEtapas,
}

fn dialogo_edicao(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    e: &mut EstadoUsuarios,
    local: bool,
) -> bool {
    let Some(f) = &mut e.edicao else {
        return false;
    };
    let mut pedido = None;
    let fechar = Dialogo::nova(format!("Usuário · {}", f.nome))
        .largura(560.0)
        .mostrar(
            ctx,
            f,
            |ui, f| {
                ui.add(Rotulo::titulo_secao("Papel"));
                SeletorOpcao::novo("Papel nesta empresa", &mut f.papel)
                    .opcoes(opcoes())
                    .mostrar(ui);
                if f.papel != f.papel_atual {
                    if let Some(p) = f.papel {
                        if ui
                            .add(Botao::primario("Trocar o papel").pequeno())
                            .clicked()
                        {
                            pedido = Some(Pedido::Papel(p));
                        }
                    }
                }
                if local {
                    ui.add_space(Espaco::E16);
                    ui.add(Rotulo::titulo_secao("Senha provisória"));
                    ui.add(
                        Rotulo::apoio(
                            "Para quem esqueceu a senha: a pessoa entra com esta e escolhe outra \
                             no mesmo momento.",
                        )
                        .quebravel(),
                    );
                    ui.add(Campo::novo("Senha provisória", &mut f.senha).senha(true));
                    if !f.senha.is_empty()
                        && ui
                            .add(Botao::secundario("Definir a senha provisória").pequeno())
                            .clicked()
                    {
                        pedido = Some(Pedido::Senha(f.senha.clone()));
                    }
                }
                if f.segundo_fator && local {
                    ui.add_space(Espaco::E16);
                    ui.add(Rotulo::titulo_secao("Verificação em duas etapas"));
                    ui.add(
                        Rotulo::apoio(
                            "Desligue só para quem perdeu o celular e os códigos de recuperação: \
                             a pessoa entra só com a senha até ativar de novo.",
                        )
                        .quebravel(),
                    );
                    if ui
                        .add(Botao::destrutivo("Desligar a verificação").pequeno())
                        .clicked()
                    {
                        pedido = Some(Pedido::DesligarDuasEtapas);
                    }
                }
                ui.add_space(Espaco::E16);
                if f.ativo {
                    ui.add(Rotulo::apoio(
                        "Desativar tira o acesso; o histórico do que a pessoa fez fica.",
                    ));
                    if ui.add(Botao::destrutivo("Desativar").pequeno()).clicked() {
                        pedido = Some(Pedido::Desativar);
                    }
                } else if local && ui.add(Botao::secundario("Reativar").pequeno()).clicked() {
                    pedido = Some(Pedido::Reativar);
                }
            },
            |ui, _| {
                let _ = ui;
            },
        );
    if fechar {
        e.edicao = None;
        return false;
    }
    let Some(pedido) = pedido else {
        return false;
    };
    let usuario = f.usuario;
    let nome = f.nome.clone();
    let r = match &pedido {
        Pedido::Papel(p) => motor.definir_papel_de(sessao, usuario, *p),
        Pedido::Senha(s) => motor.redefinir_senha_de(sessao, usuario, s),
        Pedido::Desativar => motor.desativar_usuario(sessao, usuario),
        Pedido::Reativar => motor.reativar_usuario(sessao, usuario),
        Pedido::DesligarDuasEtapas => motor.desligar_segundo_fator_de(sessao, usuario),
    };
    match r {
        Ok(()) => {
            let msg = match pedido {
                Pedido::Papel(p) => format!("{nome} agora é {}", p.nome()),
                Pedido::Senha(_) => format!("Senha provisória de {nome} definida"),
                Pedido::Desativar => format!("{nome} desativado"),
                Pedido::Reativar => format!("{nome} reativado"),
                Pedido::DesligarDuasEtapas => {
                    format!("Verificação em duas etapas de {nome} desligada")
                }
            };
            notificar(ctx, Notificacao::sucesso(msg));
            e.edicao = None;
            true
        }
        Err(erro) => {
            notificar(ctx, Notificacao::erro(erro.mensagem));
            false
        }
    }
}

/// O cartão "Minha senha".
pub fn secao_minha_senha(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    e: &mut EstadoUsuarios,
) {
    ui.add(Rotulo::titulo_secao("Minha senha"));
    ui.add_space(Espaco::E8);
    ui.add(Campo::novo("Senha atual", &mut e.atual).senha(true));
    ui.add_space(Espaco::E8);
    ui.columns(2, |c| {
        c[0].add(Campo::novo("Senha nova", &mut e.nova).senha(true));
        c[1].add(Campo::novo("Repita a senha nova", &mut e.confirmar).senha(true));
    });
    ui.add_space(Espaco::E8);
    if ui.add(Botao::primario("Trocar a senha")).clicked() {
        if e.nova != e.confirmar {
            notificar(
                ui.ctx(),
                Notificacao::aviso("As duas senhas novas não batem."),
            );
            return;
        }
        match motor.trocar_senha(sessao, &e.atual, &e.nova) {
            Ok(()) => {
                notificar(ui.ctx(), Notificacao::sucesso("Senha trocada"));
                e.atual.clear();
                e.nova.clear();
                e.confirmar.clear();
            }
            Err(erro) => notificar(ui.ctx(), Notificacao::erro(erro.mensagem)),
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::testes_comum::motor_de_teste;

    #[test]
    fn o_admin_cadastra_troca_papel_e_a_senha_inicial_e_trocada_no_primeiro_acesso() {
        let t = motor_de_teste();
        // A senha fraca é recusada antes de virar hash.
        assert!(t
            .motor
            .cadastrar_usuario(
                &t.sessao,
                "maria",
                "Maria",
                PapelDeFabrica::OperadorCaixa,
                "123"
            )
            .is_err());
        let maria = t
            .motor
            .cadastrar_usuario(
                &t.sessao,
                "Maria",
                "Maria Souza",
                PapelDeFabrica::OperadorCaixa,
                "inicial-da-maria-1",
            )
            .unwrap();
        assert!(
            t.motor
                .cadastrar_usuario(
                    &t.sessao,
                    "maria",
                    "Outra",
                    PapelDeFabrica::Vendedor,
                    "outra-senha-boa-1"
                )
                .is_err(),
            "login repetido"
        );
        let lista = t.motor.usuarios().unwrap();
        let u = lista.iter().find(|u| u.id == maria).unwrap();
        assert_eq!(u.login, "maria");
        assert_eq!(u.papel.as_deref(), Some("Operador de Caixa"));

        // Maria entra, e precisa trocar a senha antes de trabalhar.
        let s = t.motor.entrar("maria", "inicial-da-maria-1", None).unwrap();
        assert!(t.motor.precisa_trocar_senha(&s).unwrap());
        assert!(!s.concede("empresa.usuarios.gerenciar"));
        t.motor
            .trocar_senha(&s, "inicial-da-maria-1", "a-senha-da-maria-9")
            .unwrap();
        let s = t.motor.entrar("maria", "a-senha-da-maria-9", None).unwrap();
        assert!(!t.motor.precisa_trocar_senha(&s).unwrap());

        // O admin troca o papel; ela não troca o próprio nem o dos outros.
        t.motor
            .definir_papel_de(&t.sessao, maria, PapelDeFabrica::Gerente)
            .unwrap();
        assert!(t
            .motor
            .definir_papel_de(&s, maria, PapelDeFabrica::Administrador)
            .is_err());
        // A Gerente gerencia usuários, mas não dá o papel de Administrador nem mexe no admin.
        let s = t.motor.entrar("maria", "a-senha-da-maria-9", None).unwrap();
        assert!(s.concede("empresa.usuarios.gerenciar"));
        let joao = t
            .motor
            .cadastrar_usuario(
                &s,
                "joao",
                "João",
                PapelDeFabrica::Vendedor,
                "senha-do-joao-1",
            )
            .unwrap();
        assert!(t
            .motor
            .definir_papel_de(&s, joao, PapelDeFabrica::Administrador)
            .is_err());
        assert!(t
            .motor
            .redefinir_senha_de(&s, t.sessao.usuario(), "tomei-a-conta-1")
            .is_err());
        // O último administrador não perde o papel.
        assert!(t
            .motor
            .definir_papel_de(&s, t.sessao.usuario(), PapelDeFabrica::Vendedor)
            .is_err());

        // A senha provisória: João entra com ela e precisa trocar.
        t.motor
            .redefinir_senha_de(&t.sessao, joao, "provisoria-joao-2")
            .unwrap();
        let sj = t.motor.entrar("joao", "provisoria-joao-2", None).unwrap();
        assert!(t.motor.precisa_trocar_senha(&sj).unwrap());

        // Desativar e reativar.
        t.motor.desativar_usuario(&t.sessao, joao).unwrap();
        assert!(t.motor.entrar("joao", "provisoria-joao-2", None).is_err());
        t.motor.reativar_usuario(&t.sessao, joao).unwrap();
        assert!(t.motor.entrar("joao", "provisoria-joao-2", None).is_ok());

        // A tela desenha a lista, o cadastro e a edição.
        let mut e = EstadoUsuarios {
            novo: Some(FormNovo::default()),
            ..EstadoUsuarios::default()
        };
        let lista = t.motor.usuarios().unwrap();
        let ctx = egui::Context::default();
        cardeal_ui::tokens::instalar_fontes(&ctx);
        cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
        let quadro = |e: &mut EstadoUsuarios| {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    secao(ui, &t.motor, &t.sessao, e, &lista);
                    secao_minha_senha(ui, &t.motor, &t.sessao, e);
                });
            });
        };
        quadro(&mut e);
        e.edicao = Some(FormEdicao {
            usuario: joao,
            nome: "João".into(),
            ativo: true,
            segundo_fator: false,
            papel: Some(PapelDeFabrica::Vendedor),
            papel_atual: Some(PapelDeFabrica::Vendedor),
            senha: String::new(),
        });
        quadro(&mut e);
    }
}
