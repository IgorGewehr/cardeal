//! Tela de Vendas — lista de pedidos + dialog (ver / avançar estado).
//! `docs/modulos/vendas.md`.
//!
//! **Escopo desta versão:** ver os pedidos e mover a máquina de estados
//! (confirmar / faturar / cancelar). Criar pedido pela UI ainda depende de consultas de
//! listagem de **condição de pagamento** e **tabela de preço** no backend — sem elas o
//! formulário de criação não tem como preencher esses campos obrigatórios de `CriarPedido`.

use std::collections::HashMap;

use cardeal_cliente::{MotorLocal, SessaoLocal};
use cardeal_kernel::Id;
use cardeal_modkit::Icone;
use cardeal_ui::atoms::{Botao, Rotulo, ValorDinheiro};
use cardeal_ui::organisms::{ColunaGrade, Dialogo, Grade, LayoutTela};
use cardeal_ui::tokens::{Espaco, TemaUi};
use eframe::egui;
use mod_clientes::{Papel, PessoasPorPapel};
use mod_vendas::{
    CancelarPedido, ConfirmarPedido, EstadoPedido, FaturarPedido, ItemPedido, PedidosRecentes,
};

#[derive(Default)]
enum Dlg {
    #[default]
    Fechado,
    Ver(usize),
}

/// Estado local da tela.
#[derive(Default)]
pub struct EstadoTelaVendas {
    pedidos: Vec<ItemPedido>,
    nomes: HashMap<Id, String>,
    erro: Option<String>,
    dlg: Dlg,
}

impl EstadoTelaVendas {
    /// Recarrega a lista de pedidos e o índice de nomes de cliente.
    pub fn carregar(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        match motor.consultar(sessao, "vendas.pedidos_recentes.v1", &PedidosRecentes) {
            Ok(p) => {
                self.pedidos = p;
                self.erro = None;
            }
            Err(e) => self.erro = Some(e.mensagem),
        }
        if let Ok(c) = motor.consultar(
            sessao,
            "clientes.pessoas_por_papel.v1",
            &PessoasPorPapel { papel: Papel::Cliente, busca: None },
        ) {
            self.nomes = c.into_iter().map(|p| (p.pessoa, p.nome)).collect();
        }
    }

    fn nome(&self, cliente: Id) -> String {
        self.nomes.get(&cliente).cloned().unwrap_or_else(|| "—".to_owned())
    }
}

/// Desenha a tela inteira.
pub fn mostrar(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaVendas,
) {
    LayoutTela::nova("Vendas").mostrar(
        ui,
        estado,
        |ui, estado| {
            if ui.add(Botao::secundario("Recarregar").atalho("F5")).clicked() {
                estado.carregar(motor, sessao);
            }
            ui.add(
                Botao::primario("+ Novo pedido")
                    .habilitado(false),
            )
            .on_hover_text(
                "Criar pedido pela UI aguarda as consultas de condição de pagamento e \
                 tabela de preço no mod-vendas.",
            );
        },
        |ui, estado| {
            if let Some(erro) = &estado.erro {
                ui.add(Rotulo::interface(erro.clone()).quebravel().cor(ui.cores().negativo));
                ui.add_space(Espaco::E12);
            }
            lista(ui, estado);
        },
    );

    if let Dlg::Ver(i) = estado.dlg {
        dialogo_ver(ui.ctx(), motor, sessao, estado, i);
    }
}

fn lista(ui: &mut egui::Ui, estado: &mut EstadoTelaVendas) {
    if estado.pedidos.is_empty() {
        cardeal_ui::molecules::EstadoVazio::novo(Icone::Carrinho, "Nenhum pedido ainda.")
            .mostrar(ui);
        return;
    }
    let colunas = vec![
        ColunaGrade::nova("Cliente"),
        ColunaGrade::nova("Data").largura(120.0),
        ColunaGrade::nova("Itens").largura(70.0),
        ColunaGrade::nova("Total").largura(130.0),
        ColunaGrade::nova("Estado").largura(120.0),
    ];
    let clicada = Grade::nova(colunas).selecionavel(None).mostrar(
        ui,
        estado.pedidos.len(),
        |i, row| {
            let p = &estado.pedidos[i];
            row.col(|ui| {
                ui.add(Rotulo::interface(estado.nome(p.cliente)));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(p.data.to_string()));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(p.itens.to_string()));
            });
            row.col(|ui| {
                ui.add(ValorDinheiro::novo(p.total));
            });
            row.col(|ui| {
                ui.add(Rotulo::campo(p.estado.rotulo()));
            });
        },
    );
    if let Some(i) = clicada {
        estado.dlg = Dlg::Ver(i);
    }
}

fn dialogo_ver(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaVendas,
    i: usize,
) {
    let Some(p) = estado.pedidos.get(i).cloned() else {
        estado.dlg = Dlg::Fechado;
        return;
    };
    let nome = estado.nome(p.cliente);

    let fechar = Dialogo::nova(format!("Pedido · {nome}")).largura(560.0).mostrar(
        ctx,
        estado,
        |ui, _estado| {
            ui.columns(2, |c| {
                kv(&mut c[0], "Data", &p.data.to_string());
                kv(&mut c[1], "Estado", p.estado.rotulo());
            });
            ui.columns(2, |c| {
                kv(&mut c[0], "Itens", &p.itens.to_string());
                kv(&mut c[1], "Total", &p.total.formatar_com_simbolo());
            });
            ui.add_space(Espaco::E12);
            ui.separator();
            ui.add_space(Espaco::E12);
            acoes(ui, motor, sessao, _estado, &p);
        },
        |ui, estado| {
            if ui.add(Botao::secundario("Fechar")).clicked() {
                estado.dlg = Dlg::Fechado;
            }
        },
    );
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

fn acoes(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaVendas,
    p: &ItemPedido,
) {
    match p.estado {
        EstadoPedido::Rascunho => {
            ui.horizontal(|ui| {
                if ui.add(Botao::primario("Confirmar pedido")).clicked() {
                    aplicar(motor, sessao, estado, "vendas.confirmar_pedido.v1", &ConfirmarPedido {
                        pedido: p.pedido,
                    });
                }
                if ui.add(Botao::destrutivo("Cancelar pedido")).clicked() {
                    aplicar(motor, sessao, estado, "vendas.cancelar_pedido.v1", &CancelarPedido {
                        pedido: p.pedido,
                    });
                }
            });
        }
        EstadoPedido::Confirmado => {
            ui.horizontal(|ui| {
                if ui.add(Botao::primario("Faturar à vista")).clicked() {
                    aplicar(motor, sessao, estado, "vendas.faturar_pedido.v1", &FaturarPedido {
                        pedido: p.pedido,
                        a_vista: true,
                    });
                }
                if ui.add(Botao::secundario("Faturar a prazo")).clicked() {
                    aplicar(motor, sessao, estado, "vendas.faturar_pedido.v1", &FaturarPedido {
                        pedido: p.pedido,
                        a_vista: false,
                    });
                }
                if ui.add(Botao::destrutivo("Cancelar")).clicked() {
                    aplicar(motor, sessao, estado, "vendas.cancelar_pedido.v1", &CancelarPedido {
                        pedido: p.pedido,
                    });
                }
            });
        }
        EstadoPedido::Faturado
        | EstadoPedido::EmEntrega
        | EstadoPedido::Concluido
        | EstadoPedido::Cancelado => {
            ui.add(Rotulo::campo("Sem ações disponíveis neste estado."));
        }
    }
}

fn aplicar<C: cardeal_modkit::Comando + serde::Serialize>(
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaVendas,
    nome: &str,
    comando: &C,
) where
    C::Saida: serde::de::DeserializeOwned,
{
    match motor.executar(sessao, nome, comando) {
        Ok(_) => {
            estado.dlg = Dlg::Fechado;
            estado.carregar(motor, sessao);
        }
        Err(e) => estado.erro = Some(e.mensagem),
    }
}

fn kv(ui: &mut egui::Ui, chave: &str, valor: &str) {
    ui.add(Rotulo::campo(chave));
    ui.add(Rotulo::interface(if valor.trim().is_empty() { "—" } else { valor }));
    ui.add_space(Espaco::E8);
}
