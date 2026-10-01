//! A lista de clientes: a mesma receita de listagem do desktop (cabeçalho → KPI → `Grade`), e
//! a `Grade` mostra esqueleto enquanto a consulta não volta.

use cardeal_cliente::remoto::protocolo;
use cardeal_protocol::EmpresaAcessivel;
use cardeal_ui::atoms::{Botao, Rotulo};
use cardeal_ui::molecules::{CabecalhoTela, CartaoKpi};
use cardeal_ui::organisms::{ColunaGrade, Grade};
use cardeal_ui::tokens::Espaco;
use mod_clientes::{ItemPessoa, Papel, PessoasPorPapel};

use super::login;
use crate::app::Tela;
use crate::rede::{disparar, Pendente};

/// O estado da lista.
pub struct Estado {
    empresa: EmpresaAcessivel,
    lista: Option<Vec<ItemPessoa>>,
    carregando: Option<Pendente<Vec<ItemPessoa>>>,
    erro: Option<String>,
}

impl Estado {
    /// Entra na lista já pedindo os dados.
    pub fn novo(ctx: &egui::Context, empresa: EmpresaAcessivel) -> Self {
        let mut e = Self {
            empresa,
            lista: None,
            carregando: None,
            erro: None,
        };
        e.recarregar(ctx);
        e
    }

    fn recarregar(&mut self, ctx: &egui::Context) {
        let carga = protocolo::carga(&PessoasPorPapel {
            papel: Papel::Cliente,
            busca: None,
        });
        match carga {
            Ok(c) => {
                let pedido =
                    protocolo::consulta(self.empresa.id, "clientes.pessoas_por_papel.v1", c);
                self.carregando = Some(disparar(ctx, pedido));
            }
            Err(e) => self.erro = Some(e.mensagem),
        }
    }
}

/// Desenha a lista; devolve o login quando a pessoa sai.
pub fn mostrar(ui: &mut egui::Ui, e: &mut Estado) -> Option<Tela> {
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
    let mut recarregar = ui.input(|i| i.key_pressed(egui::Key::F5));
    let mut sair = false;
    CabecalhoTela::novo(format!("Clientes — {}", e.empresa.nome)).mostrar(ui, |ui| {
        recarregar |= ui
            .add(Botao::secundario("Atualizar").atalho("F5"))
            .clicked();
        sair = ui.add(Botao::fantasma("Sair")).clicked();
    });
    if sair {
        // Melhor esforço: sem rede, o cookie expira sozinho; a tela volta ao login já.
        let _: Pendente<()> = disparar_vazio(ui.ctx());
        return Some(Tela::Login(login::Estado::default()));
    }
    if recarregar && e.carregando.is_none() {
        e.recarregar(ui.ctx());
    }
    ui.add_space(Espaco::E16);
    ui.add(CartaoKpi::contagem(
        "Clientes",
        e.lista.as_ref().map_or(0, Vec::len),
    ));
    ui.add_space(Espaco::E16);
    if let Some(m) = &e.erro {
        ui.add(Rotulo::apoio(m.clone()));
        ui.add_space(Espaco::E8);
    }
    let linhas: &[ItemPessoa] = e.lista.as_deref().unwrap_or(&[]);
    Grade::nova(vec![
        ColunaGrade::nova("Nome"),
        ColunaGrade::nova("Documento").largura(180.0),
        ColunaGrade::nova("Telefone").largura(180.0),
    ])
    .id_salt("web-clientes")
    .carregando(e.carregando.is_some() && e.lista.is_none())
    .vazio("Nenhum cliente cadastrado.")
    .mostrar(ui, linhas.len(), |i, row| {
        let p = &linhas[i];
        row.col(|ui| {
            ui.add(Rotulo::interface(p.nome.clone()));
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(p.documento.clone().unwrap_or_default()));
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(p.telefone.clone().unwrap_or_default()));
        });
    });
    None
}

fn disparar_vazio(ctx: &egui::Context) -> Pendente<()> {
    crate::rede::disparar_sem_corpo(ctx, protocolo::logout())
}
