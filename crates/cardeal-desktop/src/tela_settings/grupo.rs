//! O cartão **CNPJs do grupo** (Configurações › Empresa, ADR-0017): os CNPJs em que a pessoa
//! entra e, para quem gerencia a organização, cadastrar outro — com plano de contas próprio, e
//! quem cadastra vira o administrador dele. A troca de CNPJ fica no rodapé da barra lateral.
//!
//! O cadastro precisa do motor por inteiro (no servidor, a lista de empresas da conta muda):
//! o cartão só faz o pedido (`pedido`), e quem é dono do motor o executa.

use cardeal_cliente::{EmpresaDoGrupo, Motor, Sessao};
use cardeal_ui::atoms::{Botao, Etiqueta, Rotulo};
use cardeal_ui::molecules::{Campo, Mascara};
use cardeal_ui::organisms::{notificar, Notificacao};
use cardeal_ui::tokens::Espaco;
use eframe::egui;

/// Quem pode cadastrar outro CNPJ na organização.
const PERMISSAO: &str = "empresa.organizacao.gerenciar";

/// O que o cartão mostra.
#[derive(Default)]
pub struct EstadoCnpjs {
    empresas: Vec<EmpresaDoGrupo>,
    atual: Option<cardeal_kernel::Id>,
    aberto: bool,
    razao: String,
    cnpj: String,
}

impl EstadoCnpjs {
    /// Os CNPJs do grupo.
    pub fn carregar(motor: &Motor, sessao: &Sessao) -> Self {
        Self {
            empresas: motor.empresas_do_grupo(sessao).unwrap_or_default(),
            atual: motor.empresa_da_sessao(sessao),
            ..Self::default()
        }
    }
}

/// Desenha o cartão. Devolve o pedido de cadastro (razão social, CNPJ), já conferido.
pub fn secao(ui: &mut egui::Ui, sessao: &Sessao, e: &mut EstadoCnpjs) -> Option<(String, String)> {
    ui.add(Rotulo::titulo_secao("CNPJs do grupo"));
    ui.add_space(Espaco::E4);
    ui.add(
        Rotulo::apoio(
            "Matriz e filiais, ou as empresas do mesmo dono, numa base só: clientes, fornecedores \
             e o catálogo de produtos são do grupo; estoque, financeiro, fiscal e vendas são de \
             cada CNPJ. Troque de CNPJ pelo nome da empresa no rodapé da barra lateral.",
        )
        .quebravel(),
    );
    ui.add_space(Espaco::E8);
    for emp in &e.empresas {
        ui.horizontal(|ui| {
            ui.add(Rotulo::interface(emp.nome.clone()));
            if let Some(c) = &emp.cnpj {
                ui.add(Rotulo::campo(c.clone()));
            }
            if emp.matriz {
                ui.add(Etiqueta::neutra("matriz"));
            }
            if Some(emp.id) == e.atual {
                ui.add(Etiqueta::info("você está aqui"));
            }
        });
    }
    if !sessao.concede(PERMISSAO) {
        return None;
    }
    ui.add_space(Espaco::E8);
    if !e.aberto {
        if ui
            .add(Botao::secundario("Adicionar CNPJ ao grupo"))
            .clicked()
        {
            e.aberto = true;
        }
        return None;
    }
    ui.add(Campo::novo("Razão social", &mut e.razao));
    ui.add_space(Espaco::E8);
    ui.add(Campo::novo("CNPJ", &mut e.cnpj).mascara(Mascara::Documento));
    ui.add_space(Espaco::E8);
    let mut pedido = None;
    ui.horizontal(|ui| {
        if ui.add(Botao::primario("Adicionar")).clicked() {
            if e.razao.trim().is_empty() {
                notificar(ui.ctx(), Notificacao::aviso("Informe a razão social."));
            } else if let Err(erro) = cardeal_kernel::Cnpj::novo(&e.cnpj) {
                notificar(ui.ctx(), Notificacao::aviso(erro.mensagem));
            } else {
                pedido = Some((e.razao.trim().to_owned(), e.cnpj.trim().to_owned()));
            }
        }
        if ui.add(Botao::fantasma("Cancelar")).clicked() {
            e.aberto = false;
        }
    });
    pedido
}
