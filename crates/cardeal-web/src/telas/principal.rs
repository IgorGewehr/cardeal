//! Dentro da empresa: a barra lateral (o mesmo `Sidebar` do desktop) com as áreas que a
//! sessão permite — quem não gerencia a equipe nem vê o item — e a área ativa.

use std::collections::BTreeSet;

use cardeal_cliente::remoto::{protocolo, AvisoTempoReal};
use cardeal_modkit::Icone;
use cardeal_protocol::{EmpresaAcessivel, InfoSessao};
use cardeal_ui::atoms::Botao;
use cardeal_ui::organisms::{GrupoSidebar, ItemSidebar, Janela, Sidebar};
use cardeal_ui::tokens::{Espaco, Tema};

use super::{clientes, conta, equipe, login};
use crate::app::Tela;
use crate::rede::disparar_sem_corpo;
use crate::tempo_real::Acompanhamento;

/// A área de trabalho.
pub struct Estado {
    empresa: EmpresaAcessivel,
    area: &'static str,
    clientes: clientes::Estado,
    equipe: Option<equipe::Estado>,
    conta: conta::Estado,
    /// O que os outros fazem na empresa aparece sozinho (`None`: navegador sem `EventSource`).
    tempo_real: Option<Acompanhamento>,
}

impl Estado {
    /// Entra na empresa com a sessão (usuário + permissões) que o servidor devolveu.
    pub fn novo(ctx: &egui::Context, empresa: EmpresaAcessivel, sessao: InfoSessao) -> Self {
        let permissoes: BTreeSet<String> = sessao.permissoes.into_iter().collect();
        let equipe = permissoes.contains("empresa.usuarios.ver").then(|| {
            equipe::Estado::novo(
                empresa.id,
                sessao.usuario,
                permissoes.contains("empresa.usuarios.gerenciar"),
            )
        });
        Self {
            clientes: clientes::Estado::novo(ctx, empresa.clone()),
            tempo_real: Acompanhamento::abrir(ctx, empresa.id),
            empresa,
            area: "clientes",
            equipe,
            conta: conta::Estado::default(),
        }
    }
}

/// Desenha; devolve o login quando a pessoa sai.
pub fn mostrar(ctx: &egui::Context, e: &mut Estado) -> Option<Tela> {
    for aviso in e
        .tempo_real
        .as_ref()
        .map(Acompanhamento::drenar)
        .unwrap_or_default()
    {
        if aviso == AvisoTempoReal::SessaoEncerrada {
            return Some(Tela::Login(login::Estado::default()));
        }
        if aviso.afeta("clientes") {
            e.clientes.recarregar(ctx);
        }
        if aviso.afeta("empresa") {
            if let Some(eq) = e.equipe.as_mut() {
                eq.recarregar(ctx);
            }
        }
    }
    let mut itens = vec![ItemSidebar {
        id: "clientes",
        icone: Icone::Pessoas,
        rotulo: "Clientes",
        badge: None,
    }];
    if e.equipe.is_some() {
        itens.push(ItemSidebar {
            id: "equipe",
            icone: Icone::Chave,
            rotulo: "Equipe",
            badge: None,
        });
    }
    itens.push(ItemSidebar {
        id: "conta",
        icone: Icone::Config,
        rotulo: "Minha conta",
        badge: None,
    });
    let grupos = [GrupoSidebar {
        titulo: Some(e.empresa.nome.as_str()),
        itens: &itens,
    }];

    let mut sair = false;
    egui::SidePanel::left("navegacao")
        .resizable(false)
        .exact_width(232.0)
        .frame(Janela::nova(0.0, 0.0).moldura_sidebar(Tema::Claro))
        .show(ctx, |ui| {
            ui.add_space(Espaco::E8);
            if let Some(id) = Sidebar::nova(&grupos, e.area, true).mostrar(ui) {
                if id != e.area && id == "equipe" {
                    if let Some(eq) = e.equipe.as_mut() {
                        eq.recarregar(ui.ctx());
                    }
                }
                e.area = id;
            }
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                ui.add_space(Espaco::E8);
                sair = ui.add(Botao::fantasma("Sair").preenche_largura()).clicked();
            });
        });

    egui::CentralPanel::default()
        .frame(Janela::nova(0.0, 0.0).moldura_conteudo(Tema::Claro, true))
        .show(ctx, |ui| match e.area {
            "equipe" => {
                if let Some(eq) = e.equipe.as_mut() {
                    equipe::mostrar(ui, eq);
                }
            }
            "conta" => conta::mostrar(ui, &mut e.conta),
            _ => clientes::mostrar(ui, &mut e.clientes),
        });

    if sair {
        // Melhor esforço: sem rede, o cookie expira sozinho; a tela volta ao login já.
        drop(disparar_sem_corpo(ctx, protocolo::logout()));
        return Some(Tela::Login(login::Estado::default()));
    }
    None
}
