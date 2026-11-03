//! Resultado › "Simples Nacional": o faturamento do mês e dos 12 meses anteriores (o RBT12 da
//! lei), a alíquota efetiva, o DAS estimado, a faixa e os avisos de limite — e o que o Cardeal
//! não sabe sozinho: o mês de abertura e o faturamento de antes do Cardeal (`mod_estoque::simples`).

use super::*;
use mod_estoque::simples::{
    DefinirHistoricoDoSimples, HistoricoDoSimples, HistoricoDoSimplesDaEmpresa, OrigemDoRbt12,
    RegimeDoResumo, ResumoDoSimples, ResumoDoSimplesDoMes,
};

/// Quem vê o resumo (mostra o faturamento da empresa).
pub(super) const PERMISSAO_VER: &str = "estoque.preco.simples.ver";
/// Quem corrige o histórico.
const PERMISSAO_EDITAR: &str = "estoque.tributario.editar";

/// O formulário do histórico: o mês de abertura e o faturamento de antes do Cardeal dos 12
/// meses anteriores ao mês escolhido (os outros meses informados ficam como estão).
#[derive(Default)]
pub(super) struct FormHistoricoSimples {
    inicio: String,
    valores: Vec<(Competencia, String)>,
    outros: Vec<(Competencia, Dinheiro)>,
    erro: Option<String>,
}

fn mes_texto(c: Competencia) -> String {
    format!("{:02}/{}", c.mes(), c.ano())
}

/// `MM/AAAA` (ou `M/AAAA`) em competência.
fn competencia_de(texto: &str) -> Option<Competencia> {
    let (m, a) = texto.trim().split_once('/')?;
    let (mes, ano) = (m.trim().parse::<u32>().ok()?, a.trim().parse::<i32>().ok()?);
    ((1..=12).contains(&mes) && (2000..=2100).contains(&ano)).then(|| Competencia::nova(ano, mes))
}

impl EstadoTelaFinanceiro {
    pub(super) fn carregar_simples(&mut self, motor: &Motor, sessao: &Sessao) {
        if !sessao.concede(PERMISSAO_VER) {
            self.simples = None;
            return;
        }
        let mes = self.mes_dre();
        self.simples = Some(
            motor
                .consultar(
                    sessao,
                    "estoque.resumo_do_simples.v1",
                    &ResumoDoSimplesDoMes { mes },
                )
                .map_err(|e| e.mensagem),
        );
        let historico: HistoricoDoSimples = motor
            .consultar(
                sessao,
                "estoque.historico_do_simples.v1",
                &HistoricoDoSimplesDaEmpresa,
            )
            .unwrap_or_default();
        let mut valores = Vec::with_capacity(12);
        let mut c = mes;
        for _ in 0..12 {
            c = c.anterior();
            let v = historico
                .receita_anterior
                .iter()
                .find(|(m, _)| *m == c)
                .map(|(_, v)| v.formatar())
                .unwrap_or_default();
            valores.push((c, v));
        }
        valores.reverse();
        let janela: Vec<Competencia> = valores.iter().map(|(c, _)| *c).collect();
        self.form_simples = FormHistoricoSimples {
            inicio: historico
                .inicio_atividade
                .map(mes_texto)
                .unwrap_or_default(),
            valores,
            outros: historico
                .receita_anterior
                .into_iter()
                .filter(|(c, _)| !janela.contains(c))
                .collect(),
            erro: None,
        };
    }
}

fn texto_do_regime(r: &ResumoDoSimples) -> String {
    match r.regime {
        RegimeDoResumo::Simples(anexo) => format!("Simples Nacional — {}", anexo.nome()),
        RegimeDoResumo::Mei => "MEI — o DAS é fixo; o que importa é o limite do ano".into(),
        RegimeDoResumo::ForaDoSimples => {
            "Lucro presumido ou real: o resumo do Simples não se aplica".into()
        }
    }
}

fn texto_da_origem(o: OrigemDoRbt12) -> String {
    match o {
        OrigemDoRbt12::DozeMeses => "os 12 meses anteriores".into(),
        OrigemDoRbt12::PrimeiroMes => "primeiro mês de atividade: o faturamento do mês × 12".into(),
        OrigemDoRbt12::Media { meses } => {
            format!("início de atividade: a média dos {meses} mês(es) anteriores × 12")
        }
        OrigemDoRbt12::HistoricoCurto { meses } => {
            format!("só {meses} mês(es) de histórico — veja o aviso")
        }
    }
}

pub(super) fn painel_simples(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) {
    let r = match &estado.simples {
        Some(Ok(r)) => r.clone(),
        Some(Err(e)) => {
            ui.add(
                Rotulo::interface(e.clone())
                    .quebravel()
                    .cor(ui.cores().negativo),
            );
            return;
        }
        None => {
            EstadoVazio::novo(Icone::Dinheiro, "Sem acesso ao resumo do Simples.").mostrar(ui);
            return;
        }
    };
    ui.add(Rotulo::titulo_secao(texto_do_regime(&r)));
    if r.regime == RegimeDoResumo::ForaDoSimples {
        return;
    }
    ui.add_space(Espaco::E8);
    let mut cartoes = vec![CartaoKpi::novo("Faturamento do mês", r.mes.receita).tom(Tom::Neutro)];
    match r.regime {
        RegimeDoResumo::Mei => {
            cartoes.push(CartaoKpi::novo("Faturamento no ano", r.receita_no_ano).tom(Tom::Neutro));
        }
        _ => {
            cartoes.push(
                CartaoKpi::novo("Faturamento de 12 meses", r.mes.rbt12)
                    .tom(Tom::Neutro)
                    .variacao(texto_da_origem(r.mes.origem)),
            );
            if let Some(das) = r.mes.das {
                cartoes.push(
                    CartaoKpi::novo("DAS estimado", das)
                        .tom(Tom::Atencao)
                        .variacao(format!(
                            "alíquota efetiva {}%",
                            r.mes.aliquota.map(|a| a.formatar(2)).unwrap_or_default()
                        )),
                );
            }
        }
    }
    FaixaKpi::nova(cartoes).mostrar(ui);
    ui.add_space(Espaco::E8);
    if let (Some(faixa), Some(teto)) = (r.faixa, r.teto_da_faixa) {
        let mut t = format!("Faixa {faixa} de 6, até {}.", teto.formatar_com_simbolo());
        if let Some((inicio, aliquota)) = r.proxima_faixa {
            t.push_str(&format!(
                " A próxima começa em {}, com alíquota efetiva de {}%.",
                inicio.formatar_com_simbolo(),
                aliquota.formatar(2)
            ));
        }
        ui.add(Rotulo::apoio(t));
    }
    for a in &r.avisos {
        ui.add(
            Rotulo::interface(a.texto())
                .quebravel()
                .cor(ui.cores().atencao),
        );
    }
    ui.add(Rotulo::apoio(
        "Estimativa: a alíquota efetiva sobre o faturamento do mês, pela lei (os 12 meses \
         anteriores, a parcela a deduzir). Não separa as vendas com ST ou monofásico, que pagam \
         menos — a guia é apurada pelo contador.",
    ));
    if r.regime != RegimeDoResumo::Mei {
        ui.add_space(Espaco::E12);
        Grade::nova(vec![
            ColunaGrade::nova("Mês").largura(110.0),
            ColunaGrade::nova("Faturamento").numero(),
            ColunaGrade::nova("12 meses anteriores").numero(),
            ColunaGrade::nova("Alíquota").largura(100.0).numero(),
            ColunaGrade::nova("DAS estimado").numero(),
        ])
        .id_salt("simples-meses")
        .mostrar(ui, r.meses.len(), |i, row| {
            let m = &r.meses[i];
            row.col(|ui| {
                ui.add(Rotulo::interface(mes_texto(m.competencia)));
            });
            row.col(|ui| {
                ui.add(ValorDinheiro::novo(m.receita).neutro());
            });
            row.col(|ui| {
                ui.add(ValorDinheiro::novo(m.rbt12).neutro());
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(
                    m.aliquota
                        .map(|a| format!("{}%", a.formatar(2)))
                        .unwrap_or_default(),
                ));
            });
            row.col(|ui| {
                ui.add(ValorDinheiro::novo(m.das.unwrap_or(Dinheiro::ZERO)).neutro());
            });
        });
    }
    if sessao.concede(PERMISSAO_EDITAR) {
        ui.add_space(Espaco::E12);
        historico(ui, motor, sessao, estado, !r.avisos.is_empty());
    }
}

/// O formulário do mês de abertura e do faturamento de antes do Cardeal.
fn historico(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
    aberto: bool,
) {
    let mut salvar = false;
    SecaoExpansivel::nova("Mês de abertura e faturamento de antes do Cardeal")
        .aberta_por_padrao(aberto)
        .mostrar(ui, |ui| {
            let f = &mut estado.form_simples;
            ui.add(
                Rotulo::apoio(
                    "O Cardeal só conhece o faturamento desde que começou a ser usado. Se a \
                     empresa é mais antiga, informe o faturamento (receita bruta) dos meses de \
                     antes; se abriu há menos de um ano, o mês de abertura — a receita do \
                     primeiro ano é anualizada, como a lei manda.",
                )
                .quebravel(),
            );
            ui.add(
                Campo::novo(
                    "Mês de abertura (MM/AAAA), se há menos de um ano",
                    &mut f.inicio,
                )
                .marcador("03/2026"),
            );
            ui.add_space(Espaco::E8);
            for (c, v) in &mut f.valores {
                ui.add(
                    Campo::novo(
                        format!("Faturamento de {} antes do Cardeal", mes_texto(*c)),
                        v,
                    )
                    .marcador("0,00"),
                );
            }
            if let Some(e) = &f.erro {
                ui.add(Rotulo::interface(e.clone()).cor(ui.cores().negativo));
            }
            if ui.add(Botao::primario("Salvar")).clicked() {
                salvar = true;
            }
        });
    if !salvar {
        return;
    }
    let f = &estado.form_simples;
    let inicio = if f.inicio.trim().is_empty() {
        None
    } else if let Some(c) = competencia_de(&f.inicio) {
        Some(c)
    } else {
        estado.form_simples.erro = Some("o mês de abertura deve ser MM/AAAA".into());
        return;
    };
    let mut receita_anterior = f.outros.clone();
    for (c, v) in &f.valores {
        if v.trim().is_empty() {
            continue;
        }
        match Dinheiro::de_str(v) {
            Ok(d) if !d.e_zero() => receita_anterior.push((*c, d)),
            Ok(_) => {}
            Err(_) => {
                estado.form_simples.erro =
                    Some(format!("o faturamento de {} não é um valor", mes_texto(*c)));
                return;
            }
        }
    }
    receita_anterior.sort_by_key(|(c, _)| *c);
    let r: Result<(), _> = motor.executar(
        sessao,
        "estoque.definir_historico_do_simples.v1",
        &DefinirHistoricoDoSimples {
            historico: HistoricoDoSimples {
                inicio_atividade: inicio,
                receita_anterior,
            },
        },
    );
    match r {
        Ok(()) => {
            notificar(ui.ctx(), Notificacao::sucesso("Histórico do Simples salvo"));
            estado.carregar_simples(motor, sessao);
        }
        Err(e) => estado.form_simples.erro = Some(e.mensagem),
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_mes_de_abertura_e_lido_como_mm_aaaa() {
        assert_eq!(competencia_de("03/2026"), Some(Competencia::nova(2026, 3)));
        assert_eq!(
            competencia_de(" 3 / 2026 "),
            Some(Competencia::nova(2026, 3))
        );
        assert_eq!(competencia_de("13/2026"), None);
        assert_eq!(competencia_de("2026-03"), None);
    }
}
