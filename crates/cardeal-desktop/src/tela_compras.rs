//! Tela de Compras — notas de entrada + dialog (ver / confirmar entrada).
//! `docs/modulos/compras.md`.
//!
//! **Escopo desta versão:** ver as notas e confirmar a entrada (baixa no estoque + título a
//! pagar). Lançar nota manual pela UI ainda precisa de um formulário de itens e do
//! casamento produto↔item — próximo passo.

use std::collections::HashMap;

use cardeal_cliente::{MotorLocal, SessaoLocal};
use cardeal_kernel::Id;
use cardeal_modkit::Icone;
use cardeal_ui::atoms::{Botao, Rotulo, ValorDinheiro};
use cardeal_ui::molecules::{EstadoVazio, SeletorOpcao};
use cardeal_ui::organisms::{ColunaGrade, Dialogo, Grade, LayoutTela};
use cardeal_ui::tokens::{Espaco, TemaUi};
use eframe::egui;
use mod_clientes::{Papel, PessoasPorPapel};
use mod_compras::{ConfirmarEntrada, EstadoNotaEntrada, ItemNota, NotasRecentes};
use mod_estoque::{ItemLocal, Locais};

#[derive(Default)]
enum Dlg {
    #[default]
    Fechado,
    Ver(usize),
}

/// Estado local da tela.
#[derive(Default)]
pub struct EstadoTelaCompras {
    notas: Vec<ItemNota>,
    fornecedores: HashMap<Id, String>,
    locais: Vec<ItemLocal>,
    local_sel: Option<Id>,
    erro: Option<String>,
    dlg: Dlg,
}

impl EstadoTelaCompras {
    /// Recarrega notas, nomes de fornecedor e locais de estoque.
    pub fn carregar(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        match motor.consultar(sessao, "compras.notas_recentes.v1", &NotasRecentes) {
            Ok(n) => {
                self.notas = n;
                self.erro = None;
            }
            Err(e) => self.erro = Some(e.mensagem),
        }
        if let Ok(f) = motor.consultar(
            sessao,
            "clientes.pessoas_por_papel.v1",
            &PessoasPorPapel { papel: Papel::Fornecedor, busca: None },
        ) {
            self.fornecedores = f.into_iter().map(|p| (p.pessoa, p.nome)).collect();
        }
        if let Ok(l) = motor.consultar(sessao, "estoque.locais.v1", &Locais) {
            self.locais = l;
        }
    }

    fn nome(&self, fornecedor: Id) -> String {
        self.fornecedores
            .get(&fornecedor)
            .cloned()
            .unwrap_or_else(|| "—".to_owned())
    }
}

/// Desenha a tela inteira.
pub fn mostrar(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaCompras,
) {
    LayoutTela::nova("Compras").mostrar(
        ui,
        estado,
        |ui, estado| {
            if ui.add(Botao::secundario("Recarregar").atalho("F5")).clicked() {
                estado.carregar(motor, sessao);
            }
            ui.add(Botao::primario("+ Nova nota").habilitado(false))
                .on_hover_text(
                    "Lançar nota manual pela UI aguarda o formulário de itens + casamento \
                     produto no mod-compras.",
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

fn lista(ui: &mut egui::Ui, estado: &mut EstadoTelaCompras) {
    if estado.notas.is_empty() {
        EstadoVazio::novo(Icone::Nota, "Nenhuma nota de entrada.").mostrar(ui);
        return;
    }
    let colunas = vec![
        ColunaGrade::nova("Fornecedor"),
        ColunaGrade::nova("Nº / série").largura(120.0),
        ColunaGrade::nova("Emissão").largura(120.0),
        ColunaGrade::nova("Itens").largura(70.0),
        ColunaGrade::nova("Total").largura(130.0),
        ColunaGrade::nova("Estado").largura(110.0),
    ];
    let clicada = Grade::nova(colunas).selecionavel(None).mostrar(
        ui,
        estado.notas.len(),
        |i, row| {
            let n = &estado.notas[i];
            row.col(|ui| {
                ui.add(Rotulo::interface(estado.nome(n.fornecedor)));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(format!("{} / {}", n.numero, n.serie)));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(n.data_emissao.to_string()));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(n.itens.to_string()));
            });
            row.col(|ui| {
                ui.add(ValorDinheiro::novo(n.valor_total));
            });
            row.col(|ui| {
                ui.add(Rotulo::campo(n.estado.rotulo()));
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
    estado: &mut EstadoTelaCompras,
    i: usize,
) {
    let Some(n) = estado.notas.get(i).cloned() else {
        estado.dlg = Dlg::Fechado;
        return;
    };
    let nome = estado.nome(n.fornecedor);
    let ops_local: Vec<(Id, String)> = estado
        .locais
        .iter()
        .map(|l| (l.id, l.nome.clone()))
        .collect();

    let fechar = Dialogo::nova(format!("Nota {} · {nome}", n.numero))
        .largura(600.0)
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                ui.columns(2, |c| {
                    kv(&mut c[0], "Série", &n.serie);
                    kv(&mut c[1], "Emissão", &n.data_emissao.to_string());
                });
                ui.columns(2, |c| {
                    kv(&mut c[0], "Itens", &n.itens.to_string());
                    kv(&mut c[1], "Total", &n.valor_total.formatar_com_simbolo());
                });
                kv(ui, "Estado", n.estado.rotulo());

                if matches!(n.estado, EstadoNotaEntrada::Conferida) {
                    ui.add_space(Espaco::E12);
                    ui.separator();
                    ui.add_space(Espaco::E12);
                    ui.add(Rotulo::titulo_secao("Confirmar entrada"));
                    ui.add_space(Espaco::E8);
                    SeletorOpcao::novo("Local que recebe", &mut estado.local_sel)
                        .opcoes(ops_local.clone())
                        .mostrar(ui);
                    ui.add_space(Espaco::E8);
                    if ui.add(Botao::primario("Confirmar entrada")).clicked() {
                        confirmar(motor, sessao, estado, n.nota);
                    }
                } else if matches!(n.estado, EstadoNotaEntrada::AConferir) {
                    ui.add_space(Espaco::E8);
                    ui.add(
                        Rotulo::interface(
                            "Itens ainda em conferência/casamento — confirme quando estiver Conferida.",
                        )
                        .quebravel()
                        .cor(ui.cores().atencao),
                    );
                }
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

fn confirmar(motor: &MotorLocal, sessao: &SessaoLocal, estado: &mut EstadoTelaCompras, nota: Id) {
    let Some(local) = estado.local_sel else {
        estado.erro = Some("Escolha o local de estoque que recebe.".to_owned());
        return;
    };
    match motor.executar(
        sessao,
        "compras.confirmar_entrada.v1",
        &ConfirmarEntrada {
            nota_entrada: nota,
            local,
            gerar_titulo_a_pagar: None,
        },
    ) {
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
