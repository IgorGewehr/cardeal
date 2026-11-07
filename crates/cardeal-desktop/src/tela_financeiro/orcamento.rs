//! Resultado › "Orçamento": o orçado de cada conta no ano — as do resultado e as que se orçam pelo
//! caixa (compras, investimentos, financiamentos) —, o orçado e o realizado até o mês escolhido e o
//! desvio (positivo é sempre melhor), o editor dos 12 meses de uma conta, a planilha do ano
//! (`orcamento_planilha`) e a cópia do ano anterior com reajuste (`mod_financeiro::orcamento`).

use super::*;
use cardeal_kernel::Percentual;
use mod_financeiro::orcamento::{
    desvio, CopiarOrcamentoDoAno, DefinirOrcamentoDaConta, LinhaDeOrcamento, OrcamentoAnual,
    OrcamentoDoAno,
};

/// Quem orça.
const PERMISSAO_EDITAR: &str = "financeiro.orcamento.editar";

const MESES: [&str; 12] = [
    "Jan", "Fev", "Mar", "Abr", "Mai", "Jun", "Jul", "Ago", "Set", "Out", "Nov", "Dez",
];

/// O editor de uma conta e a cópia do ano anterior.
#[derive(Default)]
pub(super) struct FormOrcamento {
    conta: Option<Id>,
    valores: [String; 12],
    todos: String,
    reajuste: String,
    erro: Option<String>,
    /// Mostra a planilha do ano (todas as contas × 12 meses) em vez da lista por conta.
    pub(super) planilha: bool,
    /// O que está digitado na planilha.
    pub(super) planilha_do_ano: Option<super::orcamento_planilha::Planilha>,
}

impl EstadoTelaFinanceiro {
    pub(super) fn carregar_orcamento(&mut self, motor: &Motor, sessao: &Sessao) {
        let ano = self.mes_dre().ano();
        self.orcamento = Some(
            motor
                .consultar(
                    sessao,
                    "financeiro.orcamento_do_ano.v1",
                    &OrcamentoDoAno { ano },
                )
                .map_err(|e| e.mensagem),
        );
    }
}

fn texto_desvio(ui: &mut egui::Ui, d: Option<Percentual>) {
    match d {
        Some(p) => {
            let melhor = p.unidades_internas() >= 0;
            let sinal = if melhor { "+" } else { "" };
            let cor = if melhor {
                ui.cores().positivo
            } else {
                ui.cores().negativo
            };
            ui.add(Rotulo::interface(format!("{sinal}{}%", p.formatar(1))).cor(cor));
        }
        None => {
            ui.add(Rotulo::interface("—").cor(ui.cores().texto_fraco));
        }
    }
}

pub(super) fn painel_orcamento(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) {
    let o = match &estado.orcamento {
        Some(Ok(o)) => o.clone(),
        Some(Err(e)) => {
            ui.add(
                Rotulo::interface(e.clone())
                    .quebravel()
                    .cor(ui.cores().negativo),
            );
            return;
        }
        None => {
            EstadoVazio::novo(Icone::Dinheiro, "O orçamento ainda não carregou.").mostrar(ui);
            return;
        }
    };
    let ate = estado.mes_dre().mes();
    let pode_editar = sessao.concede(PERMISSAO_EDITAR);
    ui.add(Rotulo::titulo_secao(format!("Orçamento de {}", o.ano)));
    ui.add(
        Rotulo::apoio(format!(
            "Quanto se espera de cada receita e de cada despesa, mês a mês — e das compras, dos \
             investimentos e dos financiamentos, pelo caixa. Até {}: o orçado e o realizado \
             (receitas e despesas pela competência, como na DRE; as de caixa pelo que entrou e \
             saiu); o desvio é positivo quando é melhor. Resultado orçado no ano: {}.",
            nome_mes(estado.mes_dre()),
            o.resultado_orcado().formatar_com_simbolo()
        ))
        .quebravel(),
    );
    ui.add_space(Espaco::E8);
    if pode_editar {
        copiar_do_ano_anterior(ui, motor, sessao, estado, &o);
        ui.add_space(Espaco::E8);
        if let Some(v) = Abas::nova(&[(false, "Por conta"), (true, "Planilha do ano")])
            .selecionada(estado.form_orcamento.planilha)
            .id_salt("orcamento-modo")
            .mostrar(ui)
        {
            estado.form_orcamento.planilha = v;
        }
        ui.add_space(Espaco::E8);
        if estado.form_orcamento.planilha {
            super::orcamento_planilha::painel(ui, motor, sessao, estado, &o);
            return;
        }
    }
    let mut abrir: Option<LinhaDeOrcamento> = None;
    // Quem orça vê também as contas ainda sem orçamento nem movimento, para orçar.
    let todas: Vec<&LinhaDeOrcamento> = o
        .linhas
        .iter()
        .filter(|l| pode_editar || l.tem_orcamento() || !l.realizado_ate(12).e_zero())
        .collect();
    Grade::nova(vec![
        ColunaGrade::nova("Conta"),
        ColunaGrade::nova("Bloco").largura(220.0),
        ColunaGrade::nova("Orçado no ano").largura(140.0).numero(),
        ColunaGrade::nova("Orçado até o mês")
            .largura(140.0)
            .numero(),
        ColunaGrade::nova("Realizado até o mês")
            .largura(150.0)
            .numero(),
        ColunaGrade::nova("Desvio").largura(100.0).numero(),
    ])
    .selecionavel(None)
    .id_salt("orcamento-do-ano")
    .vazio("Nenhuma conta orçada nem com movimento no ano.")
    .mostrar(ui, todas.len(), |i, row| {
        let l = todas[i];
        row.col(|ui| {
            ui.add(Rotulo::interface(format!("{} {}", l.codigo, l.nome)));
        });
        row.col(|ui| {
            ui.add(Rotulo::campo(l.bloco.clone()));
        });
        row.col(|ui| celula(ui, l.orcado_no_ano()));
        row.col(|ui| celula(ui, l.orcado_ate(ate)));
        row.col(|ui| celula(ui, l.realizado_ate(ate)));
        row.col(|ui| {
            texto_desvio(
                ui,
                desvio(l.realizado_ate(ate), l.orcado_ate(ate), l.soma_no_resultado),
            );
        });
        if pode_editar && row.response().clicked() {
            abrir = Some(l.clone());
        }
    });
    if let Some(l) = abrir {
        estado.form_orcamento.conta = Some(l.conta);
        estado.form_orcamento.valores = l.orcado.map(|v| {
            if v.e_zero() {
                String::new()
            } else {
                v.formatar()
            }
        });
        estado.form_orcamento.todos.clear();
        estado.form_orcamento.erro = None;
    }
    if let Some(conta) = estado.form_orcamento.conta {
        if let Some(l) = o.linhas.iter().find(|l| l.conta == conta) {
            ui.add_space(Espaco::E12);
            editor(ui, motor, sessao, estado, l, o.ano);
        }
    }
}

fn celula(ui: &mut egui::Ui, v: Dinheiro) {
    if v.e_zero() {
        ui.add(Rotulo::interface("—").cor(ui.cores().texto_fraco));
    } else {
        ui.add(ValorDinheiro::novo(v).neutro());
    }
}

/// Os 12 meses de uma conta.
fn editor(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
    l: &LinhaDeOrcamento,
    ano: i32,
) {
    let f = &mut estado.form_orcamento;
    ui.add(Rotulo::titulo_secao(format!(
        "{} {} — {ano}",
        l.codigo, l.nome
    )));
    ui.horizontal(|ui| {
        ui.add(Campo::novo("O mesmo valor em todo mês", &mut f.todos).marcador("0,00"));
        if ui
            .add(Botao::secundario("Aplicar aos 12 meses").pequeno())
            .clicked()
        {
            let v = f.todos.trim().to_owned();
            for m in &mut f.valores {
                m.clone_from(&v);
            }
        }
    });
    ui.columns(4, |colunas| {
        for (i, valor) in f.valores.iter_mut().enumerate() {
            colunas[i % 4].add(Campo::novo(MESES[i], valor).marcador("0,00"));
        }
    });
    if let Some(e) = &f.erro {
        ui.add(Rotulo::interface(e.clone()).cor(ui.cores().negativo));
    }
    let mut salvar = false;
    let mut fechar = false;
    ui.horizontal(|ui| {
        salvar = ui.add(Botao::primario("Salvar")).clicked();
        fechar = ui.add(Botao::fantasma("Fechar")).clicked();
    });
    if fechar {
        estado.form_orcamento.conta = None;
        return;
    }
    if !salvar {
        return;
    }
    let mut meses = [Dinheiro::ZERO; 12];
    for (i, texto) in estado.form_orcamento.valores.iter().enumerate() {
        if texto.trim().is_empty() {
            continue;
        }
        match Dinheiro::de_str(texto) {
            // Negativo só na conta de caixa: a parcela paga do empréstimo.
            Ok(v) if !v.e_negativo() || l.de_caixa => meses[i] = v,
            _ => {
                estado.form_orcamento.erro =
                    Some(format!("o valor de {} não é um valor positivo", MESES[i]));
                return;
            }
        }
    }
    let r: Result<(), _> = motor.executar(
        sessao,
        "financeiro.definir_orcamento_da_conta.v1",
        &DefinirOrcamentoDaConta {
            conta: l.conta,
            ano,
            meses,
        },
    );
    match r {
        Ok(()) => {
            notificar(ui.ctx(), Notificacao::sucesso("Orçamento salvo"));
            estado.form_orcamento.conta = None;
            estado.carregar_dre(motor, sessao);
        }
        Err(e) => estado.form_orcamento.erro = Some(e.mensagem),
    }
}

/// "Copiar de {ano − 1}", com reajuste — só nas contas ainda sem orçamento no ano.
fn copiar_do_ano_anterior(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
    o: &OrcamentoAnual,
) {
    let mut copiar = false;
    // O campo tem largura própria: sozinho ele ocupa a linha inteira e empurrava o botão para
    // fora da tela. O botão alinha pela base, com a caixa do campo (não com o rótulo).
    let linha = egui::vec2(ui.available_width(), 64.0);
    ui.allocate_ui_with_layout(linha, egui::Layout::left_to_right(egui::Align::Max), |ui| {
        ui.allocate_ui(egui::vec2(260.0, 64.0), |ui| {
            ui.add(
                Campo::novo(
                    "Reajuste sobre o ano anterior (%)",
                    &mut estado.form_orcamento.reajuste,
                )
                .marcador("0"),
            );
        });
        copiar = ui
            .add(Botao::secundario(format!("Copiar de {}", o.ano - 1)))
            .clicked();
    });
    if !copiar {
        return;
    }
    let texto = estado.form_orcamento.reajuste.trim().to_owned();
    let reajuste = if texto.is_empty() {
        Percentual::ZERO
    } else if let Ok(p) = Percentual::de_str(&texto) {
        p
    } else {
        notificar(
            ui.ctx(),
            Notificacao::erro("O reajuste deve ser um número (ex.: 5)"),
        );
        return;
    };
    let r: Result<u32, _> = motor.executar(
        sessao,
        "financeiro.copiar_orcamento_do_ano.v1",
        &CopiarOrcamentoDoAno {
            de: o.ano - 1,
            para: o.ano,
            reajuste,
        },
    );
    match r {
        Ok(0) => notificar(
            ui.ctx(),
            Notificacao::aviso(format!(
                "Nada a copiar: {} não tem orçamento, ou as contas já estão orçadas em {}",
                o.ano - 1,
                o.ano
            )),
        ),
        Ok(n) => {
            notificar(
                ui.ctx(),
                Notificacao::sucesso(format!("{n} conta(s) copiada(s) de {}", o.ano - 1)),
            );
            estado.carregar_dre(motor, sessao);
        }
        Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
    }
}
