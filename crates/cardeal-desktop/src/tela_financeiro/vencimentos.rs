//! Visão geral › "Vencimentos": esta semana, este mês e o próximo — do que vence em cada período,
//! o previsto, o realizado e o pendente, a receber e a pagar, com o saldo — e o que está em
//! atraso (`mod_financeiro::vencimentos`).

use super::*;
use cardeal_ui::atoms::Divisor;
use mod_financeiro::vencimentos::{
    QuadroDoPeriodo, QuadrosDeVencimento, VencimentosAPagar, VencimentosAReceber,
};

/// Os quadros a receber e a pagar (cada um só com a permissão de vê-lo).
#[derive(Default)]
pub(super) struct EstadoVencimentos {
    pub(super) receber: Option<QuadrosDeVencimento>,
    pub(super) pagar: Option<QuadrosDeVencimento>,
}

/// Largura a partir da qual os quatro quadros ficam numa linha só.
const LIMIAR_QUATRO_COLUNAS: f32 = 1100.0;

impl EstadoTelaFinanceiro {
    pub(super) fn carregar_vencimentos(&mut self, motor: &Motor, sessao: &Sessao) {
        let hoje = Data::hoje(Fuso::BRASILIA);
        self.vencimentos.receber = if sessao.concede("financeiro.receber.ver") {
            let r = motor.consultar(
                sessao,
                "financeiro.vencimentos_a_receber.v1",
                &VencimentosAReceber { hoje },
            );
            self.anotar(r)
        } else {
            None
        };
        self.vencimentos.pagar = if sessao.concede("financeiro.pagar.ver") {
            let r = motor.consultar(
                sessao,
                "financeiro.vencimentos_a_pagar.v1",
                &VencimentosAPagar { hoje },
            );
            self.anotar(r)
        } else {
            None
        };
    }
}

/// "Rótulo ........ valor" numa linha do quadro.
fn linha(ui: &mut egui::Ui, rotulo: &str, valor: Dinheiro, saldo: bool) {
    ui.horizontal(|ui| {
        if saldo {
            ui.add(Rotulo::campo(rotulo));
        } else {
            ui.add(Rotulo::interface(rotulo));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let v = ValorDinheiro::novo(valor);
            ui.add(if saldo { v.destaque() } else { v });
        });
    });
}

/// Uma seção do quadro: a receber, a pagar (negativo) e o saldo — só o que a pessoa pode ver.
fn secao(
    ui: &mut egui::Ui,
    titulo: &str,
    receber: Option<(&str, Dinheiro)>,
    pagar: Option<(&str, Dinheiro)>,
    rotulo_saldo: &str,
) {
    ui.add(Rotulo::sobrelinha(titulo));
    ui.add_space(Espaco::E4);
    if let Some((r, v)) = receber {
        linha(ui, r, v, false);
    }
    if let Some((r, v)) = pagar {
        linha(ui, r, -v, false);
    }
    if let (Some((_, a)), Some((_, b))) = (receber, pagar) {
        linha(ui, rotulo_saldo, a - b, true);
    }
}

fn quadro_do_periodo(
    ui: &mut egui::Ui,
    titulo: &str,
    receber: Option<&QuadroDoPeriodo>,
    pagar: Option<&QuadroDoPeriodo>,
) {
    let Some(base) = receber.or(pagar) else {
        return;
    };
    let periodo = format!("{} a {}", base.de.formatar(), base.ate.formatar());
    Painel::novo().plano().titulo(titulo).mostrar(ui, |ui| {
        ui.add(Rotulo::apoio(periodo));
        ui.add_space(Espaco::E8);
        secao(
            ui,
            "Previsto",
            receber.map(|q| ("A receber", q.previsto)),
            pagar.map(|q| ("A pagar", q.previsto)),
            "Saldo previsto",
        );
        ui.add_space(Espaco::E4);
        ui.add(Divisor::novo());
        secao(
            ui,
            "Realizado",
            receber.map(|q| ("Recebido", q.realizado)),
            pagar.map(|q| ("Pago", q.realizado)),
            "Saldo realizado",
        );
        ui.add_space(Espaco::E4);
        ui.add(Divisor::novo());
        secao(
            ui,
            "Pendente",
            receber.map(|q| ("A receber", q.pendente)),
            pagar.map(|q| ("A pagar", q.pendente)),
            "Saldo a realizar",
        );
    });
}

fn quadro_de_atraso(
    ui: &mut egui::Ui,
    receber: Option<&QuadrosDeVencimento>,
    pagar: Option<&QuadrosDeVencimento>,
) {
    let parcelas =
        receber.map_or(0, |q| q.atraso.parcelas) + pagar.map_or(0, |q| q.atraso.parcelas);
    let mais_antigo = [receber, pagar]
        .into_iter()
        .flatten()
        .filter_map(|q| q.atraso.mais_antigo)
        .min();
    let painel = if parcelas > 0 {
        Painel::novo().realce(Tom::Negativo)
    } else {
        Painel::novo().plano()
    };
    painel.titulo("Em atraso").mostrar(ui, |ui| {
        ui.add(Rotulo::apoio(match mais_antigo {
            Some(d) => format!(
                "{parcelas} parcela(s) · a mais antiga venceu em {}",
                d.formatar()
            ),
            None => "Nada vencido em aberto.".to_owned(),
        }));
        ui.add_space(Espaco::E8);
        secao(
            ui,
            "Vencido e em aberto",
            receber.map(|q| ("A receber", q.atraso.saldo)),
            pagar.map(|q| ("A pagar", q.atraso.saldo)),
            "Saldo do atraso",
        );
    });
}

/// Os quatro quadros: semana, mês, próximo mês e atraso.
pub(super) fn painel(ui: &mut egui::Ui, estado: &EstadoTelaFinanceiro) {
    let (r, p) = (
        estado.vencimentos.receber.as_ref(),
        estado.vencimentos.pagar.as_ref(),
    );
    if r.is_none() && p.is_none() {
        return;
    }
    let semana = |ui: &mut egui::Ui| {
        quadro_do_periodo(
            ui,
            "Esta semana",
            r.map(|q| &q.semana),
            p.map(|q| &q.semana),
        );
    };
    let mes = |ui: &mut egui::Ui| {
        quadro_do_periodo(ui, "Este mês", r.map(|q| &q.mes), p.map(|q| &q.mes));
    };
    let proximo = |ui: &mut egui::Ui| {
        quadro_do_periodo(
            ui,
            "Próximo mês",
            r.map(|q| &q.proximo_mes),
            p.map(|q| &q.proximo_mes),
        );
    };
    let atraso = |ui: &mut egui::Ui| quadro_de_atraso(ui, r, p);
    if ui.available_width() >= LIMIAR_QUATRO_COLUNAS {
        ui.columns(4, |c| {
            semana(&mut c[0]);
            mes(&mut c[1]);
            proximo(&mut c[2]);
            atraso(&mut c[3]);
        });
    } else if ui.available_width() >= LIMIAR_COLUNAS_VISAO_VENC {
        ui.columns(2, |c| {
            semana(&mut c[0]);
            mes(&mut c[1]);
        });
        ui.add_space(Espaco::E12);
        ui.columns(2, |c| {
            proximo(&mut c[0]);
            atraso(&mut c[1]);
        });
    } else {
        semana(ui);
        ui.add_space(Espaco::E12);
        mes(ui);
        ui.add_space(Espaco::E12);
        proximo(ui);
        ui.add_space(Espaco::E12);
        atraso(ui);
    }
    ui.add_space(Espaco::E24);
}

/// Abaixo disso, um quadro por linha.
const LIMIAR_COLUNAS_VISAO_VENC: f32 = 560.0;
