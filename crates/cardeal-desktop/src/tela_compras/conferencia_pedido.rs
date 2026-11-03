//! A seção **Pedido de compra** na conferência da nota: o pedido a que a nota se ligou (e como),
//! item a item o que se pediu, o que já chegou, o que esta nota traz e o que falta, e o que
//! diverge (quantidade a mais, preço acima, item fora do pedido). Sem pedido, oferece ligar a um
//! dos pedidos do fornecedor que aguardam entrega.

use cardeal_cliente::{Motor, Sessao};
use cardeal_kernel::{Id, Preco};
use cardeal_ui::atoms::{Botao, Etiqueta, Rotulo, Tom};
use cardeal_ui::molecules::SeletorOpcao;
use cardeal_ui::organisms::{notificar, ColunaGrade, Grade, Notificacao, Painel};
use cardeal_ui::tokens::{Espaco, TemaUi};
use eframe::egui;
use mod_compras::pedido::{ConferenciaDoPedido, ResumoPedido};
use mod_compras::{ConferenciaComPedido, PedidosDeCompra, VincularNotaAoPedido};

/// O que a seção mostra da nota aberta.
#[derive(Default)]
pub struct EstadoConferenciaPedido {
    conferencia: Option<ConferenciaDoPedido>,
    /// Os pedidos do fornecedor que aguardam entrega (para ligar a nota sem pedido).
    abertos: Vec<ResumoPedido>,
    escolha: Option<Id>,
}

impl EstadoConferenciaPedido {
    /// Carrega a conferência da nota e os pedidos em aberto do fornecedor.
    pub fn carregar(motor: &Motor, sessao: &Sessao, nota: Id, fornecedor: Id) -> Self {
        if !sessao.concede("compras.pedido.ver") {
            return Self::default();
        }
        let conferencia = motor
            .consultar(
                sessao,
                "compras.conferencia_com_pedido.v1",
                &ConferenciaComPedido { nota_entrada: nota },
            )
            .ok()
            .flatten();
        let abertos = motor
            .consultar(
                sessao,
                "compras.pedidos.v1",
                &PedidosDeCompra {
                    so_em_andamento: true,
                    fornecedor: Some(fornecedor),
                },
            )
            .map(|v| {
                v.into_iter()
                    .filter(|p: &ResumoPedido| p.estado.aguardando())
                    .collect()
            })
            .unwrap_or_default();
        Self {
            conferencia,
            abertos,
            escolha: None,
        }
    }

    /// A nota diverge do pedido.
    pub fn diverge(&self) -> bool {
        self.conferencia
            .as_ref()
            .is_some_and(|c| !c.divergencias.is_empty())
    }
}

fn ligar(ui: &egui::Ui, motor: &Motor, sessao: &Sessao, nota: Id, pedido: Option<Id>) -> bool {
    match motor.executar(
        sessao,
        "compras.vincular_nota_ao_pedido.v1",
        &VincularNotaAoPedido {
            nota_entrada: nota,
            pedido,
        },
    ) {
        Ok(()) => {
            notificar(
                ui.ctx(),
                Notificacao::sucesso(if pedido.is_some() {
                    "Nota ligada ao pedido"
                } else {
                    "Nota desligada do pedido"
                }),
            );
            true
        }
        Err(e) => {
            notificar(ui.ctx(), Notificacao::erro(e.mensagem));
            false
        }
    }
}

fn preco(p: Option<Preco>) -> String {
    p.map_or_else(|| "—".to_owned(), Preco::formatar)
}

/// Desenha a seção. `editavel`: a nota ainda não entrou (dá para ligar e desligar). Devolve
/// `true` quando a ligação mudou (a tela recarrega a seção).
pub fn secao(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoConferenciaPedido,
    nota: Id,
    editavel: bool,
) -> bool {
    let pode = editavel && sessao.concede("compras.entrada.conferir");
    let Some(c) = &estado.conferencia else {
        if !pode || estado.abertos.is_empty() {
            return false;
        }
        let mut mudou = false;
        Painel::novo().realce(Tom::Neutro).mostrar(ui, |ui| {
            ui.add(Rotulo::titulo_secao("Pedido de compra"));
            ui.add(
                Rotulo::apoio(format!(
                    "Este fornecedor tem {} pedido(s) aguardando entrega. Ligue a nota ao pedido \
                     para conferir o que veio contra o que se pediu.",
                    estado.abertos.len()
                ))
                .quebravel(),
            );
            ui.add_space(Espaco::E4);
            let ops: Vec<(Id, String)> = estado
                .abertos
                .iter()
                .map(|p| {
                    (
                        p.pedido,
                        format!(
                            "Pedido {:04} — {} — {}",
                            p.numero,
                            p.data.formatar_curta(),
                            p.total.formatar_com_simbolo()
                        ),
                    )
                })
                .collect();
            ui.horizontal(|ui| {
                ui.allocate_ui(egui::vec2(420.0, 40.0), |ui| {
                    SeletorOpcao::novo("Pedido", &mut estado.escolha)
                        .opcoes(ops)
                        .placeholder("Escolha o pedido…")
                        .mostrar(ui);
                });
                if let Some(p) = estado.escolha {
                    if ui
                        .add(Botao::secundario("Ligar ao pedido").pequeno())
                        .clicked()
                    {
                        mudou = ligar(ui, motor, sessao, nota, Some(p));
                    }
                }
            });
        });
        return mudou;
    };
    let mut mudou = false;
    let tom = if c.divergencias.is_empty() {
        Tom::Info
    } else {
        Tom::Atencao
    };
    Painel::novo().realce(tom).mostrar(ui, |ui| {
        ui.horizontal(|ui| {
            ui.add(Rotulo::titulo_secao(format!(
                "Pedido de compra {:04}",
                c.numero
            )));
            if c.divergencias.is_empty() {
                ui.add(Etiqueta::positiva(if c.completa_o_pedido {
                    "confere — completa o pedido"
                } else {
                    "confere"
                }));
            } else {
                ui.add(Etiqueta::atencao("diverge do pedido"));
            }
        });
        if !c.ligacao.is_empty() {
            ui.add(Rotulo::apoio(format!("Ligada {}.", c.ligacao)));
        }
        ui.add_space(Espaco::E4);
        let colunas = vec![
            ColunaGrade::nova("Produto"),
            ColunaGrade::nova("Pedido").largura(70.0).numero(),
            ColunaGrade::nova("Já chegou").largura(80.0).numero(),
            ColunaGrade::nova("Nesta nota").largura(84.0).numero(),
            ColunaGrade::nova("Falta").largura(64.0).numero(),
            ColunaGrade::nova("Preço pedido").largura(96.0).numero(),
            ColunaGrade::nova("Preço nota").largura(90.0).numero(),
        ];
        Grade::nova(colunas)
            .id_salt("conferencia_pedido")
            .mostrar(ui, c.linhas.len(), |i, row| {
                let l = &c.linhas[i];
                row.col(|ui| {
                    ui.add(Rotulo::interface(l.descricao.clone()));
                });
                row.col(|ui| {
                    ui.add(Rotulo::interface(l.pedida.formatar(0)));
                });
                row.col(|ui| {
                    ui.add(Rotulo::interface(l.recebida_antes.formatar(0)));
                });
                row.col(|ui| {
                    let r = Rotulo::interface(l.nesta_nota.formatar(0));
                    ui.add(if l.a_mais().e_positiva() {
                        r.cor(ui.cores().atencao)
                    } else {
                        r
                    });
                });
                row.col(|ui| {
                    ui.add(Rotulo::interface(l.faltando().formatar(0)));
                });
                row.col(|ui| {
                    ui.add(Rotulo::interface(l.preco_pedido.formatar()));
                });
                row.col(|ui| {
                    let r = Rotulo::interface(preco(l.preco_na_nota));
                    ui.add(if l.preco_acima() {
                        r.cor(ui.cores().atencao)
                    } else {
                        r
                    });
                });
            });
        for d in &c.divergencias {
            ui.add(Rotulo::campo(format!("• {d}")).quebravel());
        }
        if c.sem_produto > 0 {
            ui.add(
                Rotulo::apoio(format!(
                    "{} item(ns) da nota ainda sem produto ficam fora da conferência — vincule \
                     abaixo.",
                    c.sem_produto
                ))
                .quebravel(),
            );
        }
        if !c.divergencias.is_empty() {
            ui.add(
                Rotulo::apoio(
                    "A nota que diverge não entra sozinha: confira com o fornecedor e confirme \
                     a entrada se estiver certo.",
                )
                .quebravel(),
            );
        }
        if pode {
            ui.add_space(Espaco::E4);
            if ui
                .add(Botao::fantasma("Desligar do pedido").pequeno())
                .clicked()
            {
                mudou = ligar(ui, motor, sessao, nota, None);
            }
        }
    });
    mudou
}
