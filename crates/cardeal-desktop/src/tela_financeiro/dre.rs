//! A aba "Resultado" (DRE gerencial): o mês escolhido ao lado do anterior e do mesmo mês do
//! ano passado, direto do Razão — Receita bruta até Lucro líquido, com a variação.

use super::*;
use cardeal_kernel::Percentual;
use cardeal_ui::tokens::Papel;
use mod_financeiro::dfc::Dfc;
use mod_financeiro::dre::{Dre, LinhaDre, TipoLinhaDre};
use mod_financeiro::{ComparativoDre, DfcDoPeriodo, DreComparativa};

impl EstadoTelaFinanceiro {
    /// O mês da DRE: o escolhido ou, na primeira vez, o atual.
    pub(super) fn mes_dre(&self) -> Competencia {
        self.dre_mes
            .unwrap_or_else(|| Data::hoje(Fuso::BRASILIA).competencia())
    }

    pub(super) fn carregar_dre(&mut self, motor: &Motor, sessao: &Sessao) {
        let r = motor.consultar(
            sessao,
            "financeiro.dre_comparativa.v1",
            &DreComparativa {
                mes: self.mes_dre(),
            },
        );
        self.dre = self.anotar(r);
        let mes = self.mes_dre();
        let r = motor.consultar(
            sessao,
            "financeiro.dfc_do_periodo.v1",
            &DfcDoPeriodo {
                periodo: Periodo::novo(mes.primeiro_dia(), mes.ultimo_dia()),
            },
        );
        self.dfc = self.anotar(r);
        let r = motor.consultar(
            sessao,
            "financeiro.dfc_indireto_do_periodo.v1",
            &mod_financeiro::DfcIndiretoDoPeriodo {
                periodo: Periodo::novo(mes.primeiro_dia(), mes.ultimo_dia()),
            },
        );
        self.dfc_indireto = self.anotar(r);
    }
}

/// O valor da mesma linha (rótulo de bloco/resultado ou código de conta) noutra DRE.
fn valor_equivalente(outra: &Dre, linha: &LinhaDre) -> Dinheiro {
    outra
        .linhas
        .iter()
        .find(|l| match (&linha.codigo, &l.codigo) {
            (Some(a), Some(b)) => a == b,
            (None, None) => l.rotulo == linha.rotulo,
            _ => false,
        })
        .map_or(Dinheiro::ZERO, |l| l.valor)
}

/// A variação de `atual` sobre `base` em %, como texto; `None` quando a base é zero.
/// O valor é uma contribuição com sinal, então variação positiva é sempre melhora.
fn variacao(atual: Dinheiro, base: Dinheiro) -> Option<(String, bool)> {
    if base.e_zero() {
        return None;
    }
    let p = Percentual::da_razao((atual - base).em_centavos(), base.em_centavos().abs());
    let sinal = if p.unidades_internas() >= 0 { "+" } else { "" };
    Some((
        format!("{sinal}{}%", p.formatar(1)),
        p.unidades_internas() >= 0,
    ))
}

fn celula_valor(ui: &mut egui::Ui, v: Dinheiro) {
    if v.e_zero() {
        ui.add(Rotulo::interface("—").cor(ui.cores().texto_fraco));
    } else {
        ui.add(ValorDinheiro::novo(v).neutro());
    }
}

fn nome_mes_cheio(c: Competencia) -> String {
    nome_mes(c)
}

pub(super) fn painel_dre(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) {
    let mes = estado.mes_dre();
    let mut recarregar = false;
    ui.horizontal(|ui| {
        ui.add(Rotulo::sobrelinha("Mês"));
        ui.add_space(Espaco::E8);
        if ui.add(Botao::secundario("‹").pequeno()).clicked() {
            estado.dre_mes = Some(mes.anterior());
            recarregar = true;
        }
        ui.add(Rotulo::titulo_secao(nome_mes_cheio(mes)));
        if ui.add(Botao::secundario("›").pequeno()).clicked() {
            estado.dre_mes = Some(mes.proxima());
            recarregar = true;
        }
        if estado.dre_mes.is_some()
            && mes != Data::hoje(Fuso::BRASILIA).competencia()
            && ui.add(Botao::fantasma("Mês atual").pequeno()).clicked()
        {
            estado.dre_mes = None;
            recarregar = true;
        }
    });
    if recarregar {
        estado.carregar_dre(motor, sessao);
    }
    ui.add_space(Espaco::E12);

    ui.add_space(Espaco::E8);
    if let Some(v) = Abas::nova(&[(false, "DRE · resultado"), (true, "DFC · fluxo de caixa")])
        .selecionada(estado.dre_dfc)
        .id_salt("dre-visao")
        .mostrar(ui)
    {
        estado.dre_dfc = v;
    }
    ui.add_space(Espaco::E12);
    if estado.dre_dfc {
        if let Some(v) = Abas::nova(&[(false, "Método direto"), (true, "Método indireto")])
            .selecionada(estado.dfc_metodo_indireto)
            .id_salt("dfc-metodo")
            .mostrar(ui)
        {
            estado.dfc_metodo_indireto = v;
        }
        ui.add_space(Espaco::E8);
        match (
            estado.dfc_metodo_indireto,
            &estado.dfc,
            &estado.dfc_indireto,
        ) {
            (false, Some(dfc), _) => tabela_dfc(ui, dfc),
            (true, _, Some(ind)) => tabela_dfc_indireto(ui, ind),
            _ => {
                EstadoVazio::novo(Icone::Dinheiro, "Sem dados para apurar o fluxo de caixa.")
                    .mostrar(ui);
            }
        }
        return;
    }
    let Some(cmp) = estado.dre.clone() else {
        EstadoVazio::novo(Icone::Dinheiro, "Sem dados para apurar o resultado.").mostrar(ui);
        return;
    };
    kpis(ui, &cmp);
    ui.add_space(Espaco::E16);
    ui.add(Rotulo::titulo_secao("Demonstração do resultado"));
    ui.add(Rotulo::apoio(format!(
        "{} comparado a {} (mês anterior) e {} (ano anterior). Lido do Razão pela competência \
         de cada lançamento; receitas somam, custos e despesas subtraem, e variação positiva \
         é sempre melhor.",
        nome_mes(mes),
        nome_mes(mes.anterior()),
        nome_mes(Competencia::nova(mes.ano() - 1, mes.mes())),
    )));
    ui.add_space(Espaco::E8);
    tabela(ui, &cmp);
}

/// O DFC do mês pelo método direto.
fn tabela_dfc(ui: &mut egui::Ui, dfc: &Dfc) {
    ui.add(Rotulo::apoio(
        "Só o que realmente entrou e saiu do caixa e dos bancos no mês (pela data do pagamento \
         ou recebimento), aberto pela contrapartida. Transferências entre contas não aparecem.",
    ));
    ui.add_space(Espaco::E8);
    tabela_fluxo(
        ui,
        "dfc",
        dfc.saldo_inicial,
        &dfc.blocos,
        dfc.variacao,
        dfc.saldo_final,
    );
}

/// O DFC do mês pelo método indireto: do lucro líquido ao caixa.
fn tabela_dfc_indireto(ui: &mut egui::Ui, d: &mod_financeiro::dfc::DfcIndireto) {
    ui.add(
        Rotulo::apoio(
            "Do lucro líquido da DRE ao caixa: o que cresceu nos ativos (a receber, estoque) \
             prendeu dinheiro, o que cresceu nos passivos (fornecedores, impostos) o soltou, e a \
             depreciação volta porque não sai do caixa. Termina na mesma variação do método \
             direto.",
        )
        .quebravel(),
    );
    if !d.diferenca_de_datas.e_zero() {
        ui.add_space(Espaco::E4);
        ui.add(Etiqueta::info(format!(
            "{} de diferença de datas: lançamento com competência num mês e pagamento noutro",
            d.diferenca_de_datas.formatar_com_simbolo()
        )));
    }
    ui.add_space(Espaco::E8);
    tabela_fluxo(
        ui,
        "dfc-indireto",
        d.saldo_inicial,
        &d.blocos,
        d.variacao,
        d.saldo_final,
    );
}

/// Saldo inicial, as três atividades com suas linhas, variação e saldo final.
fn tabela_fluxo(
    ui: &mut egui::Ui,
    id: &'static str,
    saldo_inicial: Dinheiro,
    blocos: &[mod_financeiro::dfc::BlocoDfc],
    variacao: Dinheiro,
    saldo_final: Dinheiro,
) {
    FaixaKpi::nova(vec![
        CartaoKpi::novo("Caixa no início", saldo_inicial).tom(Tom::Neutro),
        CartaoKpi::novo("Variação no mês", variacao).tom(if variacao.e_negativo() {
            Tom::Negativo
        } else {
            Tom::Positivo
        }),
        CartaoKpi::novo("Caixa no fim", saldo_final).tom(Tom::Neutro),
    ])
    .mostrar(ui);
    ui.add_space(Espaco::E16);

    // Linhas planas: (rotulo, valor, tipo).
    let mut linhas: Vec<(String, Dinheiro, TipoLinhaDre)> = vec![(
        "Caixa no início do mês".to_owned(),
        saldo_inicial,
        TipoLinhaDre::Resultado,
    )];
    for b in blocos {
        linhas.push((b.nome.clone(), b.total, TipoLinhaDre::Bloco));
        for l in &b.linhas {
            linhas.push((l.rotulo.clone(), l.valor, TipoLinhaDre::Conta));
        }
    }
    linhas.push((
        "Variação do caixa".to_owned(),
        variacao,
        TipoLinhaDre::Resultado,
    ));
    linhas.push((
        "Caixa no fim do mês".to_owned(),
        saldo_final,
        TipoLinhaDre::Resultado,
    ));
    Grade::nova(vec![
        ColunaGrade::nova("").largura(420.0),
        ColunaGrade::nova("Valor").largura(180.0).numero(),
    ])
    .id_salt(id)
    .mostrar(ui, linhas.len(), |i, row| {
        let (rotulo, valor, tipo) = &linhas[i];
        row.col(|ui| match tipo {
            TipoLinhaDre::Conta => {
                ui.add_space(Espaco::E16);
                ui.add(Rotulo::interface(rotulo.clone()));
            }
            TipoLinhaDre::Bloco => {
                ui.add(Rotulo::campo(rotulo.clone()));
            }
            TipoLinhaDre::Resultado => {
                ui.add(Rotulo::novo(Papel::TituloSecao, rotulo.clone()));
            }
        });
        row.col(|ui| celula_valor(ui, *valor));
    });
}

fn kpis(ui: &mut egui::Ui, c: &ComparativoDre) {
    let a = &c.atual;
    let tom_lucro = if a.lucro_liquido.e_negativo() {
        Tom::Negativo
    } else {
        Tom::Positivo
    };
    let margem = |m: Option<Percentual>| {
        m.map_or_else(
            || "sem receita".to_owned(),
            |p| format!("{}%", p.formatar(1)),
        )
    };
    FaixaKpi::nova(vec![
        CartaoKpi::novo("Receita líquida", a.receita_liquida).tom(Tom::Positivo),
        CartaoKpi::novo("Lucro bruto", a.lucro_bruto)
            .tom(Tom::Neutro)
            .variacao(format!("margem bruta {}", margem(a.margem_bruta()))),
        CartaoKpi::novo("EBITDA", a.ebitda).tom(Tom::Neutro),
        CartaoKpi::novo("Lucro líquido", a.lucro_liquido)
            .tom(tom_lucro)
            .variacao(format!("margem líquida {}", margem(a.margem_liquida()))),
    ])
    .mostrar(ui);
}

fn tabela(ui: &mut egui::Ui, c: &ComparativoDre) {
    let linhas = &c.atual.linhas;
    if linhas.iter().all(|l| l.valor.e_zero()) {
        EstadoVazio::novo(
            Icone::Dinheiro,
            "Nenhum lançamento de receita ou despesa neste mês.",
        )
        .mostrar(ui);
        return;
    }
    Grade::nova(vec![
        ColunaGrade::nova("").largura(340.0),
        ColunaGrade::nova("Mês").largura(160.0).numero(),
        ColunaGrade::nova("Mês anterior").largura(160.0).numero(),
        ColunaGrade::nova("Variação").largura(120.0).numero(),
        ColunaGrade::nova("Ano anterior").largura(160.0).numero(),
    ])
    .id_salt("dre")
    .mostrar(ui, linhas.len(), |i, row| {
        let l = &linhas[i];
        let anterior = valor_equivalente(&c.mes_anterior, l);
        let ano_antes = valor_equivalente(&c.mesmo_mes_ano_anterior, l);
        row.col(|ui| match l.tipo {
            TipoLinhaDre::Conta => {
                ui.add_space(Espaco::E16);
                ui.add(Rotulo::interface(l.rotulo.clone()));
            }
            TipoLinhaDre::Bloco => {
                ui.add(Rotulo::campo(l.rotulo.clone()));
            }
            TipoLinhaDre::Resultado => {
                ui.add(Rotulo::novo(Papel::TituloSecao, l.rotulo.clone()));
            }
        });
        row.col(|ui| celula_valor(ui, l.valor));
        row.col(|ui| celula_valor(ui, anterior));
        row.col(|ui| match variacao(l.valor, anterior) {
            Some((texto, melhorou)) => {
                let cores = ui.cores();
                ui.add(Rotulo::interface(texto).cor(if melhorou {
                    cores.positivo
                } else {
                    cores.negativo
                }));
            }
            None => {
                ui.add(Rotulo::interface("—").cor(ui.cores().texto_fraco));
            }
        });
        row.col(|ui| celula_valor(ui, ano_antes));
    });
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn variacao_e_sobre_a_base_em_modulo_e_positiva_quando_melhora() {
        let (t, ok) = variacao(Dinheiro::reais(120), Dinheiro::reais(100)).unwrap();
        assert_eq!(t, "+20,0%");
        assert!(ok);
        // Prejuízo que diminui: de −100 para −50 melhorou 50%.
        let (t, ok) = variacao(Dinheiro::reais(-50), Dinheiro::reais(-100)).unwrap();
        assert_eq!(t, "+50,0%");
        assert!(ok);
        let (t, ok) = variacao(Dinheiro::reais(80), Dinheiro::reais(100)).unwrap();
        assert_eq!(t, "-20,0%");
        assert!(!ok);
        assert!(variacao(Dinheiro::reais(5), Dinheiro::ZERO).is_none());
    }
}
