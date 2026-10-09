//! A aba "Resultado" (DRE gerencial): o mês escolhido ao lado do anterior e do mesmo mês do
//! ano passado, direto do Razão — Receita bruta até Lucro líquido, com a variação.

use super::*;
use cardeal_kernel::Percentual;
use cardeal_ui::tokens::Papel;
use mod_financeiro::dre::{Dre, LinhaDre, TipoLinhaDre};
use mod_financeiro::{ComparativoDre, DreComparativa};

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
