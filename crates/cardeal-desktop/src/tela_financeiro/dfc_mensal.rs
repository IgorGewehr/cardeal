//! Resultado › DFC › "Mês a mês" e "Orçado × realizado": o fluxo de caixa do ano por conta do
//! plano (`mod_financeiro::dfc_gerencial`) — os 12 meses lado a lado, o realizado, o a vencer, os
//! dois somados ou o orçado, em reais ou em % das receitas (a análise vertical), com os subtotais
//! que o dono do negócio lê (receita líquida, margem de contribuição, resultado operacional,
//! resultado líquido do caixa); e o orçado contra o realizado, acumulado até o mês ou mês a mês.
//! As duas visões exportam em PDF e Excel.

use super::exportar::{self, Coluna, Linha as LinhaExport, Relatorio, Tabela, Valor};
use super::*;
use cardeal_kernel::Percentual;
use cardeal_ui::tokens::Papel;
use mod_financeiro::dfc_gerencial::{
    atingido, percentual, soma_ate, DfcGerencial, DfcGerencialDoAno, GrupoGerencial,
};

const MESES: [&str; 12] = [
    "Jan", "Fev", "Mar", "Abr", "Mai", "Jun", "Jul", "Ago", "Set", "Out", "Nov", "Dez",
];
const ORC: [&str; 12] = [
    "Orç jan", "Orç fev", "Orç mar", "Orç abr", "Orç mai", "Orç jun", "Orç jul", "Orç ago",
    "Orç set", "Orç out", "Orç nov", "Orç dez",
];
const REAL: [&str; 12] = [
    "Real jan", "Real fev", "Real mar", "Real abr", "Real mai", "Real jun", "Real jul", "Real ago",
    "Real set", "Real out", "Real nov", "Real dez",
];

/// O que a tabela de 12 meses mostra.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum Fonte {
    /// O que entrou e saiu.
    #[default]
    Realizado,
    /// O que está em aberto para entrar e sair, pelo vencimento.
    AVencer,
    /// Os dois: o caixa projetado.
    Projetado,
    /// O orçamento ("a visão DFC do orçamento").
    Orcado,
}

/// O DFC do ano e como mostrá-lo.
#[derive(Default)]
pub(super) struct EstadoDfcMensal {
    pub(super) dados: Option<Result<DfcGerencial, String>>,
    pub(super) fonte: Fonte,
    /// Mostra cada linha em % das receitas do mês.
    pub(super) percentual: bool,
    /// O orçado × realizado com os 12 meses lado a lado (senão, o acumulado até o mês).
    pub(super) comparado_mes_a_mes: bool,
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
    /// Receita líquida, margem, resultado operacional, resultado líquido.
    Subtotal,
}

/// Uma linha com tudo: o realizado, o a vencer e o orçado de cada mês.
#[derive(Clone, Debug)]
struct LinhaBase {
    rotulo: String,
    tipo: Tipo,
    recuada: bool,
    realizado: [Dinheiro; 12],
    a_vencer: [Dinheiro; 12],
    orcado: [Dinheiro; 12],
}

impl LinhaBase {
    fn valores(&self, fonte: Fonte) -> [Dinheiro; 12] {
        match fonte {
            Fonte::Realizado => self.realizado,
            Fonte::AVencer => self.a_vencer,
            Fonte::Orcado => self.orcado,
            Fonte::Projetado => {
                let mut v = self.realizado;
                for (a, b) in v.iter_mut().zip(self.a_vencer) {
                    *a += b;
                }
                v
            }
        }
    }

    fn vazia(&self, fonte: Fonte) -> bool {
        self.valores(fonte).iter().all(|v| v.e_zero())
    }
}

fn somar(a: &mut [Dinheiro; 12], b: [Dinheiro; 12]) {
    for (x, y) in a.iter_mut().zip(b) {
        *x += y;
    }
}

/// Os grupos, as agrupadoras e as contas, com os subtotais no lugar: receita líquida depois das
/// deduções, margem de contribuição depois dos custos, resultado operacional depois das despesas
/// e, no fim, o resultado líquido do caixa (a variação). Sem as linhas de saldo.
fn linhas_base(d: &DfcGerencial) -> Vec<LinhaBase> {
    let mut linhas = Vec::new();
    let tem_operacional = d.grupos.iter().any(|g| g.grupo.operacional());
    let zero = [Dinheiro::ZERO; 12];
    let mut acum = (zero, zero, zero);
    let mut total = (zero, zero, zero);
    let subtotal = |rotulo: &str, s: &([Dinheiro; 12], [Dinheiro; 12], [Dinheiro; 12])| LinhaBase {
        rotulo: rotulo.to_owned(),
        tipo: Tipo::Subtotal,
        recuada: false,
        realizado: s.0,
        a_vencer: s.1,
        orcado: s.2,
    };
    for g in GrupoGerencial::TODOS {
        if let Some(grupo) = d.grupos.iter().find(|x| x.grupo == g) {
            linhas.push(LinhaBase {
                rotulo: g.rotulo().to_owned(),
                tipo: Tipo::Grupo,
                recuada: false,
                realizado: grupo.realizado,
                a_vencer: grupo.a_vencer,
                orcado: grupo.orcado,
            });
            let mut agrupadora_atual: Option<&str> = None;
            for l in &grupo.linhas {
                if l.agrupadora.as_deref() != agrupadora_atual {
                    agrupadora_atual = l.agrupadora.as_deref();
                    if let Some(nome) = agrupadora_atual {
                        let mut s = (zero, zero, zero);
                        for x in grupo
                            .linhas
                            .iter()
                            .filter(|x| x.agrupadora.as_deref() == Some(nome))
                        {
                            somar(&mut s.0, x.realizado);
                            somar(&mut s.1, x.a_vencer);
                            somar(&mut s.2, x.orcado);
                        }
                        linhas.push(LinhaBase {
                            rotulo: nome.to_owned(),
                            tipo: Tipo::Agrupadora,
                            recuada: false,
                            realizado: s.0,
                            a_vencer: s.1,
                            orcado: s.2,
                        });
                    }
                }
                linhas.push(LinhaBase {
                    rotulo: l.nome.clone(),
                    tipo: Tipo::Conta,
                    recuada: l.agrupadora.is_some(),
                    realizado: l.realizado,
                    a_vencer: l.a_vencer,
                    orcado: l.orcado,
                });
            }
            if g.operacional() {
                somar(&mut acum.0, grupo.realizado);
                somar(&mut acum.1, grupo.a_vencer);
                somar(&mut acum.2, grupo.orcado);
            }
            somar(&mut total.0, grupo.realizado);
            somar(&mut total.1, grupo.a_vencer);
            somar(&mut total.2, grupo.orcado);
        }
        if tem_operacional {
            match g {
                GrupoGerencial::Deducoes => linhas.push(subtotal("(=) Receita líquida", &acum)),
                GrupoGerencial::Custos => {
                    linhas.push(subtotal("(=) Margem de contribuição", &acum));
                }
                GrupoGerencial::Despesas => {
                    linhas.push(subtotal("(=) Resultado operacional", &acum));
                }
                _ => {}
            }
        }
    }
    linhas.push(subtotal("(=) Resultado líquido do caixa", &total));
    linhas
}

/// Uma linha da tabela de 12 meses de uma fonte.
#[derive(Clone, Debug)]
struct Linha {
    rotulo: String,
    tipo: Tipo,
    valores: [Dinheiro; 12],
    total: Dinheiro,
    recuada: bool,
}

/// As linhas da tabela de 12 meses de uma fonte, do caixa no início ao caixa no fim. O caixa do
/// orçado e do projetado parte do de 1º de janeiro; o do a vencer, também (é o que entraria e
/// sairia em cima dele).
fn linhas_mes_a_mes(d: &DfcGerencial, fonte: Fonte) -> Vec<Linha> {
    let base = linhas_base(d);
    let variacao = base
        .last()
        .map_or([Dinheiro::ZERO; 12], |l| l.valores(fonte));
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
    for l in base {
        if l.tipo != Tipo::Subtotal && l.vazia(fonte) {
            continue;
        }
        let valores = l.valores(fonte);
        linhas.push(Linha {
            total: valores.iter().copied().sum(),
            rotulo: l.rotulo,
            tipo: l.tipo,
            valores,
            recuada: l.recuada,
        });
    }
    linhas.push(Linha {
        rotulo: "Caixa no fim do mês".to_owned(),
        tipo: Tipo::Saldo,
        valores: fim,
        total: fim[11],
        recuada: false,
    });
    linhas
}

/// As receitas de cada mês (a base do %), na fonte.
fn receitas(d: &DfcGerencial, fonte: Fonte) -> [Dinheiro; 12] {
    linhas_base(d)
        .iter()
        .find(|l| l.tipo == Tipo::Grupo && l.rotulo == GrupoGerencial::Receitas.rotulo())
        .map_or([Dinheiro::ZERO; 12], |l| l.valores(fonte))
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

fn celula_rotulo(ui: &mut egui::Ui, rotulo: &str, tipo: Tipo, recuada: bool) {
    match tipo {
        Tipo::Conta => {
            ui.add_space(if recuada { Espaco::E32 } else { Espaco::E16 });
            ui.add(Rotulo::interface(rotulo.to_owned()));
        }
        Tipo::Agrupadora => {
            ui.add_space(Espaco::E16);
            ui.add(Rotulo::campo(rotulo.to_owned()).cor(ui.cores().texto_fraco));
        }
        Tipo::Grupo => {
            ui.add(Rotulo::campo(rotulo.to_owned()));
        }
        Tipo::Saldo | Tipo::Subtotal => {
            ui.add(Rotulo::novo(Papel::TituloSecao, rotulo.to_owned()));
        }
    }
}

const fn recuo_de(tipo: Tipo, recuada: bool) -> u8 {
    match tipo {
        Tipo::Conta if recuada => 2,
        Tipo::Conta | Tipo::Agrupadora => 1,
        _ => 0,
    }
}

const fn destaque_de(tipo: Tipo) -> bool {
    !matches!(tipo, Tipo::Conta)
}

fn dados(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) -> Option<DfcGerencial> {
    match &estado.dfc_mensal.dados {
        Some(Ok(d)) => Some(d.clone()),
        Some(Err(e)) => {
            ui.add(
                Rotulo::interface(e.clone())
                    .quebravel()
                    .cor(ui.cores().negativo),
            );
            None
        }
        None => {
            estado.carregar_dfc_mensal(motor, sessao);
            None
        }
    }
}

const fn nome_da_fonte(f: Fonte) -> &'static str {
    match f {
        Fonte::Realizado => "Realizado",
        Fonte::AVencer => "A vencer",
        Fonte::Projetado => "Realizado + a vencer",
        Fonte::Orcado => "Orçado",
    }
}

pub(super) fn painel_mes_a_mes(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) {
    let Some(d) = dados(ui, motor, sessao, estado) else {
        return;
    };
    let fonte = estado.dfc_mensal.fonte;
    let explicacao = match fonte {
        Fonte::Realizado => {
            "O que entrou e saiu, pela data do dinheiro: cada recebimento e pagamento na conta do \
             título de origem (a venda, o aluguel, a compra de mercadoria), não em \"clientes\" \
             ou \"fornecedores\"."
        }
        Fonte::AVencer => {
            "O que está em aberto para entrar e sair, pelo vencimento e na conta do título; o \
             vencido e ainda em aberto conta no mês de hoje."
        }
        Fonte::Projetado => {
            "O realizado até hoje mais o que está a vencer: o caixa do ano se tudo for recebido e \
             pago no vencimento."
        }
        Fonte::Orcado => "O orçamento do ano, no sentido do caixa: o que se prevê entrar e sair.",
    };
    ui.add(
        Rotulo::apoio(format!(
            "{} de {}. {explicacao}",
            nome_da_fonte(fonte),
            d.ano
        ))
        .quebravel(),
    );
    if !d.sem_origem.e_zero() && fonte != Fonte::Orcado {
        ui.add_space(Espaco::E4);
        ui.add(Etiqueta::info(format!(
            "{} recebidos ou pagos sem título de origem ficam nas linhas \"sem conta de origem\"",
            d.sem_origem.formatar_com_simbolo()
        )));
    }
    ui.add_space(Espaco::E8);
    let linhas = linhas_mes_a_mes(&d, fonte);
    let base = receitas(&d, fonte);
    let base_total: Dinheiro = base.iter().copied().sum();
    let em_percentual = estado.dfc_mensal.percentual;
    ui.horizontal_wrapped(|ui| {
        if let Some(v) = Abas::nova(&[
            (Fonte::Realizado, "Realizado"),
            (Fonte::AVencer, "A vencer"),
            (Fonte::Projetado, "Realizado + a vencer"),
            (Fonte::Orcado, "Orçado"),
        ])
        .selecionada(fonte)
        .id_salt("dfc-mensal-fonte")
        .mostrar(ui)
        {
            estado.dfc_mensal.fonte = v;
        }
        ui.add_space(Espaco::E16);
        if let Some(v) = Abas::nova(&[(false, "Em reais"), (true, "Em % das receitas")])
            .selecionada(estado.dfc_mensal.percentual)
            .id_salt("dfc-mensal-formato")
            .mostrar(ui)
        {
            estado.dfc_mensal.percentual = v;
        }
        ui.add_space(Espaco::E16);
        let ano = d.ano;
        let linhas_export = linhas.clone();
        exportar::botoes(ui, motor, || {
            relatorio_mes_a_mes(ano, fonte, &linhas_export)
        });
    });
    ui.add_space(Espaco::E8);
    let vazio = match fonte {
        Fonte::Orcado if !d.tem_orcamento() => Some(format!(
            "Sem orçamento para {}. Orce as contas em Resultado › Orçamento.",
            d.ano
        )),
        Fonte::AVencer if !d.tem_a_vencer() => Some(format!(
            "Nada a vencer em {}: nenhuma conta a receber ou a pagar em aberto.",
            d.ano
        )),
        _ => None,
    };
    if let Some(v) = vazio {
        EstadoVazio::novo(Icone::Dinheiro, v).mostrar(ui);
        return;
    }

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
            row.col(|ui| celula_rotulo(ui, &l.rotulo, l.tipo, l.recuada));
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

fn relatorio_mes_a_mes(ano: i32, fonte: Fonte, linhas: &[Linha]) -> Relatorio {
    let mut colunas = vec![Coluna::texto("Conta", 4.0)];
    colunas.extend(MESES.iter().map(|m| Coluna::numero(m, 1.4)));
    colunas.push(Coluna::numero("Total", 1.6));
    Relatorio {
        titulo: "Fluxo de caixa mês a mês".to_owned(),
        subtitulo: format!("{} de {ano}", nome_da_fonte(fonte)),
        tabelas: vec![Tabela {
            titulo: nome_da_fonte(fonte).to_owned(),
            nota: Some(
                "Por conta do plano: cada recebimento e pagamento na conta do título de origem."
                    .to_owned(),
            ),
            colunas,
            linhas: linhas
                .iter()
                .map(|l| {
                    let mut celulas = vec![Valor::texto(l.rotulo.clone())];
                    celulas.extend(l.valores.iter().map(|v| Valor::Dinheiro(*v)));
                    celulas.push(Valor::Dinheiro(l.total));
                    let linha = LinhaExport::nova(celulas).recuo(recuo_de(l.tipo, l.recuada));
                    if destaque_de(l.tipo) {
                        linha.destaque()
                    } else {
                        linha
                    }
                })
                .collect(),
        }],
    }
}

/// Uma linha do orçado × realizado acumulado até o mês.
#[derive(Clone, Debug)]
struct LinhaComparada {
    rotulo: String,
    tipo: Tipo,
    orcado: Dinheiro,
    realizado: Dinheiro,
    recuada: bool,
}

/// O orçado × realizado de janeiro até o mês `ate`: os grupos e as contas com algum dos dois e os
/// subtotais.
fn linhas_comparadas(d: &DfcGerencial, ate: u32) -> Vec<LinhaComparada> {
    linhas_base(d)
        .into_iter()
        .filter_map(|l| {
            let (o, r) = (soma_ate(&l.orcado, ate), soma_ate(&l.realizado, ate));
            if l.tipo != Tipo::Subtotal && o.e_zero() && r.e_zero() {
                return None;
            }
            Some(LinhaComparada {
                rotulo: l.rotulo,
                tipo: l.tipo,
                orcado: o,
                realizado: r,
                recuada: l.recuada,
            })
        })
        .collect()
}

pub(super) fn painel_orcado_realizado(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) {
    let Some(d) = dados(ui, motor, sessao, estado) else {
        return;
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
    let mes_a_mes = estado.dfc_mensal.comparado_mes_a_mes;
    ui.add(
        Rotulo::apoio(if mes_a_mes {
            format!(
                "{} pelo caixa, mês a mês: o que o orçamento previa entrar e sair ao lado do que \
                 de fato entrou e saiu.",
                d.ano
            )
        } else {
            format!(
                "De janeiro até {}, pelo caixa: o que o orçamento previa entrar e sair e o que de \
                 fato entrou e saiu. A diferença positiva é sempre melhor para o caixa (receita \
                 acima do previsto, despesa abaixo).",
                mes.formatar_extenso()
            )
        })
        .quebravel(),
    );
    ui.add_space(Espaco::E8);
    let linhas = linhas_comparadas(&d, if mes_a_mes { 12 } else { mes.mes() });
    let resultado = linhas.last().map_or((Dinheiro::ZERO, Dinheiro::ZERO), |l| {
        (l.orcado, l.realizado)
    });
    let diferenca = resultado.1 - resultado.0;
    FaixaKpi::nova(vec![
        CartaoKpi::novo("Resultado orçado", resultado.0).tom(Tom::Neutro),
        CartaoKpi::novo("Resultado realizado", resultado.1).tom(if resultado.1.e_negativo() {
            Tom::Negativo
        } else {
            Tom::Positivo
        }),
        CartaoKpi::novo("Diferença", diferenca).tom(if diferenca.e_negativo() {
            Tom::Negativo
        } else {
            Tom::Positivo
        }),
    ])
    .mostrar(ui);
    ui.add_space(Espaco::E12);
    let base = linhas_base(&d);
    ui.horizontal(|ui| {
        if let Some(v) = Abas::nova(&[(false, "Acumulado até o mês"), (true, "Mês a mês")])
            .selecionada(mes_a_mes)
            .id_salt("dfc-orcado-modo")
            .mostrar(ui)
        {
            estado.dfc_mensal.comparado_mes_a_mes = v;
        }
        ui.add_space(Espaco::E16);
        let ano = d.ano;
        let ate = mes.mes();
        let (linhas_export, base_export) = (linhas.clone(), base.clone());
        exportar::botoes(ui, motor, || {
            if mes_a_mes {
                relatorio_comparado_mes_a_mes(ano, &base_export)
            } else {
                relatorio_comparado(ano, ate, &linhas_export)
            }
        });
    });
    ui.add_space(Espaco::E8);
    if mes_a_mes {
        grade_comparada_mes_a_mes(ui, &base);
        return;
    }
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
        row.col(|ui| celula_rotulo(ui, &l.rotulo, l.tipo, l.recuada));
        row.col(|ui| super::dre::celula_valor(ui, l.orcado));
        row.col(|ui| super::dre::celula_valor(ui, l.realizado));
        row.col(|ui| celula_diferenca(ui, l.realizado - l.orcado));
        row.col(|ui| celula_percentual(ui, atingido(l.realizado, l.orcado)));
    });
}

fn celula_diferenca(ui: &mut egui::Ui, dif: Dinheiro) {
    if dif.e_zero() {
        super::dre::celula_valor(ui, dif);
    } else {
        ui.add(ValorDinheiro::novo(dif).com_sinal());
    }
}

/// O orçado e o realizado de cada mês lado a lado, com os totais e a diferença.
fn grade_comparada_mes_a_mes(ui: &mut egui::Ui, base: &[LinhaBase]) {
    let linhas: Vec<&LinhaBase> = base
        .iter()
        .filter(|l| {
            l.tipo == Tipo::Subtotal || !(l.vazia(Fonte::Orcado) && l.vazia(Fonte::Realizado))
        })
        .collect();
    let mut colunas = vec![ColunaGrade::nova("Conta").largura(280.0)];
    for m in 0..12 {
        colunas.push(ColunaGrade::nova(ORC[m]).largura(98.0).numero());
        colunas.push(ColunaGrade::nova(REAL[m]).largura(98.0).numero());
    }
    colunas.push(ColunaGrade::nova("Total orçado").largura(120.0).numero());
    colunas.push(ColunaGrade::nova("Total realizado").largura(120.0).numero());
    colunas.push(ColunaGrade::nova("Diferença").largura(120.0).numero());
    Grade::nova(colunas)
        .id_salt("dfc-orcado-mes-a-mes")
        .rolagem_horizontal()
        .mostrar(ui, linhas.len(), |i, row| {
            let l = linhas[i];
            row.col(|ui| celula_rotulo(ui, &l.rotulo, l.tipo, l.recuada));
            for m in 0..12 {
                row.col(|ui| super::dre::celula_valor(ui, l.orcado[m]));
                row.col(|ui| super::dre::celula_valor(ui, l.realizado[m]));
            }
            let (o, r): (Dinheiro, Dinheiro) = (
                l.orcado.iter().copied().sum(),
                l.realizado.iter().copied().sum(),
            );
            row.col(|ui| super::dre::celula_valor(ui, o));
            row.col(|ui| super::dre::celula_valor(ui, r));
            row.col(|ui| celula_diferenca(ui, r - o));
        });
}

fn relatorio_comparado(ano: i32, ate: u32, linhas: &[LinhaComparada]) -> Relatorio {
    Relatorio {
        titulo: "Orçado × realizado".to_owned(),
        subtitulo: format!(
            "Janeiro a {}",
            Competencia::nova(ano, ate.clamp(1, 12)).formatar_extenso()
        ),
        tabelas: vec![Tabela {
            titulo: "Acumulado".to_owned(),
            nota: Some("Pelo caixa. A diferença positiva é sempre melhor para o caixa.".to_owned()),
            colunas: vec![
                Coluna::texto("Conta", 4.0),
                Coluna::numero("Orçado", 1.6),
                Coluna::numero("Realizado", 1.6),
                Coluna::numero("Diferença", 1.6),
                Coluna::numero("Atingido", 1.2),
            ],
            linhas: linhas
                .iter()
                .map(|l| {
                    let linha = LinhaExport::nova(vec![
                        Valor::texto(l.rotulo.clone()),
                        Valor::Dinheiro(l.orcado),
                        Valor::Dinheiro(l.realizado),
                        Valor::Dinheiro(l.realizado - l.orcado),
                        Valor::texto(
                            atingido(l.realizado, l.orcado)
                                .map_or_else(String::new, |p| format!("{}%", p.formatar(1))),
                        ),
                    ])
                    .recuo(recuo_de(l.tipo, l.recuada));
                    if destaque_de(l.tipo) {
                        linha.destaque()
                    } else {
                        linha
                    }
                })
                .collect(),
        }],
    }
}

fn relatorio_comparado_mes_a_mes(ano: i32, base: &[LinhaBase]) -> Relatorio {
    let mut colunas = vec![Coluna::texto("Conta", 4.0)];
    for m in 0..12 {
        colunas.push(Coluna::numero(ORC[m], 1.2));
        colunas.push(Coluna::numero(REAL[m], 1.2));
    }
    colunas.push(Coluna::numero("Total orçado", 1.4));
    colunas.push(Coluna::numero("Total realizado", 1.4));
    colunas.push(Coluna::numero("Diferença", 1.4));
    Relatorio {
        titulo: "Orçado × realizado mês a mês".to_owned(),
        subtitulo: ano.to_string(),
        tabelas: vec![Tabela {
            titulo: "Mês a mês".to_owned(),
            nota: Some("Pelo caixa.".to_owned()),
            colunas,
            linhas: base
                .iter()
                .filter(|l| {
                    l.tipo == Tipo::Subtotal
                        || !(l.vazia(Fonte::Orcado) && l.vazia(Fonte::Realizado))
                })
                .map(|l| {
                    let mut celulas = vec![Valor::texto(l.rotulo.clone())];
                    for m in 0..12 {
                        celulas.push(Valor::Dinheiro(l.orcado[m]));
                        celulas.push(Valor::Dinheiro(l.realizado[m]));
                    }
                    let o: Dinheiro = l.orcado.iter().copied().sum();
                    let r: Dinheiro = l.realizado.iter().copied().sum();
                    celulas.push(Valor::Dinheiro(o));
                    celulas.push(Valor::Dinheiro(r));
                    celulas.push(Valor::Dinheiro(r - o));
                    let linha = LinhaExport::nova(celulas).recuo(recuo_de(l.tipo, l.recuada));
                    if destaque_de(l.tipo) {
                        linha.destaque()
                    } else {
                        linha
                    }
                })
                .collect(),
        }],
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use mod_financeiro::dfc_gerencial::{GrupoDfcGerencial, LinhaGerencial};

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
            a_vencer: [Dinheiro::ZERO; 12],
            orcado: o,
        }
    }

    fn grupo(g: GrupoGerencial, linhas: Vec<LinhaGerencial>) -> GrupoDfcGerencial {
        let mut realizado = [Dinheiro::ZERO; 12];
        let mut a_vencer = [Dinheiro::ZERO; 12];
        let mut orcado = [Dinheiro::ZERO; 12];
        for l in &linhas {
            somar(&mut realizado, l.realizado);
            somar(&mut a_vencer, l.a_vencer);
            somar(&mut orcado, l.orcado);
        }
        GrupoDfcGerencial {
            grupo: g,
            linhas,
            realizado,
            a_vencer,
            orcado,
        }
    }

    fn dfc() -> DfcGerencial {
        let mut aluguel_a_vencer = conta(
            "Aluguel",
            Some("Estrutura"),
            meses(&[(0, -300), (1, -300)]),
            meses(&[(0, -300), (1, -300)]),
        );
        aluguel_a_vencer.a_vencer = meses(&[(10, -300)]);
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
                    aluguel_a_vencer,
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
            somar(&mut variacao, g.realizado);
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
    fn mes_a_mes_tem_os_subtotais_do_dono_e_fecha_no_caixa() {
        let d = dfc();
        let l = linhas_mes_a_mes(&d, Fonte::Realizado);
        let rotulos: Vec<(&str, Tipo)> = l.iter().map(|l| (l.rotulo.as_str(), l.tipo)).collect();
        assert_eq!(
            rotulos,
            [
                ("Caixa no início do mês", Tipo::Saldo),
                ("Receitas operacionais", Tipo::Grupo),
                ("Vendas", Tipo::Conta),
                ("(=) Receita líquida", Tipo::Subtotal),
                ("(=) Margem de contribuição", Tipo::Subtotal),
                ("Despesas operacionais", Tipo::Grupo),
                ("Estrutura", Tipo::Agrupadora),
                ("Aluguel", Tipo::Conta),
                ("Energia", Tipo::Conta),
                ("(=) Resultado operacional", Tipo::Subtotal),
                ("Atividades de financiamento", Tipo::Grupo),
                ("Empréstimos", Tipo::Conta),
                ("(=) Resultado líquido do caixa", Tipo::Subtotal),
                ("Caixa no fim do mês", Tipo::Saldo),
            ]
        );
        assert_eq!(l[6].valores[0], Dinheiro::reais(-400));
        assert!(l[7].recuada);
        assert_eq!(l[9].total, Dinheiro::reais(1_100));
        assert_eq!(l[3].total, Dinheiro::reais(1_800));
        // O caixa: 50 + 600 em janeiro + 2.500 em fevereiro; o total do início é janeiro.
        assert_eq!(l[0].total, Dinheiro::reais(50));
        assert_eq!(l[13].valores[1], Dinheiro::reais(3_150));
        assert_eq!(l[13].total, Dinheiro::reais(3_150));
        assert_eq!(l[12].total, Dinheiro::reais(3_100));
    }

    #[test]
    fn no_orcado_e_no_a_vencer_so_aparece_o_que_tem_valor() {
        let d = dfc();
        let l = linhas_mes_a_mes(&d, Fonte::Orcado);
        assert!(l
            .iter()
            .all(|l| l.rotulo != "Energia" && l.rotulo != "Empréstimos"));
        assert!(l.iter().all(|l| l.rotulo != "Atividades de financiamento"));
        // 50 + 600 + 600.
        assert_eq!(l.last().unwrap().valores[1], Dinheiro::reais(1_250));
        assert_eq!(receitas(&d, Fonte::Orcado)[0], Dinheiro::reais(900));
        // A vencer: só o aluguel de novembro.
        let a = linhas_mes_a_mes(&d, Fonte::AVencer);
        assert!(a.iter().any(|l| l.rotulo == "Aluguel"));
        assert!(a.iter().all(|l| l.rotulo != "Vendas"));
        // Projetado: o realizado do ano mais o aluguel a vencer.
        let p = linhas_mes_a_mes(&d, Fonte::Projetado);
        assert_eq!(p.last().unwrap().total, Dinheiro::reais(2_850));
    }

    #[test]
    fn orcado_realizado_acumula_ate_o_mes_e_exporta() {
        let d = dfc();
        let l = linhas_comparadas(&d, 2);
        let vendas = l.iter().find(|l| l.rotulo == "Vendas").unwrap();
        assert_eq!(vendas.orcado, Dinheiro::reais(1_800));
        assert_eq!(vendas.realizado, Dinheiro::reais(1_800));
        let energia = l.iter().find(|l| l.rotulo == "Energia").unwrap();
        assert_eq!(energia.orcado, Dinheiro::ZERO);
        assert_eq!(energia.realizado, Dinheiro::reais(-100));
        let ultima = l.last().unwrap();
        assert_eq!(ultima.rotulo, "(=) Resultado líquido do caixa");
        assert_eq!(ultima.orcado, Dinheiro::reais(1_200));
        assert_eq!(ultima.realizado, Dinheiro::reais(3_100));
        // Só janeiro.
        let jan = linhas_comparadas(&d, 1);
        assert_eq!(
            jan.iter().find(|l| l.rotulo == "Vendas").unwrap().realizado,
            Dinheiro::reais(1_000)
        );
        // Os relatórios exportados têm as colunas da tela.
        let r = relatorio_comparado_mes_a_mes(2026, &linhas_base(&d));
        assert_eq!(r.tabelas[0].colunas.len(), 1 + 24 + 3);
        assert!(r.tabelas[0].linhas.iter().all(|x| x.celulas.len() == 28));
        let m = relatorio_mes_a_mes(
            2026,
            Fonte::Realizado,
            &linhas_mes_a_mes(&d, Fonte::Realizado),
        );
        assert!(m.tabelas[0].linhas.iter().all(|x| x.celulas.len() == 14));
        let c = relatorio_comparado(2026, 2, &l);
        assert_eq!(c.subtitulo, "Janeiro a fevereiro de 2026");
    }
}
