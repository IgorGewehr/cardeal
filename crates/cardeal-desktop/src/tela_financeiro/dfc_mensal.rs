//! Resultado › DFC › "Mês a mês" e "Orçado × realizado": o fluxo de caixa do ano por conta do
//! plano (`mod_financeiro::dfc_gerencial`) — os 12 meses lado a lado, o realizado ou o orçado,
//! em reais ou em % das receitas (a análise vertical), e o orçado contra o realizado de janeiro
//! até o mês escolhido.

use super::*;
use cardeal_kernel::Percentual;
use cardeal_ui::tokens::Papel;
use mod_financeiro::dfc_gerencial::{
    atingido, percentual, soma_ate, DfcGerencial, DfcGerencialDoAno, GrupoDfcGerencial,
};

const MESES: [&str; 12] = [
    "Jan", "Fev", "Mar", "Abr", "Mai", "Jun", "Jul", "Ago", "Set", "Out", "Nov", "Dez",
];

/// O DFC do ano e como mostrá-lo.
#[derive(Default)]
pub(super) struct EstadoDfcMensal {
    pub(super) dados: Option<Result<DfcGerencial, String>>,
    /// Mostra o orçado em vez do realizado ("a visão DFC do orçamento").
    pub(super) orcado: bool,
    /// Mostra cada linha em % das receitas do mês.
    pub(super) percentual: bool,
}

impl EstadoTelaFinanceiro {
    pub(super) fn carregar_dfc_mensal(&mut self, motor: &Motor, sessao: &Sessao) {
        let ano = self.mes_dre().ano();
        self.dfc_mensal.dados = Some(
            motor
                .consultar(
                    sessao,
                    "financeiro.dfc_gerencial_do_ano.v1",
                    &DfcGerencialDoAno { ano },
                )
                .map_err(|e| e.mensagem),
        );
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tipo {
    /// Caixa no início/fim: não tem percentual nem soma no total.
    Saldo,
    /// O total de um grupo.
    Grupo,
    /// A agrupadora do plano dentro do grupo.
    Agrupadora,
    /// Uma conta.
    Conta,
    /// Geração operacional, variação do caixa.
    Subtotal,
}

#[derive(Clone, Debug)]
struct Linha {
    rotulo: String,
    tipo: Tipo,
    valores: [Dinheiro; 12],
    total: Dinheiro,
    /// A conta está sob uma agrupadora (recuo maior).
    recuada: bool,
}

impl Linha {
    fn fluxo(rotulo: impl Into<String>, tipo: Tipo, valores: [Dinheiro; 12]) -> Self {
        Self {
            rotulo: rotulo.into(),
            tipo,
            total: valores.iter().copied().sum(),
            valores,
            recuada: false,
        }
    }
}

fn valores_do_grupo(g: &GrupoDfcGerencial, orcado: bool) -> [Dinheiro; 12] {
    if orcado {
        g.orcado
    } else {
        g.realizado
    }
}

/// As linhas da tabela de 12 meses, do caixa no início ao caixa no fim. No orçado, o caixa é o
/// de 1º de janeiro mais o que o orçamento prevê, mês a mês.
fn linhas_mes_a_mes(d: &DfcGerencial, orcado: bool) -> Vec<Linha> {
    let variacao = if orcado {
        d.variacao_orcada()
    } else {
        d.variacao
    };
    let mut inicio = [Dinheiro::ZERO; 12];
    let mut fim = [Dinheiro::ZERO; 12];
    let mut saldo = d.saldo_inicial[0];
    for i in 0..12 {
        inicio[i] = saldo;
        saldo += variacao[i];
        fim[i] = saldo;
    }
    let mut linhas = vec![Linha {
        rotulo: "Caixa no início do mês".to_owned(),
        tipo: Tipo::Saldo,
        valores: inicio,
        total: inicio[0],
        recuada: false,
    }];
    let tem_operacional = d.grupos.iter().any(|g| g.grupo.operacional());
    let mut operacional_posto = false;
    for g in &d.grupos {
        if !g.grupo.operacional() && tem_operacional && !operacional_posto {
            linhas.push(Linha::fluxo(
                "Geração de caixa operacional",
                Tipo::Subtotal,
                if orcado {
                    d.geracao_operacional_orcada()
                } else {
                    d.geracao_operacional()
                },
            ));
            operacional_posto = true;
        }
        let total = valores_do_grupo(g, orcado);
        if total.iter().all(|v| v.e_zero())
            && g.linhas.iter().all(|l| {
                (if orcado { &l.orcado } else { &l.realizado })
                    .iter()
                    .all(|v| v.e_zero())
            })
        {
            continue;
        }
        linhas.push(Linha::fluxo(g.grupo.rotulo(), Tipo::Grupo, total));
        let mut agrupadora_atual: Option<&str> = None;
        for l in &g.linhas {
            let valores = if orcado { l.orcado } else { l.realizado };
            if valores.iter().all(|v| v.e_zero()) {
                continue;
            }
            if l.agrupadora.as_deref() != agrupadora_atual {
                agrupadora_atual = l.agrupadora.as_deref();
                if let Some(nome) = agrupadora_atual {
                    let mut soma = [Dinheiro::ZERO; 12];
                    for x in g
                        .linhas
                        .iter()
                        .filter(|x| x.agrupadora.as_deref() == Some(nome))
                    {
                        let v = if orcado { x.orcado } else { x.realizado };
                        for (s, v) in soma.iter_mut().zip(v) {
                            *s += v;
                        }
                    }
                    linhas.push(Linha::fluxo(nome, Tipo::Agrupadora, soma));
                }
            }
            let mut linha = Linha::fluxo(l.nome.clone(), Tipo::Conta, valores);
            linha.recuada = l.agrupadora.is_some();
            linhas.push(linha);
        }
    }
    if tem_operacional && !operacional_posto {
        linhas.push(Linha::fluxo(
            "Geração de caixa operacional",
            Tipo::Subtotal,
            if orcado {
                d.geracao_operacional_orcada()
            } else {
                d.geracao_operacional()
            },
        ));
    }
    linhas.push(Linha::fluxo("Variação do caixa", Tipo::Subtotal, variacao));
    linhas.push(Linha {
        rotulo: "Caixa no fim do mês".to_owned(),
        tipo: Tipo::Saldo,
        valores: fim,
        total: fim[11],
        recuada: false,
    });
    linhas
}

/// As receitas de cada mês (a base do %), no realizado ou no orçado.
fn receitas(d: &DfcGerencial, orcado: bool) -> [Dinheiro; 12] {
    d.grupos
        .iter()
        .find(|g| g.grupo == mod_financeiro::dfc_gerencial::GrupoGerencial::Receitas)
        .map_or([Dinheiro::ZERO; 12], |g| valores_do_grupo(g, orcado))
}

fn celula_percentual(ui: &mut egui::Ui, p: Option<Percentual>) {
    match p {
        Some(p) if p.unidades_internas() != 0 => {
            ui.add(Rotulo::interface(format!("{}%", p.formatar(1))));
        }
        _ => {
            ui.add(Rotulo::interface("—").cor(ui.cores().texto_fraco));
        }
    }
}

fn celula_rotulo(ui: &mut egui::Ui, l: &Linha) {
    match l.tipo {
        Tipo::Conta => {
            ui.add_space(if l.recuada { Espaco::E32 } else { Espaco::E16 });
            ui.add(Rotulo::interface(l.rotulo.clone()));
        }
        Tipo::Agrupadora => {
            ui.add_space(Espaco::E16);
            ui.add(Rotulo::campo(l.rotulo.clone()).cor(ui.cores().texto_fraco));
        }
        Tipo::Grupo => {
            ui.add(Rotulo::campo(l.rotulo.clone()));
        }
        Tipo::Saldo | Tipo::Subtotal => {
            ui.add(Rotulo::novo(Papel::TituloSecao, l.rotulo.clone()));
        }
    }
}

pub(super) fn painel_mes_a_mes(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) {
    let d = match &estado.dfc_mensal.dados {
        Some(Ok(d)) => d.clone(),
        Some(Err(e)) => {
            ui.add(
                Rotulo::interface(e.clone())
                    .quebravel()
                    .cor(ui.cores().negativo),
            );
            return;
        }
        None => {
            estado.carregar_dfc_mensal(motor, sessao);
            return;
        }
    };
    ui.add(
        Rotulo::apoio(format!(
            "O caixa de {} mês a mês, por conta do plano: cada recebimento e pagamento entra na \
             conta do título de origem (a venda, o aluguel, a compra de mercadoria), não em \
             \"clientes\" ou \"fornecedores\". Pela data em que o dinheiro se moveu.",
            d.ano
        ))
        .quebravel(),
    );
    if !d.sem_origem.e_zero() {
        ui.add_space(Espaco::E4);
        ui.add(Etiqueta::info(format!(
            "{} recebidos ou pagos sem título de origem ficam nas linhas \"sem conta de origem\"",
            d.sem_origem.formatar_com_simbolo()
        )));
    }
    ui.add_space(Espaco::E8);
    ui.horizontal(|ui| {
        if let Some(v) = Abas::nova(&[(false, "Realizado"), (true, "Orçado")])
            .selecionada(estado.dfc_mensal.orcado)
            .id_salt("dfc-mensal-fonte")
            .mostrar(ui)
        {
            estado.dfc_mensal.orcado = v;
        }
        ui.add_space(Espaco::E16);
        if let Some(v) = Abas::nova(&[(false, "Em reais"), (true, "Em % das receitas")])
            .selecionada(estado.dfc_mensal.percentual)
            .id_salt("dfc-mensal-formato")
            .mostrar(ui)
        {
            estado.dfc_mensal.percentual = v;
        }
    });
    ui.add_space(Espaco::E8);
    let orcado = estado.dfc_mensal.orcado;
    if orcado && !d.tem_orcamento() {
        EstadoVazio::novo(
            Icone::Dinheiro,
            format!(
                "Sem orçamento para {}. Orce as contas em Resultado › Orçamento.",
                d.ano
            ),
        )
        .mostrar(ui);
        return;
    }
    let linhas = linhas_mes_a_mes(&d, orcado);
    let base = receitas(&d, orcado);
    let base_total: Dinheiro = base.iter().copied().sum();
    let em_percentual = estado.dfc_mensal.percentual;

    let mut colunas = vec![ColunaGrade::nova("Conta").largura(300.0)];
    colunas.extend(
        MESES
            .iter()
            .map(|m| ColunaGrade::nova(m).largura(100.0).numero()),
    );
    colunas.push(ColunaGrade::nova("Total").largura(120.0).numero());
    Grade::nova(colunas)
        .id_salt("dfc-mensal")
        .rolagem_horizontal()
        .mostrar(ui, linhas.len(), |i, row| {
            let l = &linhas[i];
            row.col(|ui| celula_rotulo(ui, l));
            for (m, v) in l.valores.iter().enumerate() {
                row.col(|ui| {
                    if em_percentual {
                        if l.tipo == Tipo::Saldo {
                            celula_percentual(ui, None);
                        } else {
                            celula_percentual(ui, percentual(*v, base[m]));
                        }
                    } else {
                        super::dre::celula_valor(ui, *v);
                    }
                });
            }
            row.col(|ui| {
                if em_percentual {
                    if l.tipo == Tipo::Saldo {
                        celula_percentual(ui, None);
                    } else {
                        celula_percentual(ui, percentual(l.total, base_total));
                    }
                } else {
                    super::dre::celula_valor(ui, l.total);
                }
            });
        });
}

/// Uma linha do orçado × realizado: o acumulado de janeiro até o mês.
#[derive(Clone, Debug)]
struct LinhaComparada {
    rotulo: String,
    tipo: Tipo,
    orcado: Dinheiro,
    realizado: Dinheiro,
    recuada: bool,
}

/// O orçado × realizado de janeiro até o mês `ate`: os grupos e as contas com algum dos dois, a
/// geração operacional e a variação do caixa.
fn linhas_comparadas(d: &DfcGerencial, ate: u32) -> Vec<LinhaComparada> {
    let mut linhas = Vec::new();
    let tem_operacional = d.grupos.iter().any(|g| g.grupo.operacional());
    let mut operacional_posto = false;
    let operacional = |linhas: &mut Vec<LinhaComparada>| {
        linhas.push(LinhaComparada {
            rotulo: "Geração de caixa operacional".to_owned(),
            tipo: Tipo::Subtotal,
            orcado: soma_ate(&d.geracao_operacional_orcada(), ate),
            realizado: soma_ate(&d.geracao_operacional(), ate),
            recuada: false,
        });
    };
    for g in &d.grupos {
        if !g.grupo.operacional() && tem_operacional && !operacional_posto {
            operacional(&mut linhas);
            operacional_posto = true;
        }
        let (o, r) = (soma_ate(&g.orcado, ate), soma_ate(&g.realizado, ate));
        if o.e_zero() && r.e_zero() {
            continue;
        }
        linhas.push(LinhaComparada {
            rotulo: g.grupo.rotulo().to_owned(),
            tipo: Tipo::Grupo,
            orcado: o,
            realizado: r,
            recuada: false,
        });
        for l in &g.linhas {
            let (o, r) = (l.orcado_ate(ate), l.realizado_ate(ate));
            if o.e_zero() && r.e_zero() {
                continue;
            }
            linhas.push(LinhaComparada {
                rotulo: l.nome.clone(),
                tipo: Tipo::Conta,
                orcado: o,
                realizado: r,
                recuada: false,
            });
        }
    }
    if tem_operacional && !operacional_posto {
        operacional(&mut linhas);
    }
    linhas.push(LinhaComparada {
        rotulo: "Variação do caixa".to_owned(),
        tipo: Tipo::Subtotal,
        orcado: soma_ate(&d.variacao_orcada(), ate),
        realizado: soma_ate(&d.variacao, ate),
        recuada: false,
    });
    linhas
}

pub(super) fn painel_orcado_realizado(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) {
    let d = match &estado.dfc_mensal.dados {
        Some(Ok(d)) => d.clone(),
        Some(Err(e)) => {
            ui.add(
                Rotulo::interface(e.clone())
                    .quebravel()
                    .cor(ui.cores().negativo),
            );
            return;
        }
        None => {
            estado.carregar_dfc_mensal(motor, sessao);
            return;
        }
    };
    let mes = estado.mes_dre();
    if !d.tem_orcamento() {
        EstadoVazio::novo(
            Icone::Dinheiro,
            format!(
                "Sem orçamento para {}. Orce as contas em Resultado › Orçamento para comparar \
                 com o caixa realizado.",
                d.ano
            ),
        )
        .mostrar(ui);
        if sessao.concede("financeiro.orcamento.editar")
            && ui.add(Botao::secundario("Abrir o orçamento")).clicked()
        {
            estado.visao_resultado = 4;
        }
        return;
    }
    ui.add(
        Rotulo::apoio(format!(
            "De janeiro até {} de {}, pelo caixa: o que o orçamento previa entrar e sair e o que \
             de fato entrou e saiu. A diferença positiva é sempre melhor para o caixa (receita \
             acima do previsto, despesa abaixo).",
            MESES[mes.mes() as usize - 1].to_lowercase(),
            d.ano
        ))
        .quebravel(),
    );
    ui.add_space(Espaco::E8);
    let linhas = linhas_comparadas(&d, mes.mes());
    let resultado = linhas.last().map_or((Dinheiro::ZERO, Dinheiro::ZERO), |l| {
        (l.orcado, l.realizado)
    });
    FaixaKpi::nova(vec![
        CartaoKpi::novo("Variação orçada", resultado.0).tom(Tom::Neutro),
        CartaoKpi::novo("Variação realizada", resultado.1).tom(if resultado.1.e_negativo() {
            Tom::Negativo
        } else {
            Tom::Positivo
        }),
        CartaoKpi::novo("Diferença", resultado.1 - resultado.0).tom(
            if (resultado.1 - resultado.0).e_negativo() {
                Tom::Negativo
            } else {
                Tom::Positivo
            },
        ),
    ])
    .mostrar(ui);
    ui.add_space(Espaco::E12);
    Grade::nova(vec![
        ColunaGrade::nova("Conta"),
        ColunaGrade::nova("Orçado").largura(140.0).numero(),
        ColunaGrade::nova("Realizado").largura(140.0).numero(),
        ColunaGrade::nova("Diferença").largura(140.0).numero(),
        ColunaGrade::nova("Atingido").largura(110.0).numero(),
    ])
    .id_salt("dfc-orcado-realizado")
    .mostrar(ui, linhas.len(), |i, row| {
        let l = &linhas[i];
        row.col(|ui| {
            celula_rotulo(
                ui,
                &Linha {
                    rotulo: l.rotulo.clone(),
                    tipo: l.tipo,
                    valores: [Dinheiro::ZERO; 12],
                    total: Dinheiro::ZERO,
                    recuada: l.recuada,
                },
            );
        });
        row.col(|ui| super::dre::celula_valor(ui, l.orcado));
        row.col(|ui| super::dre::celula_valor(ui, l.realizado));
        row.col(|ui| {
            let dif = l.realizado - l.orcado;
            if dif.e_zero() {
                super::dre::celula_valor(ui, dif);
            } else {
                ui.add(ValorDinheiro::novo(dif).com_sinal());
            }
        });
        row.col(|ui| celula_percentual(ui, atingido(l.realizado, l.orcado)));
    });
}

#[cfg(test)]
mod testes {
    use super::*;
    use mod_financeiro::dfc_gerencial::{GrupoGerencial, LinhaGerencial};

    fn meses(pares: &[(usize, i64)]) -> [Dinheiro; 12] {
        let mut m = [Dinheiro::ZERO; 12];
        for (i, v) in pares {
            m[*i] = Dinheiro::reais(*v);
        }
        m
    }

    fn conta(
        nome: &str,
        agrupadora: Option<&str>,
        r: [Dinheiro; 12],
        o: [Dinheiro; 12],
    ) -> LinhaGerencial {
        LinhaGerencial {
            conta: Id::novo(),
            codigo: "x".into(),
            nome: nome.into(),
            agrupadora: agrupadora.map(Into::into),
            realizado: r,
            orcado: o,
        }
    }

    fn grupo(g: GrupoGerencial, linhas: Vec<LinhaGerencial>) -> GrupoDfcGerencial {
        let mut realizado = [Dinheiro::ZERO; 12];
        let mut orcado = [Dinheiro::ZERO; 12];
        for l in &linhas {
            for i in 0..12 {
                realizado[i] += l.realizado[i];
                orcado[i] += l.orcado[i];
            }
        }
        GrupoDfcGerencial {
            grupo: g,
            linhas,
            realizado,
            orcado,
        }
    }

    fn dfc() -> DfcGerencial {
        let grupos = vec![
            grupo(
                GrupoGerencial::Receitas,
                vec![conta(
                    "Vendas",
                    None,
                    meses(&[(0, 1_000), (1, 800)]),
                    meses(&[(0, 900), (1, 900)]),
                )],
            ),
            grupo(
                GrupoGerencial::Despesas,
                vec![
                    conta(
                        "Aluguel",
                        Some("Estrutura"),
                        meses(&[(0, -300), (1, -300)]),
                        meses(&[(0, -300), (1, -300)]),
                    ),
                    conta(
                        "Energia",
                        Some("Estrutura"),
                        meses(&[(0, -100)]),
                        meses(&[]),
                    ),
                ],
            ),
            grupo(
                GrupoGerencial::Financiamentos,
                vec![conta("Empréstimos", None, meses(&[(1, 2_000)]), meses(&[]))],
            ),
        ];
        let mut variacao = [Dinheiro::ZERO; 12];
        for g in &grupos {
            for (v, r) in variacao.iter_mut().zip(g.realizado) {
                *v += r;
            }
        }
        let mut saldo_inicial = [Dinheiro::ZERO; 12];
        let mut saldo_final = [Dinheiro::ZERO; 12];
        let mut s = Dinheiro::reais(50);
        for i in 0..12 {
            saldo_inicial[i] = s;
            s += variacao[i];
            saldo_final[i] = s;
        }
        DfcGerencial {
            ano: 2026,
            saldo_inicial,
            grupos,
            variacao,
            saldo_final,
            sem_origem: Dinheiro::ZERO,
        }
    }

    #[test]
    fn mes_a_mes_tem_a_agrupadora_a_geracao_operacional_e_fecha_no_caixa() {
        let d = dfc();
        let l = linhas_mes_a_mes(&d, false);
        let rotulos: Vec<(&str, Tipo)> = l.iter().map(|l| (l.rotulo.as_str(), l.tipo)).collect();
        assert_eq!(
            rotulos,
            [
                ("Caixa no início do mês", Tipo::Saldo),
                ("Receitas operacionais", Tipo::Grupo),
                ("Vendas", Tipo::Conta),
                ("Despesas operacionais", Tipo::Grupo),
                ("Estrutura", Tipo::Agrupadora),
                ("Aluguel", Tipo::Conta),
                ("Energia", Tipo::Conta),
                ("Geração de caixa operacional", Tipo::Subtotal),
                ("Atividades de financiamento", Tipo::Grupo),
                ("Empréstimos", Tipo::Conta),
                ("Variação do caixa", Tipo::Subtotal),
                ("Caixa no fim do mês", Tipo::Saldo),
            ]
        );
        assert_eq!(l[4].valores[0], Dinheiro::reais(-400));
        assert!(l[5].recuada);
        assert_eq!(l[7].total, Dinheiro::reais(1_100));
        // O caixa: 50 + 600 em janeiro + 2.500 em fevereiro; o total do início é janeiro.
        assert_eq!(l[0].total, Dinheiro::reais(50));
        assert_eq!(l[11].valores[1], Dinheiro::reais(3_150));
        assert_eq!(l[11].total, Dinheiro::reais(3_150));
    }

    #[test]
    fn no_orcado_o_caixa_e_o_previsto_e_a_conta_sem_orcamento_some() {
        let d = dfc();
        let l = linhas_mes_a_mes(&d, true);
        assert!(l
            .iter()
            .all(|l| l.rotulo != "Energia" && l.rotulo != "Empréstimos"));
        assert!(l.iter().all(|l| l.rotulo != "Atividades de financiamento"));
        let fim = l.last().unwrap();
        // 50 + 600 + 600.
        assert_eq!(fim.valores[1], Dinheiro::reais(1_250));
        assert_eq!(receitas(&d, true)[0], Dinheiro::reais(900));
    }

    #[test]
    fn orcado_realizado_acumula_ate_o_mes() {
        let d = dfc();
        let l = linhas_comparadas(&d, 2);
        let vendas = l.iter().find(|l| l.rotulo == "Vendas").unwrap();
        assert_eq!(vendas.orcado, Dinheiro::reais(1_800));
        assert_eq!(vendas.realizado, Dinheiro::reais(1_800));
        let energia = l.iter().find(|l| l.rotulo == "Energia").unwrap();
        assert_eq!(energia.orcado, Dinheiro::ZERO);
        assert_eq!(energia.realizado, Dinheiro::reais(-100));
        let ultima = l.last().unwrap();
        assert_eq!(ultima.rotulo, "Variação do caixa");
        assert_eq!(ultima.orcado, Dinheiro::reais(1_200));
        assert_eq!(ultima.realizado, Dinheiro::reais(3_100));
        // Só janeiro.
        let jan = linhas_comparadas(&d, 1);
        assert_eq!(
            jan.iter().find(|l| l.rotulo == "Vendas").unwrap().realizado,
            Dinheiro::reais(1_000)
        );
    }
}
