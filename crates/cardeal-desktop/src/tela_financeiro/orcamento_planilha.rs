//! Resultado › Orçamento › "Planilha do ano": todas as contas que se orçam × os 12 meses numa
//! grade só, com o subtotal de cada bloco e o total de cada conta, gravada de uma vez
//! ("Salvar orçamento", `financeiro.definir_orcamento_do_ano.v1` — tudo ou nada). É o
//! "planejamento orçamentário" de quem orça o ano numa planilha; a visão do caixa orçado fica em
//! Resultado › DFC › Mês a mês › Orçado.

use std::collections::HashMap;

use super::*;
use cardeal_ui::atoms::CampoTexto;
use cardeal_ui::tokens::AlturaLinha;
use mod_financeiro::orcamento::{DefinirOrcamentoDoAno, LinhaDeOrcamento, OrcamentoAnual};

const MESES: [&str; 12] = [
    "Jan", "Fev", "Mar", "Abr", "Mai", "Jun", "Jul", "Ago", "Set", "Out", "Nov", "Dez",
];

/// O que está digitado na planilha de um ano.
pub(super) struct Planilha {
    ano: i32,
    /// Os 12 meses de cada conta, em texto, como o usuário digitou.
    valores: HashMap<Id, [String; 12]>,
    erro: Option<String>,
}

impl Planilha {
    fn de(o: &OrcamentoAnual) -> Self {
        Self {
            ano: o.ano,
            valores: o
                .linhas
                .iter()
                .map(|l| {
                    (
                        l.conta,
                        l.orcado.map(|v| {
                            if v.e_zero() {
                                String::new()
                            } else {
                                v.formatar()
                            }
                        }),
                    )
                })
                .collect(),
            erro: None,
        }
    }
}

/// O valor de uma célula: vazio é zero; `None` se não é um valor.
fn ler(texto: &str) -> Option<Dinheiro> {
    let t = texto.trim();
    if t.is_empty() {
        Some(Dinheiro::ZERO)
    } else {
        Dinheiro::de_str(t).ok()
    }
}

/// As contas cujo orçado mudou, já conferidas: um valor que não se lê, ou negativo numa conta do
/// resultado, é o erro (com a conta e o mês).
fn alteradas(
    o: &OrcamentoAnual,
    valores: &HashMap<Id, [String; 12]>,
) -> Result<Vec<(Id, [Dinheiro; 12])>, String> {
    let mut saida = Vec::new();
    for l in &o.linhas {
        let Some(textos) = valores.get(&l.conta) else {
            continue;
        };
        let mut meses = [Dinheiro::ZERO; 12];
        for (i, t) in textos.iter().enumerate() {
            let v = ler(t).ok_or_else(|| {
                format!("{}, {}: \"{}\" não é um valor", l.nome, MESES[i], t.trim())
            })?;
            if v.e_negativo() && !l.de_caixa {
                return Err(format!(
                    "{}, {}: receita e despesa se orçam positivas",
                    l.nome, MESES[i]
                ));
            }
            meses[i] = v;
        }
        if meses != l.orcado {
            saida.push((l.conta, meses));
        }
    }
    Ok(saida)
}

/// Uma linha da planilha: o cabeçalho de um bloco (com o subtotal) ou uma conta.
enum LinhaPlanilha<'a> {
    Bloco(String, Vec<Id>),
    Conta(&'a LinhaDeOrcamento),
}

/// As linhas: cada bloco (na ordem em que aparece no plano) seguido das contas dele.
fn linhas(o: &OrcamentoAnual) -> Vec<LinhaPlanilha<'_>> {
    let mut blocos: Vec<(&str, Vec<&LinhaDeOrcamento>)> = Vec::new();
    for l in &o.linhas {
        match blocos.iter_mut().find(|(b, _)| *b == l.bloco) {
            Some((_, ls)) => ls.push(l),
            None => blocos.push((l.bloco.as_str(), vec![l])),
        }
    }
    let mut saida = Vec::new();
    for (bloco, ls) in blocos {
        saida.push(LinhaPlanilha::Bloco(
            bloco.to_owned(),
            ls.iter().map(|l| l.conta).collect(),
        ));
        saida.extend(ls.into_iter().map(LinhaPlanilha::Conta));
    }
    saida
}

fn celula_soma(ui: &mut egui::Ui, v: Option<Dinheiro>) {
    match v {
        Some(v) => super::dre::celula_valor(ui, v),
        None => {
            ui.add(Rotulo::interface("?").cor(ui.cores().negativo));
        }
    }
}

pub(super) fn painel(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
    o: &OrcamentoAnual,
) {
    if estado
        .form_orcamento
        .planilha_do_ano
        .as_ref()
        .is_none_or(|p| p.ano != o.ano)
    {
        estado.form_orcamento.planilha_do_ano = Some(Planilha::de(o));
    }
    let mut salvar = false;
    let mut descartar = false;
    let mut ver_caixa = false;
    let alteracoes = estado
        .form_orcamento
        .planilha_do_ano
        .as_ref()
        .map(|p| alteradas(o, &p.valores));
    let ha_alteracao = matches!(&alteracoes, Some(Ok(a)) if !a.is_empty());
    ui.horizontal(|ui| {
        salvar = ui
            .add(Botao::primario("Salvar orçamento").habilitado(ha_alteracao))
            .clicked();
        descartar = ui
            .add(Botao::secundario("Descartar alterações").habilitado(ha_alteracao))
            .clicked();
        ver_caixa = ui
            .add(Botao::fantasma("Ver o caixa orçado, mês a mês"))
            .clicked();
    });
    ui.add(
        Rotulo::apoio(
            "Receitas e despesas se orçam positivas, pela competência (como na DRE). Compras, \
             investimentos e financiamentos se orçam pelo caixa: no empréstimo, o que entra é \
             positivo e a parcela paga, negativa.",
        )
        .quebravel(),
    );
    if let Some(Err(e)) = &alteracoes {
        ui.add(Rotulo::interface(e.clone()).cor(ui.cores().negativo));
    }
    if let Some(e) = estado
        .form_orcamento
        .planilha_do_ano
        .as_ref()
        .and_then(|p| p.erro.clone())
    {
        ui.add(Rotulo::interface(e).cor(ui.cores().negativo));
    }
    ui.add_space(Espaco::E8);

    if ver_caixa {
        estado.visao_resultado = 1;
        estado.dfc_visao = 2;
        estado.dfc_mensal.orcado = true;
        return;
    }
    if descartar {
        estado.form_orcamento.planilha_do_ano = Some(Planilha::de(o));
        return;
    }
    if salvar {
        if let Some(Ok(contas)) = alteracoes {
            let r: Result<u32, _> = motor.executar(
                sessao,
                "financeiro.definir_orcamento_do_ano.v1",
                &DefinirOrcamentoDoAno { ano: o.ano, contas },
            );
            match r {
                Ok(n) => {
                    notificar(
                        ui.ctx(),
                        Notificacao::sucesso(format!(
                            "Orçamento de {} salvo ({n} conta(s))",
                            o.ano
                        )),
                    );
                    estado.form_orcamento.planilha_do_ano = None;
                    estado.carregar_dre(motor, sessao);
                }
                Err(e) => {
                    if let Some(p) = estado.form_orcamento.planilha_do_ano.as_mut() {
                        p.erro = Some(e.mensagem);
                    }
                }
            }
            return;
        }
    }

    let Some(planilha) = estado.form_orcamento.planilha_do_ano.as_mut() else {
        return;
    };
    let linhas = linhas(o);
    let mut colunas = vec![ColunaGrade::nova("Conta").largura(280.0)];
    colunas.extend(
        MESES
            .iter()
            .map(|m| ColunaGrade::nova(m).largura(110.0).numero()),
    );
    colunas.push(ColunaGrade::nova("Total").largura(130.0).numero());
    let valores = &mut planilha.valores;
    Grade::nova(colunas)
        .id_salt("orcamento-planilha")
        .rolagem_horizontal()
        .altura_linha(AlturaLinha::Toque)
        .mostrar(ui, linhas.len(), |i, row| match &linhas[i] {
            LinhaPlanilha::Bloco(nome, contas) => {
                row.col(|ui| {
                    ui.add(Rotulo::campo(nome.clone()));
                });
                let soma_mes = |m: usize| -> Option<Dinheiro> {
                    contas
                        .iter()
                        .filter_map(|c| valores.get(c))
                        .map(|t| ler(&t[m]))
                        .sum()
                };
                let mut total = Some(Dinheiro::ZERO);
                for m in 0..12 {
                    let v = soma_mes(m);
                    total = total.zip(v).map(|(a, b)| a + b);
                    row.col(|ui| celula_soma(ui, v));
                }
                row.col(|ui| celula_soma(ui, total));
            }
            LinhaPlanilha::Conta(l) => {
                row.col(|ui| {
                    ui.add_space(Espaco::E16);
                    ui.add(Rotulo::interface(format!("{} {}", l.codigo, l.nome)));
                });
                let textos = valores.entry(l.conta).or_default();
                for t in textos.iter_mut() {
                    row.col(|ui| {
                        ui.add(CampoTexto::novo(t).numerico().compacto().marcador("0,00"));
                    });
                }
                let total: Option<Dinheiro> = textos.iter().map(|t| ler(t)).sum();
                row.col(|ui| celula_soma(ui, total));
            }
        });
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::testes_comum::motor_de_teste;

    #[test]
    fn a_planilha_desenha_grava_o_que_mudou_e_recarrega() {
        let t = motor_de_teste();
        let mut estado = EstadoTelaFinanceiro {
            aba: Aba::Resultado,
            ..Default::default()
        };
        estado.carregar(&t.motor, &t.sessao);
        estado.visao_resultado = 4;
        estado.form_orcamento.planilha = true;
        let ctx = egui::Context::default();
        cardeal_ui::tokens::instalar_fontes(&ctx);
        cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
        let desenhar = |estado: &mut EstadoTelaFinanceiro| {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    super::super::dre::painel_dre(ui, &t.motor, &t.sessao, estado);
                });
            });
        };
        desenhar(&mut estado);
        let o = estado.orcamento.clone().unwrap().unwrap();
        // A planilha abriu com todas as contas: as do resultado e as de caixa.
        let p = estado
            .form_orcamento
            .planilha_do_ano
            .as_mut()
            .expect("abriu");
        assert_eq!(p.valores.len(), o.linhas.len());
        assert!(o
            .linhas
            .iter()
            .any(|l| l.de_caixa && l.bloco == "Financiamentos"));
        // Digita o aluguel o ano todo e a parcela do empréstimo; grava só o que mudou.
        let aluguel = o.linhas.iter().find(|l| l.codigo == "5.6").unwrap().conta;
        let emprestimo = o
            .linhas
            .iter()
            .find(|l| l.codigo == "2.1.04")
            .unwrap()
            .conta;
        p.valores
            .insert(aluguel, std::array::from_fn(|_| "1.500".to_owned()));
        p.valores.get_mut(&emprestimo).unwrap()[5] = "-800,00".into();
        let contas = alteradas(&o, &p.valores).unwrap();
        assert_eq!(contas.len(), 2);
        desenhar(&mut estado);
        let n: u32 = t
            .motor
            .executar(
                &t.sessao,
                "financeiro.definir_orcamento_do_ano.v1",
                &DefinirOrcamentoDoAno { ano: o.ano, contas },
            )
            .unwrap();
        assert_eq!(n, 2);
        estado.form_orcamento.planilha_do_ano = None;
        estado.carregar_dre(&t.motor, &t.sessao);
        desenhar(&mut estado);
        let o = estado.orcamento.clone().unwrap().unwrap();
        let a = o.linhas.iter().find(|l| l.conta == aluguel).unwrap();
        assert_eq!(a.orcado_no_ano(), Dinheiro::reais(18_000));
        let p = estado.form_orcamento.planilha_do_ano.as_ref().unwrap();
        assert_eq!(p.valores[&emprestimo][5], "-800,00");
        assert!(alteradas(&o, &p.valores).unwrap().is_empty());
        // O caixa orçado mostra o empréstimo saindo em junho.
        let d = estado.dfc_mensal.dados.clone().unwrap().unwrap();
        assert_eq!(d.variacao_orcada()[5], Dinheiro::reais(-2_300));
    }

    fn linha(nome: &str, bloco: &str, de_caixa: bool, orcado: [Dinheiro; 12]) -> LinhaDeOrcamento {
        LinhaDeOrcamento {
            conta: Id::novo(),
            codigo: "x".into(),
            nome: nome.into(),
            bloco: bloco.into(),
            soma_no_resultado: false,
            de_caixa,
            orcado,
            realizado: [Dinheiro::ZERO; 12],
        }
    }

    #[test]
    fn so_o_que_mudou_vai_e_negativo_so_na_conta_de_caixa() {
        let o = OrcamentoAnual {
            ano: 2026,
            linhas: vec![
                linha("Aluguel", "Despesas", false, [Dinheiro::reais(100); 12]),
                linha("Energia", "Despesas", false, [Dinheiro::ZERO; 12]),
                linha("Empréstimos", "Financiamentos", true, [Dinheiro::ZERO; 12]),
            ],
        };
        let mut p = Planilha::de(&o);
        assert_eq!(p.valores[&o.linhas[0].conta][0], "100,00");
        assert!(alteradas(&o, &p.valores).unwrap().is_empty());
        // Muda a energia e o empréstimo (com a parcela negativa).
        p.valores.get_mut(&o.linhas[1].conta).unwrap()[2] = "1.234,50".into();
        p.valores.get_mut(&o.linhas[2].conta).unwrap()[0] = "20000".into();
        p.valores.get_mut(&o.linhas[2].conta).unwrap()[1] = "-2.000".into();
        let a = alteradas(&o, &p.valores).unwrap();
        assert_eq!(a.len(), 2);
        assert_eq!(a[0].1[2], Dinheiro::centavos(123_450));
        assert_eq!(a[1].1[1], Dinheiro::reais(-2_000));
        // Negativo na despesa e texto que não é valor: erro com a conta e o mês.
        p.valores.get_mut(&o.linhas[0].conta).unwrap()[3] = "-5".into();
        assert_eq!(
            alteradas(&o, &p.valores).unwrap_err(),
            "Aluguel, Abr: receita e despesa se orçam positivas"
        );
        p.valores.get_mut(&o.linhas[0].conta).unwrap()[3] = "abc".into();
        assert!(alteradas(&o, &p.valores).unwrap_err().contains("Abr"));
        // Os blocos na ordem em que aparecem, cada um antes das contas dele.
        let l = linhas(&o);
        assert!(matches!(&l[0], LinhaPlanilha::Bloco(b, c) if b == "Despesas" && c.len() == 2));
        assert!(matches!(&l[3], LinhaPlanilha::Bloco(b, _) if b == "Financiamentos"));
        assert_eq!(l.len(), 5);
    }
}
