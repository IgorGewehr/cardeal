//! Tela de Financeiro — Contas a Receber / a Pagar. `docs/modulos/financeiro.md`.
//!
//! Duas abas (a receber / a pagar), cada uma a lista inteira de parcelas em aberto.
//! "+ Lançar título" e o clique numa linha abrem o `Dialogo` — ver a parcela e **dar baixa**.

use std::collections::HashMap;

use cardeal_cliente::{MotorLocal, SessaoLocal};
use cardeal_kernel::{Data, Dinheiro, Fuso, Id};
use cardeal_ledger::Contraparte;
use cardeal_modkit::Icone;
use cardeal_ui::atoms::{Botao, Rotulo, ValorDinheiro};
use cardeal_ui::molecules::{Campo, EstadoVazio, SeletorOpcao};
use cardeal_ui::organisms::{ColunaGrade, Dialogo, Grade, LayoutTela};
use cardeal_ui::tokens::{Espaco, TemaUi};
use eframe::egui;
use mod_clientes::{ItemPessoa, Papel, PessoasPorPapel};
use mod_financeiro::{
    BaixarPagamento, BaixarRecebimento, ItemTituloEmAberto, LancarTituloAPagar,
    LancarTituloAReceber, TitulosAPagarEmAberto, TitulosAReceberEmAberto,
};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Aba {
    #[default]
    Receber,
    Pagar,
}

impl Aba {
    const fn a_receber(self) -> bool {
        matches!(self, Self::Receber)
    }
}

#[derive(Default)]
enum Dlg {
    #[default]
    Fechado,
    Lancar(FormLancar),
    Baixar { indice: usize, valor: String, data: String },
}

struct FormLancar {
    contraparte: Option<Id>,
    valor: String,
    emissao: String,
    parcelas: String,
    primeiro_vencimento: String,
    intervalo: String,
    observacao: String,
}

impl Default for FormLancar {
    fn default() -> Self {
        let hoje = Data::hoje(Fuso::BRASILIA).to_string();
        Self {
            contraparte: None,
            valor: String::new(),
            emissao: hoje.clone(),
            parcelas: "1".to_owned(),
            primeiro_vencimento: hoje,
            intervalo: "30".to_owned(),
            observacao: String::new(),
        }
    }
}

/// Estado local da tela.
#[derive(Default)]
pub struct EstadoTelaFinanceiro {
    aba: Aba,
    parcelas: Vec<ItemTituloEmAberto>,
    nomes: HashMap<Id, String>,
    clientes: Vec<ItemPessoa>,
    fornecedores: Vec<ItemPessoa>,
    erro: Option<String>,
    dlg: Dlg,
}

impl EstadoTelaFinanceiro {
    /// Recarrega a lista da aba ativa e o índice de nomes.
    pub fn carregar(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        let r = if self.aba.a_receber() {
            motor.consultar(
                sessao,
                "financeiro.titulos_a_receber_em_aberto.v1",
                &TitulosAReceberEmAberto,
            )
        } else {
            motor.consultar(
                sessao,
                "financeiro.titulos_a_pagar_em_aberto.v1",
                &TitulosAPagarEmAberto,
            )
        };
        match r {
            Ok(p) => {
                self.parcelas = p;
                self.erro = None;
            }
            Err(e) => self.erro = Some(e.mensagem),
        }

        if let Ok(c) = motor.consultar(
            sessao,
            "clientes.pessoas_por_papel.v1",
            &PessoasPorPapel { papel: Papel::Cliente, busca: None },
        ) {
            self.clientes = c;
        }
        if let Ok(f) = motor.consultar(
            sessao,
            "clientes.pessoas_por_papel.v1",
            &PessoasPorPapel { papel: Papel::Fornecedor, busca: None },
        ) {
            self.fornecedores = f;
        }
        self.nomes = self
            .clientes
            .iter()
            .chain(self.fornecedores.iter())
            .map(|p| (p.pessoa, p.nome.clone()))
            .collect();
    }

    fn nome_contraparte(&self, c: &Contraparte) -> String {
        let id = match c {
            Contraparte::Cliente(i)
            | Contraparte::Fornecedor(i)
            | Contraparte::Funcionario(i)
            | Contraparte::Socio(i)
            | Contraparte::Outro(i) => *i,
        };
        self.nomes.get(&id).cloned().unwrap_or_else(|| "—".to_owned())
    }
}

/// Desenha a tela inteira.
pub fn mostrar(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    LayoutTela::nova("Financeiro").mostrar(
        ui,
        estado,
        |ui, estado| {
            if ui.add(Botao::secundario("Recarregar").atalho("F5")).clicked() {
                estado.carregar(motor, sessao);
            }
            let rot = if estado.aba.a_receber() {
                "+ Lançar a receber"
            } else {
                "+ Lançar a pagar"
            };
            if ui.add(Botao::primario(rot).atalho("Ctrl+N")).clicked() {
                estado.dlg = Dlg::Lancar(FormLancar::default());
            }
        },
        |ui, estado| {
            abas(ui, motor, sessao, estado);
            ui.add_space(Espaco::E16);

            if let Some(erro) = &estado.erro {
                ui.add(Rotulo::interface(erro.clone()).quebravel().cor(ui.cores().negativo));
                ui.add_space(Espaco::E12);
            }
            lista(ui, estado);
        },
    );

    match estado.dlg {
        Dlg::Fechado => {}
        Dlg::Lancar(_) => dialogo_lancar(ui.ctx(), motor, sessao, estado),
        Dlg::Baixar { .. } => dialogo_baixar(ui.ctx(), motor, sessao, estado),
    }
}

fn abas(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    ui.horizontal(|ui| {
        for (aba, rotulo) in [(Aba::Receber, "A receber"), (Aba::Pagar, "A pagar")] {
            let ativa = estado.aba == aba;
            let b = if ativa {
                Botao::primario(rotulo)
            } else {
                Botao::fantasma(rotulo)
            };
            if ui.add(b).clicked() && !ativa {
                estado.aba = aba;
                estado.carregar(motor, sessao);
            }
        }
    });
}

fn lista(ui: &mut egui::Ui, estado: &mut EstadoTelaFinanceiro) {
    if estado.parcelas.is_empty() {
        let msg = if estado.aba.a_receber() {
            "Nada a receber em aberto."
        } else {
            "Nada a pagar em aberto."
        };
        EstadoVazio::novo(Icone::Dinheiro, msg).mostrar(ui);
        return;
    }
    let hoje = Data::hoje(Fuso::BRASILIA);
    let colunas = vec![
        ColunaGrade::nova("Contraparte"),
        ColunaGrade::nova("Parc.").largura(60.0),
        ColunaGrade::nova("Vencimento").largura(120.0),
        ColunaGrade::nova("Saldo").largura(130.0),
        ColunaGrade::nova("Estado").largura(110.0),
    ];
    let clicada = Grade::nova(colunas).selecionavel(None).mostrar(
        ui,
        estado.parcelas.len(),
        |i, row| {
            let p = &estado.parcelas[i];
            row.col(|ui| {
                ui.add(Rotulo::interface(estado.nome_contraparte(&p.contraparte)));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(p.numero.to_string()));
            });
            row.col(|ui| {
                let venceu = p.vencimento < hoje;
                let r = Rotulo::interface(p.vencimento.to_string());
                ui.add(if venceu { r.cor(ui.cores().negativo) } else { r });
            });
            row.col(|ui| {
                ui.add(ValorDinheiro::novo(p.saldo()));
            });
            row.col(|ui| {
                ui.add(Rotulo::campo(format!("{:?}", p.estado)));
            });
        },
    );
    if let Some(i) = clicada {
        estado.dlg = Dlg::Baixar {
            indice: i,
            valor: estado.parcelas[i].saldo().formatar(),
            data: Data::hoje(Fuso::BRASILIA).to_string(),
        };
    }
}

fn dialogo_lancar(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    let a_receber = estado.aba.a_receber();
    let titulo = if a_receber {
        "Lançar título a receber"
    } else {
        "Lançar título a pagar"
    };
    let ops: Vec<(Id, String)> = if a_receber {
        estado.clientes.iter().map(|p| (p.pessoa, p.nome.clone())).collect()
    } else {
        estado.fornecedores.iter().map(|p| (p.pessoa, p.nome.clone())).collect()
    };

    let fechar = Dialogo::nova(titulo).largura(680.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            let Dlg::Lancar(f) = &mut estado.dlg else { return };
            SeletorOpcao::novo(if a_receber { "Cliente" } else { "Fornecedor" }, &mut f.contraparte)
                .opcoes(ops.clone())
                .mostrar(ui);
            ui.add_space(Espaco::E12);
            ui.columns(2, |c| {
                c[0].add(Campo::novo("Valor total", &mut f.valor).marcador("0,00"));
                c[1].add(Campo::novo("Emissão", &mut f.emissao));
            });
            ui.add_space(Espaco::E12);
            ui.columns(3, |c| {
                c[0].add(Campo::novo("Parcelas", &mut f.parcelas));
                c[1].add(Campo::novo("1º vencimento", &mut f.primeiro_vencimento));
                c[2].add(Campo::novo("Intervalo (dias)", &mut f.intervalo));
            });
            ui.add_space(Espaco::E12);
            ui.add(Campo::novo("Observação", &mut f.observacao));
        },
        |ui, estado| {
            if ui.add(Botao::primario("Lançar")).clicked() {
                lancar(motor, sessao, estado);
            }
            if ui.add(Botao::secundario("Cancelar")).clicked() {
                estado.dlg = Dlg::Fechado;
            }
        },
    );
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

fn lancar(motor: &MotorLocal, sessao: &SessaoLocal, estado: &mut EstadoTelaFinanceiro) {
    let a_receber = estado.aba.a_receber();
    let Dlg::Lancar(f) = &estado.dlg else { return };
    let Some(contraparte) = f.contraparte else {
        estado.erro = Some("Selecione a contraparte.".to_owned());
        return;
    };
    let (Ok(valor), Ok(emissao), Ok(parcelas), Ok(prim), Ok(intervalo)) = (
        f.valor.parse::<Dinheiro>(),
        f.emissao.parse::<Data>(),
        f.parcelas.parse::<u16>(),
        f.primeiro_vencimento.parse::<Data>(),
        f.intervalo.parse::<i32>(),
    ) else {
        estado.erro = Some("Confira os campos — valor 0,00, datas dd/mm/aaaa.".to_owned());
        return;
    };
    let obs = (!f.observacao.trim().is_empty()).then(|| f.observacao.clone());

    let r = if a_receber {
        motor
            .executar(
                sessao,
                "financeiro.lancar_titulo_a_receber.v1",
                &LancarTituloAReceber {
                    cliente: contraparte,
                    valor_total: valor,
                    emissao,
                    parcelas,
                    primeiro_vencimento: prim,
                    intervalo_dias: intervalo,
                    observacao: obs,
                },
            )
            .map(|_: mod_financeiro::TituloAReceberLancado| ())
    } else {
        motor
            .executar(
                sessao,
                "financeiro.lancar_titulo_a_pagar.v1",
                &LancarTituloAPagar {
                    fornecedor: contraparte,
                    valor_total: valor,
                    emissao,
                    parcelas,
                    primeiro_vencimento: prim,
                    intervalo_dias: intervalo,
                    observacao: obs,
                },
            )
            .map(|_: mod_financeiro::TituloAPagarLancado| ())
    };
    match r {
        Ok(()) => {
            estado.dlg = Dlg::Fechado;
            estado.carregar(motor, sessao);
        }
        Err(e) => estado.erro = Some(e.mensagem),
    }
}

fn dialogo_baixar(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    let Dlg::Baixar { indice, .. } = estado.dlg else { return };
    let Some(p) = estado.parcelas.get(indice).cloned() else {
        estado.dlg = Dlg::Fechado;
        return;
    };
    let nome = estado.nome_contraparte(&p.contraparte);

    let fechar = Dialogo::nova(format!("Parcela {} · {}", p.numero, nome))
        .largura(560.0)
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                ui.columns(2, |c| {
                    kv(&mut c[0], "Vencimento", &p.vencimento.to_string());
                    kv(&mut c[1], "Valor original", &p.valor_original.formatar_com_simbolo());
                });
                ui.columns(2, |c| {
                    kv(&mut c[0], "Já baixado", &p.valor_baixado.formatar_com_simbolo());
                    kv(&mut c[1], "Saldo", &p.saldo().formatar_com_simbolo());
                });
                ui.add_space(Espaco::E16);
                ui.separator();
                ui.add_space(Espaco::E12);
                ui.add(Rotulo::titulo_secao("Dar baixa"));
                ui.add_space(Espaco::E8);
                let Dlg::Baixar { valor, data, .. } = &mut estado.dlg else { return };
                ui.columns(2, |c| {
                    c[0].add(Campo::novo("Valor recebido", valor));
                    c[1].add(Campo::novo("Data", data));
                });
            },
            |ui, estado| {
                if ui.add(Botao::primario("Confirmar baixa")).clicked() {
                    baixar(motor, sessao, estado, &p);
                }
                if ui.add(Botao::secundario("Fechar")).clicked() {
                    estado.dlg = Dlg::Fechado;
                }
            },
        );
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

fn baixar(
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
    p: &ItemTituloEmAberto,
) {
    let Dlg::Baixar { valor, data, .. } = &estado.dlg else { return };
    let (Ok(valor), Ok(data)) = (valor.parse::<Dinheiro>(), data.parse::<Data>()) else {
        estado.erro = Some("Valor (0,00) ou data (dd/mm/aaaa) inválidos.".to_owned());
        return;
    };
    let r = if estado.aba.a_receber() {
        motor
            .executar(
                sessao,
                "financeiro.baixar_recebimento.v1",
                &BaixarRecebimento { parcela: p.parcela, valor, data, conta_destino: None },
            )
            .map(|_: mod_financeiro::RecebimentoBaixado| ())
    } else {
        motor
            .executar(
                sessao,
                "financeiro.baixar_pagamento.v1",
                &BaixarPagamento { parcela: p.parcela, valor, data, conta_destino: None },
            )
            .map(|_: mod_financeiro::PagamentoBaixado| ())
    };
    match r {
        Ok(()) => {
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
