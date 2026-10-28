//! Visão geral › **Obrigações**: o que ainda vence no mês (e no começo do próximo, quando o mês
//! está acabando), pelo regime da empresa — `fiscal.obrigacoes_do_mes.v1`. As datas são as da
//! regra federal (dias úteis com os feriados nacionais); o ICMS vem sem data, e o rodapé pede
//! para conferir com o contador.

use cardeal_cliente::{Motor, Sessao};
use cardeal_kernel::{Data, Fuso};
use cardeal_ui::atoms::{Etiqueta, Rotulo};
use cardeal_ui::organisms::Painel;
use cardeal_ui::tokens::Espaco;
use eframe::egui;
use mod_fiscal::calendario::{Obrigacao, ObrigacoesDoMes};

/// Quantas obrigações a Visão mostra, no máximo.
const NO_MAXIMO: usize = 6;

/// As obrigações a mostrar a partir de `hoje`: as que ainda vencem neste mês e as sem data; se
/// sobrar pouco, as do mês seguinte. As de quem tem funcionários vão por último.
#[must_use]
pub(super) fn a_mostrar(
    deste: &[Obrigacao],
    do_proximo: &[Obrigacao],
    hoje: Data,
) -> Vec<Obrigacao> {
    let ainda = |o: &&Obrigacao| o.quando.is_none_or(|d| d >= hoje);
    let mut v: Vec<Obrigacao> = deste.iter().filter(ainda).cloned().collect();
    if v.iter().filter(|o| o.quando.is_some()).count() < 3 {
        v.extend(do_proximo.iter().filter(|o| o.quando.is_some()).cloned());
    }
    v.sort_by_key(|o| (o.so_com_funcionarios, o.quando.is_none(), o.quando));
    v.truncate(NO_MAXIMO);
    v
}

/// Carrega as obrigações (este mês e o próximo). Sem `fiscal.ver`, nada.
pub(super) fn carregar(motor: &Motor, sessao: &Sessao) -> Vec<Obrigacao> {
    if !sessao.concede("fiscal.ver") {
        return Vec::new();
    }
    let hoje = Data::hoje(Fuso::BRASILIA);
    let mes = hoje.competencia();
    let ler = |mes| {
        motor
            .consultar(
                sessao,
                "fiscal.obrigacoes_do_mes.v1",
                &ObrigacoesDoMes { mes },
            )
            .unwrap_or_default()
    };
    a_mostrar(&ler(mes), &ler(mes.proxima()), hoje)
}

/// O cartão.
pub(super) fn cartao(ui: &mut egui::Ui, obrigacoes: &[Obrigacao]) {
    if obrigacoes.is_empty() {
        return;
    }
    let hoje = Data::hoje(Fuso::BRASILIA);
    Painel::novo().titulo("Obrigações").mostrar(ui, |ui| {
        for o in obrigacoes {
            ui.horizontal(|ui| {
                match o.quando {
                    Some(d) => {
                        let faltam = hoje.dias_ate(d);
                        let texto = match faltam {
                            0 => format!("{} — hoje", d.formatar_curta()),
                            1 => format!("{} — amanhã", d.formatar_curta()),
                            n => format!("{} — em {n} dias", d.formatar_curta()),
                        };
                        ui.add(if faltam <= 3 {
                            Etiqueta::atencao(texto)
                        } else {
                            Etiqueta::neutra(texto)
                        });
                    }
                    None => {
                        ui.add(Etiqueta::neutra("prazo do estado"));
                    }
                }
                let nome = if o.so_com_funcionarios {
                    format!("{} ({}) — com funcionários", o.nome, o.referencia)
                } else {
                    format!("{} ({})", o.nome, o.referencia)
                };
                ui.add(Rotulo::interface(nome));
            });
            ui.add(Rotulo::apoio(o.descricao.clone()).quebravel());
            ui.add_space(Espaco::E4);
        }
        ui.add(
            Rotulo::apoio(
                "Datas da regra federal, com os feriados nacionais — feriado do estado ou do \
                 município e o prazo do ICMS: confira com o contador.",
            )
            .quebravel(),
        );
    });
    ui.add_space(Espaco::E12);
}

#[cfg(test)]
mod testes {
    use cardeal_kernel::Competencia;
    use mod_fiscal::calendario::obrigacoes_do_mes;

    use super::*;

    #[test]
    fn mostra_o_que_ainda_vence_e_completa_com_o_proximo_mes() {
        let deste = obrigacoes_do_mes("SimplesNacional", Competencia::nova(2026, 10));
        let do_proximo = obrigacoes_do_mes("SimplesNacional", Competencia::nova(2026, 11));
        // No dia 25 o DAS de outubro (dia 20) já passou: entram os de novembro.
        let v = a_mostrar(&deste, &do_proximo, Data::de_ymd(2026, 10, 25).unwrap());
        assert!(v.iter().all(|o| o
            .quando
            .is_none_or(|d| d >= Data::de_ymd(2026, 10, 25).unwrap())));
        assert!(v
            .iter()
            .any(|o| o.nome == "DAS e PGDAS-D" && o.referencia == "10/2026"));
        // As de quem tem funcionários vão por último.
        let primeira_de_funcionario = v.iter().position(|o| o.so_com_funcionarios);
        let ultima_sem = v.iter().rposition(|o| !o.so_com_funcionarios);
        if let (Some(f), Some(s)) = (primeira_de_funcionario, ultima_sem) {
            assert!(s < f);
        }
        assert!(v.len() <= NO_MAXIMO);
    }
}
