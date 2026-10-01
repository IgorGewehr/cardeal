//! O app: login → empresa → clientes. Só componentes do `cardeal-ui` (ADR-0015), o mesmo
//! visual do desktop. Cada tela devolve a próxima quando muda.

use cardeal_ui::organisms::Janela;
use cardeal_ui::tokens::{instalar_estilo, instalar_fontes, Tema};

use crate::telas::{clientes, empresas, login};

/// Em que tela o app está.
pub enum Tela {
    /// E-mail e senha.
    Login(login::Estado),
    /// Conta com mais de uma empresa (ou entrando na única).
    Empresas(empresas::Estado),
    /// A lista de clientes da empresa.
    Clientes(clientes::Estado),
}

/// O app web.
pub struct App {
    tela: Tela,
}

impl App {
    /// Instala fontes e tema do design system.
    pub fn novo(ctx: &egui::Context) -> Self {
        instalar_fontes(ctx);
        instalar_estilo(ctx, Tema::Claro);
        Self {
            tela: Tela::Login(login::Estado::default()),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // A mesma moldura do desktop (fundo do tema, margem de 24 px), sem cantos: no
        // navegador não há janela própria.
        let moldura = Janela::nova(0.0, 0.0).moldura_conteudo(Tema::Claro, false);
        let proxima = egui::CentralPanel::default()
            .frame(moldura)
            .show(ctx, |ui| match &mut self.tela {
                Tela::Login(e) => login::mostrar(ui, e),
                Tela::Empresas(e) => empresas::mostrar(ui, e),
                Tela::Clientes(e) => {
                    clientes::mostrar(ui, e);
                    None
                }
            })
            .inner;
        if let Some(t) = proxima {
            self.tela = t;
        }
    }
}
