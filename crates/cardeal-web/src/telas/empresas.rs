//! A escolha de empresa — com uma só, entra direto.

use cardeal_cliente::remoto::protocolo;
use cardeal_kernel::Id;
use cardeal_protocol::{EmpresaAcessivel, InfoSessao, RespostaLogin};
use cardeal_ui::atoms::{Botao, Rotulo};
use cardeal_ui::organisms::Cartao;
use cardeal_ui::tokens::Espaco;

use super::clientes;
use crate::app::Tela;
use crate::rede::{disparar, Pendente};

/// O estado da escolha.
pub struct Estado {
    nome: String,
    empresas: Vec<EmpresaAcessivel>,
    entrando: Option<(Id, Pendente<InfoSessao>)>,
    erro: Option<String>,
}

impl Estado {
    /// A partir do login aceito.
    pub fn novo(login: RespostaLogin) -> Self {
        Self {
            nome: login.nome,
            empresas: login.empresas,
            entrando: None,
            erro: None,
        }
    }
}

/// Desenha; devolve a próxima tela quando a sessão na empresa chega.
pub fn mostrar(ui: &mut egui::Ui, e: &mut Estado) -> Option<Tela> {
    if let Some((id, r)) = e
        .entrando
        .as_ref()
        .and_then(|(id, p)| p.pronto().map(|r| (*id, r)))
    {
        e.entrando = None;
        match r {
            Ok(_sessao) => {
                let empresa = e.empresas.iter().find(|x| x.id == id).cloned()?;
                return Some(Tela::Clientes(clientes::Estado::novo(ui.ctx(), empresa)));
            }
            Err(erro) => e.erro = Some(erro.mensagem),
        }
    }
    if e.empresas.len() == 1 && e.entrando.is_none() && e.erro.is_none() {
        let id = e.empresas[0].id;
        e.entrando = Some((id, disparar(ui.ctx(), protocolo::sessao_empresa(id))));
    }
    ui.add_space(Espaco::E64);
    ui.vertical_centered(|ui| {
        Cartao::novo().largura(380.0).mostrar(ui, |ui| {
            ui.add(Rotulo::apoio(format!("Olá, {}. Em qual empresa?", e.nome)));
            ui.add_space(Espaco::E12);
            for empresa in &e.empresas {
                let clicou = ui
                    .add(Botao::secundario(empresa.nome.clone()).preenche_largura())
                    .clicked();
                if clicou && e.entrando.is_none() {
                    let id = empresa.id;
                    e.entrando = Some((id, disparar(ui.ctx(), protocolo::sessao_empresa(id))));
                }
                ui.add_space(Espaco::E8);
            }
            if let Some(m) = &e.erro {
                ui.add(Rotulo::apoio(m.clone()));
            }
        });
    });
    None
}
