//! A conferência do saldo bancário: na aba Conciliação, o saldo que o banco informa × o do
//! sistema, com a diferença explicada pelo que está só de um lado (e o saldo digitado, para
//! quem importa CSV); na Visão geral, o alerta da conciliação parada
//! (`mod_financeiro::ConferenciaDasContas`).

use super::*;
use mod_financeiro::{
    ConferenciaDaConta, ConferenciaDasContas, FechamentoDoSaldo, InformarSaldoDoBanco,
    OrigemDoSaldo, Soma,
};

/// Quantos dias uma linha pendente ou um extrato sem importar aguentam até virar alerta.
pub(super) const TOLERANCIA_DIAS: i32 = 7;

impl EstadoConciliacao {
    pub(super) fn carregar_conferencias(&mut self, motor: &Motor, sessao: &Sessao) {
        self.conferencias = motor
            .consultar(
                sessao,
                "financeiro.conferencia_das_contas.v1",
                &ConferenciaDasContas,
            )
            .unwrap_or_default();
    }

    /// A conferência da conta escolhida.
    pub(super) fn conferencia(&self) -> Option<&ConferenciaDaConta> {
        let conta = self.conta?;
        self.conferencias.iter().find(|c| c.conta == conta)
    }

    /// Os alertas de todas as contas, prefixados pelo nome da conta.
    pub(super) fn alertas(&self, hoje: Data) -> Vec<String> {
        self.conferencias
            .iter()
            .flat_map(|c| {
                c.alertas(hoje, TOLERANCIA_DIAS)
                    .into_iter()
                    .map(move |a| format!("{}: {a}", c.nome))
            })
            .collect()
    }
}

/// Grava o saldo digitado no formulário.
pub(super) fn informar_saldo(
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) -> Result<(), String> {
    let c = &estado.conciliacao;
    let conta = c
        .conta
        .ok_or_else(|| "Escolha a conta bancária".to_owned())?;
    let (data, valor) = c
        .saldo_form
        .as_ref()
        .ok_or_else(|| "Abra o formulário do saldo".to_owned())?;
    let data = Data::de_str(data.trim()).map_err(|_| "Digite a data como dd/mm/aaaa".to_owned())?;
    let saldo = Dinheiro::de_str(valor.trim())
        .map_err(|_| "Digite o saldo, como 1.234,56 (negativo com o sinal de menos)".to_owned())?;
    let (): () = motor
        .executar(
            sessao,
            "financeiro.informar_saldo_do_banco.v1",
            &InformarSaldoDoBanco { conta, data, saldo },
        )
        .map_err(|e| e.mensagem)?;
    estado.conciliacao.saldo_form = None;
    estado.conciliacao.carregar_conferencias(motor, sessao);
    Ok(())
}

/// O painel da conta escolhida, entre o cabeçalho e a lista da aba Conciliação.
pub(super) fn painel(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) {
    let hoje = Data::hoje(Fuso::BRASILIA);
    let conferencia = estado.conciliacao.conferencia().cloned();
    let mut abrir = false;
    Painel::novo().plano().mostrar(ui, |ui| {
        ui.horizontal(|ui| {
            ui.add(Rotulo::titulo_secao("Saldo do banco × sistema"));
            ui.add_space(Espaco::E8);
            if ui
                .add(Botao::fantasma("Informar saldo…").pequeno())
                .clicked()
            {
                abrir = true;
            }
        });
        ui.add_space(Espaco::E4);
        match conferencia.as_ref().and_then(|c| c.fechamento) {
            None => {
                ui.add(
                    Rotulo::apoio(
                        "O banco ainda não informou o saldo desta conta. O OFX traz o saldo; \
                         com CSV, informe o saldo que o aplicativo do banco mostra.",
                    )
                    .quebravel(),
                );
            }
            Some(f) => fechamento(ui, &f),
        }
        if let Some(c) = &conferencia {
            for a in c.alertas(hoje, TOLERANCIA_DIAS) {
                ui.add(Etiqueta::atencao(a));
            }
        }
    });
    if abrir {
        estado.conciliacao.saldo_form = Some((hoje.formatar(), String::new()));
    }
    formulario(ui, motor, sessao, estado);
}

fn fechamento(ui: &mut egui::Ui, f: &FechamentoDoSaldo) {
    ui.add(Rotulo::interface(format!(
        "No banco em {} ({}): {} · No sistema: {} · Diferença: {}",
        f.banco.data.formatar(),
        match f.banco.origem {
            OrigemDoSaldo::Extrato => "do extrato",
            OrigemDoSaldo::Digitado => "digitado",
        },
        f.banco.saldo.formatar_com_simbolo(),
        f.sistema.formatar_com_simbolo(),
        f.diferenca().formatar_com_simbolo()
    )));
    let parte = |ui: &mut egui::Ui, s: Soma, o_que: &str| {
        if s.quantidade > 0 {
            ui.add(Rotulo::apoio(format!(
                "{} em {} {o_que}",
                s.valor.formatar_com_simbolo(),
                s.quantidade
            )));
        }
    };
    parte(
        ui,
        f.pendentes,
        "linha(s) do extrato ainda pendente(s) — estão no banco, não no sistema",
    );
    parte(
        ui,
        f.ignoradas,
        "linha(s) ignorada(s) — estão no banco, fora do sistema",
    );
    parte(
        ui,
        f.so_no_sistema,
        "baixa(s) que o banco ainda não mostrou — estão no sistema, não no banco",
    );
    ui.add_space(Espaco::E4);
    ui.add(if f.fechado() {
        Etiqueta::positiva("Conta fechada: banco e sistema batem")
    } else if f.sem_explicacao().e_zero() {
        Etiqueta::info("Diferença toda explicada — falta conciliar")
    } else {
        Etiqueta::negativa(format!(
            "Sem explicação: {}",
            f.sem_explicacao().formatar_com_simbolo()
        ))
    });
}

fn formulario(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) {
    let Some((data, valor)) = estado.conciliacao.saldo_form.as_mut() else {
        return;
    };
    let (mut salvar, mut cancelar) = (false, false);
    ui.add_space(Espaco::E8);
    Painel::novo().realce(Tom::Atencao).mostrar(ui, |ui| {
        ui.add(Rotulo::apoio(
            "O saldo do fim do dia que o aplicativo ou o extrato do banco mostra.",
        ));
        ui.add_space(Espaco::E8);
        ui.columns(2, |col| {
            col[0].add(Campo::novo("Data", data).mascara(Mascara::Data));
            col[1].add(Campo::novo("Saldo (R$)", valor).marcador("ex.: 1.234,56"));
        });
        ui.add_space(Espaco::E8);
        ui.horizontal(|ui| {
            salvar = ui.add(Botao::primario("Salvar saldo")).clicked();
            cancelar = ui.add(Botao::fantasma("Cancelar")).clicked();
        });
    });
    if cancelar {
        estado.conciliacao.saldo_form = None;
    } else if salvar {
        match informar_saldo(motor, sessao, estado) {
            Ok(()) => notificar(ui.ctx(), Notificacao::sucesso("Saldo do banco informado")),
            Err(e) => notificar(ui.ctx(), Notificacao::erro(e)),
        }
    }
}

/// O alerta da Visão geral: o que está parado na conciliação, com o atalho para a aba.
pub(super) fn alerta_na_visao(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) {
    let alertas = estado.conciliacao.alertas(Data::hoje(Fuso::BRASILIA));
    if alertas.is_empty() {
        return;
    }
    let mut abrir = false;
    Painel::novo().realce(Tom::Atencao).mostrar(ui, |ui| {
        ui.add(Rotulo::titulo_secao("Conciliação bancária"));
        ui.add_space(Espaco::E4);
        for a in &alertas {
            ui.add(Rotulo::interface(a.clone()).quebravel());
        }
        ui.add_space(Espaco::E8);
        abrir = ui
            .add(Botao::secundario("Abrir a conciliação").pequeno())
            .clicked();
    });
    ui.add_space(Espaco::E12);
    if abrir {
        estado.aba = Aba::Conciliacao;
        estado.carregar(motor, sessao);
    }
}
