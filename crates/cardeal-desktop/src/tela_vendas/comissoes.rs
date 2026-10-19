//! "Comissões": o fechamento do mês por vendedor — a prévia do que falta apurar, o apurado a
//! pagar e o já pago —, "Apurar mês" (lança a despesa no Razão, uma vez por pedido) e o
//! pagamento de cada vendedor de uma conta de caixa ou banco (`docs/modulos/vendas.md` §5).

use super::*;
use cardeal_cliente::UsuarioResumo;
use cardeal_kernel::{Competencia, Data, Dinheiro, Fuso};
use cardeal_ui::atoms::Tom;
use cardeal_ui::molecules::{CartaoKpi, Mascara};
use cardeal_ui::organisms::{FaixaKpi, Painel};
use mod_financeiro::{ContasDisponiveis, ItemContaDisponivel};
use mod_vendas::{
    resumo_por_vendedor, ApurarComissoes, ComissoesApuradas, ComissoesDoMes, ComissoesPagas,
    LinhaComissao, PagarComissoes, ResumoDoVendedor,
};

/// O estado do diálogo.
#[derive(Default)]
pub(super) struct EstadoComissoes {
    /// O mês escolhido; `None` = o atual.
    pub(super) mes: Option<Competencia>,
    pub(super) linhas: Vec<LinhaComissao>,
    pub(super) resumo: Vec<ResumoDoVendedor>,
    usuarios: Vec<UsuarioResumo>,
    contas: Vec<ItemContaDisponivel>,
    erro: Option<String>,
    /// O vendedor sendo pago; o formulário logo abaixo.
    pub(super) pagar: Option<Id>,
    pub(super) conta: Option<Id>,
    pub(super) data: String,
}

impl EstadoComissoes {
    pub(super) fn mes(&self) -> Competencia {
        self.mes
            .unwrap_or_else(|| Data::hoje(Fuso::BRASILIA).competencia())
    }

    pub(super) fn carregar(&mut self, motor: &Motor, sessao: &Sessao) {
        match motor.consultar(
            sessao,
            "vendas.comissoes_do_mes.v1",
            &ComissoesDoMes {
                competencia: self.mes(),
            },
        ) {
            Ok(l) => {
                self.resumo = resumo_por_vendedor(&l);
                self.linhas = l;
                self.erro = None;
            }
            Err(e) => {
                self.linhas.clear();
                self.resumo.clear();
                self.erro = Some(e.mensagem);
            }
        }
        if self.usuarios.is_empty() {
            if let Ok(u) = motor.usuarios() {
                self.usuarios = u;
            }
        }
        if self.contas.is_empty() {
            if let Ok(c) = motor.consultar(
                sessao,
                "financeiro.contas_disponiveis.v1",
                &ContasDisponiveis,
            ) {
                self.contas = c;
            }
        }
        self.pagar = None;
    }

    fn nome(&self, vendedor: Id) -> String {
        self.usuarios
            .iter()
            .find(|u| u.id == vendedor)
            .map_or_else(|| "Vendedor".to_owned(), |u| u.nome.clone())
    }

    /// Abre o pagamento de um vendedor, sugerindo o caixa e a data de hoje.
    pub(super) fn abrir_pagamento(&mut self, vendedor: Id) {
        self.pagar = Some(vendedor);
        if self.conta.is_none() {
            self.conta = self
                .contas
                .iter()
                .find(|c| c.e_caixa)
                .or_else(|| self.contas.first())
                .map(|c| c.conta);
        }
        self.data = Data::hoje(Fuso::BRASILIA).formatar();
    }
}

/// Apura o mês escolhido.
pub(super) fn apurar(
    motor: &Motor,
    sessao: &Sessao,
    c: &mut EstadoComissoes,
) -> Result<ComissoesApuradas, String> {
    let r = motor
        .executar(
            sessao,
            "vendas.apurar_comissoes.v1",
            &ApurarComissoes {
                competencia: c.mes(),
            },
        )
        .map_err(|e| e.mensagem)?;
    c.carregar(motor, sessao);
    Ok(r)
}

/// Paga o vendedor aberto.
pub(super) fn pagar(
    motor: &Motor,
    sessao: &Sessao,
    c: &mut EstadoComissoes,
) -> Result<ComissoesPagas, String> {
    let vendedor = c.pagar.ok_or_else(|| "Escolha o vendedor".to_owned())?;
    let conta = c
        .conta
        .ok_or_else(|| "Escolha a conta de onde sai o pagamento".to_owned())?;
    let data =
        Data::de_str(c.data.trim()).map_err(|_| "Digite a data como dd/mm/aaaa".to_owned())?;
    let r = motor
        .executar(
            sessao,
            "vendas.pagar_comissoes.v1",
            &PagarComissoes {
                vendedor,
                competencia: c.mes(),
                conta,
                data,
            },
        )
        .map_err(|e| e.mensagem)?;
    c.carregar(motor, sessao);
    Ok(r)
}

pub(super) fn dialogo(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaVendas,
) {
    let fechar = Dialogo::nova("Comissões").largura(860.0).mostrar(
        ctx,
        estado,
        |ui, estado| corpo(ui, motor, sessao, &mut estado.comissoes),
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

fn corpo(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, c: &mut EstadoComissoes) {
    let mes = c.mes();
    let mut recarregar = false;
    ui.horizontal(|ui| {
        ui.add(Rotulo::sobrelinha("Mês"));
        ui.add_space(Espaco::E8);
        if ui.add(Botao::secundario("‹").pequeno()).clicked() {
            c.mes = Some(mes.anterior());
            recarregar = true;
        }
        ui.add(Rotulo::titulo_secao(mes.formatar_extenso()));
        if ui.add(Botao::secundario("›").pequeno()).clicked() {
            c.mes = Some(mes.proxima());
            recarregar = true;
        }
    });
    if recarregar {
        c.carregar(motor, sessao);
    }
    ui.add_space(Espaco::E8);
    if let Some(erro) = &c.erro {
        ui.add(
            Rotulo::interface(erro.clone())
                .quebravel()
                .cor(ui.cores().negativo),
        );
        return;
    }

    let soma = |f: fn(&ResumoDoVendedor) -> Dinheiro| {
        c.resumo
            .iter()
            .fold(Dinheiro::ZERO, |total, r| total + f(r))
    };
    let (a_apurar, a_pagar, pago) = (soma(|r| r.a_apurar), soma(|r| r.a_pagar), soma(|r| r.pago));
    FaixaKpi::nova(vec![
        CartaoKpi::novo("A apurar", a_apurar).tom(if a_apurar.e_positivo() {
            Tom::Atencao
        } else {
            Tom::Neutro
        }),
        CartaoKpi::novo("Apurado a pagar", a_pagar),
        CartaoKpi::novo("Pago", pago).tom(Tom::Positivo),
    ])
    .mostrar(ui);
    ui.add_space(Espaco::E8);
    ui.add(
        Rotulo::apoio(
            "A comissão de cada produto (a dele, a do grupo ou a padrão, em Estoque) fica \
             valendo no faturamento do pedido. Apurar lança a despesa no mês da venda; rodar de \
             novo só pega os pedidos faturados depois.",
        )
        .quebravel(),
    );
    ui.add_space(Espaco::E8);
    if a_apurar.e_positivo()
        && ui
            .add(Botao::primario(format!("Apurar {a_apurar} do mês")))
            .clicked()
    {
        match apurar(motor, sessao, c) {
            Ok(r) => notificar(
                ui.ctx(),
                Notificacao::sucesso(format!(
                    "Comissão apurada: {} em {} pedido(s)",
                    r.valor, r.pedidos
                )),
            ),
            Err(e) => notificar(ui.ctx(), Notificacao::erro(e)),
        }
    }
    ui.add_space(Espaco::E8);
    grade(ui, c);
    pagamento(ui, motor, sessao, c);
}

fn grade(ui: &mut egui::Ui, c: &mut EstadoComissoes) {
    let mut abrir = None;
    let nomes: Vec<String> = c.resumo.iter().map(|r| c.nome(r.vendedor)).collect();
    Grade::nova(vec![
        ColunaGrade::nova("Vendedor"),
        ColunaGrade::nova("Pedidos").largura(70.0).numero(),
        ColunaGrade::nova("Vendido").largura(110.0).numero(),
        ColunaGrade::nova("A apurar").largura(100.0).numero(),
        ColunaGrade::nova("A pagar").largura(100.0).numero(),
        ColunaGrade::nova("Pago").largura(100.0).numero(),
        ColunaGrade::nova("").largura(90.0),
    ])
    .id_salt("vendas-comissoes")
    .vazio("Nenhuma venda com comissão neste mês.")
    .mostrar(ui, c.resumo.len(), |i, row| {
        let r = &c.resumo[i];
        row.col(|ui| {
            ui.add(Rotulo::interface(nomes[i].clone()));
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(r.pedidos.to_string()));
        });
        row.col(|ui| {
            ui.add(ValorDinheiro::novo(r.base).neutro());
        });
        row.col(|ui| {
            ui.add(ValorDinheiro::novo(r.a_apurar).neutro());
        });
        row.col(|ui| {
            ui.add(ValorDinheiro::novo(r.a_pagar).neutro());
        });
        row.col(|ui| {
            ui.add(ValorDinheiro::novo(r.pago).neutro());
        });
        row.col(|ui| {
            if r.a_pagar.e_positivo() && ui.add(Botao::secundario("Pagar").pequeno()).clicked() {
                abrir = Some(r.vendedor);
            }
        });
    });
    if let Some(v) = abrir {
        c.abrir_pagamento(v);
    }
}

fn pagamento(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, c: &mut EstadoComissoes) {
    let Some(vendedor) = c.pagar else {
        return;
    };
    let valor = c
        .resumo
        .iter()
        .find(|r| r.vendedor == vendedor)
        .map_or(Dinheiro::ZERO, |r| r.a_pagar);
    let nome = c.nome(vendedor);
    ui.add_space(Espaco::E12);
    Painel::novo().realce(Tom::Atencao).mostrar(ui, |ui| {
        ui.add(Rotulo::titulo_secao(format!("Pagar {valor} a {nome}")));
        ui.add(Rotulo::apoio(
            "Quita tudo o que está apurado para o vendedor neste mês, num lançamento só.",
        ));
        ui.add_space(Espaco::E8);
        if c.contas.is_empty() {
            ui.add(
                Rotulo::interface(
                    "Sem acesso às contas de caixa e banco: peça a permissão de ver contas \
                     bancárias no Financeiro.",
                )
                .quebravel()
                .cor(ui.cores().negativo),
            );
            return;
        }
        let opcoes: Vec<(Id, String)> = c
            .contas
            .iter()
            .map(|k| (k.conta, format!("{} ({})", k.nome, k.codigo)))
            .collect();
        ui.columns(2, |col| {
            SeletorOpcao::novo("Sai de", &mut c.conta)
                .opcoes(opcoes)
                .mostrar(&mut col[0]);
            col[1].add(Campo::novo("Data do pagamento", &mut c.data).mascara(Mascara::Data));
        });
        ui.add_space(Espaco::E8);
        ui.horizontal(|ui| {
            if ui.add(Botao::primario("Pagar")).clicked() {
                match pagar(motor, sessao, c) {
                    Ok(r) => notificar(
                        ui.ctx(),
                        Notificacao::sucesso(format!(
                            "Comissão paga: {} ({} pedido(s))",
                            r.valor, r.pedidos
                        )),
                    ),
                    Err(e) => notificar(ui.ctx(), Notificacao::erro(e)),
                }
            }
            if ui.add(Botao::fantasma("Cancelar")).clicked() {
                c.pagar = None;
            }
        });
    });
}
