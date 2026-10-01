//! A chave "Pessoal" do Financeiro: as finanças do próprio usuário — o que entra (pró-labore,
//! salário, vendas por fora), o que sai (contas, faculdade, compras) e as faturas do cartão com
//! parcelamentos — com a projeção dos próximos meses. Fica fora da contabilidade da empresa
//! (`mod_financeiro::pessoal`) e cada usuário só vê o seu.

mod cartoes;
mod lancar;
mod lista;
#[cfg(test)]
mod testes;
mod visao;

use cardeal_cliente::{Motor, Sessao};
use cardeal_kernel::{Competencia, Data, Dinheiro, Fuso, Id, Periodo};
use cardeal_modkit::Icone;
use cardeal_ui::atoms::{Botao, Etiqueta, Rotulo, ValorDinheiro, ATALHO_NOVO};
use cardeal_ui::molecules::{Abas, CartaoKpi, EstadoVazio};
use cardeal_ui::organisms::{notificar, ColunaGrade, FaixaKpi, Grade, Notificacao};
use cardeal_ui::tokens::{Espaco, TemaUi};
use eframe::egui;
use mod_financeiro::pessoal::comandos::{ExcluirPessoal, MarcarPessoalPago};
use mod_financeiro::pessoal::consultas::{LancamentosPessoais, Sugestoes, SugestoesPessoais};
use mod_financeiro::pessoal::{LancamentoPessoal, TipoPessoal};

pub(super) use lancar::FormPessoal;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum AbaPessoal {
    #[default]
    Visao,
    Lancamentos,
    Cartoes,
}

/// O diálogo aberto.
#[derive(Default)]
enum DlgPessoal {
    #[default]
    Fechado,
    Novo(Box<FormPessoal>),
    /// Um lançamento: marcar pago, excluir.
    Ver(Id),
}

/// Estado da parte pessoal.
#[derive(Default)]
pub(crate) struct EstadoPessoal {
    aba: AbaPessoal,
    /// O mês em foco; `None` = o atual.
    mes: Option<Competencia>,
    /// Seis meses para trás e doze para a frente do mês atual.
    lancamentos: Vec<LancamentoPessoal>,
    sugestoes: Sugestoes,
    busca: String,
    erro: Option<String>,
    carregado: bool,
    dlg: DlgPessoal,
}

impl EstadoPessoal {
    fn mes(&self) -> Competencia {
        self.mes
            .unwrap_or_else(|| Data::hoje(Fuso::BRASILIA).competencia())
    }

    pub(crate) fn carregar(&mut self, motor: &Motor, sessao: &Sessao) {
        self.carregado = true;
        let atual = Data::hoje(Fuso::BRASILIA).inicio_do_mes();
        let periodo = Periodo::novo(atual.mais_meses(-6), atual.mais_meses(12).fim_do_mes());
        match motor.consultar(
            sessao,
            "financeiro.lancamentos_pessoais.v1",
            &LancamentosPessoais { periodo },
        ) {
            Ok(v) => {
                self.lancamentos = v;
                self.erro = None;
            }
            Err(e) => self.erro = Some(e.mensagem),
        }
        if let Ok(s) = motor.consultar(
            sessao,
            "financeiro.sugestoes_pessoais.v1",
            &SugestoesPessoais,
        ) {
            self.sugestoes = s;
        }
    }

    fn do_mes(&self) -> impl Iterator<Item = &LancamentoPessoal> {
        let mes = self.mes();
        self.lancamentos
            .iter()
            .filter(move |l| l.vencimento.competencia() == mes)
    }

    fn marcar(
        &mut self,
        ctx: &egui::Context,
        motor: &Motor,
        sessao: &Sessao,
        ids: Vec<Id>,
        pago: bool,
    ) {
        let r = motor.executar(
            sessao,
            "financeiro.marcar_pessoal_pago.v1",
            &MarcarPessoalPago {
                lancamentos: ids,
                pago,
                data: Data::hoje(Fuso::BRASILIA),
            },
        );
        match r {
            Ok(n) => {
                let _: usize = n;
                self.carregar(motor, sessao);
                notificar(
                    ctx,
                    Notificacao::sucesso(if pago {
                        "Marcado como pago"
                    } else {
                        "Voltou para previsto"
                    }),
                );
            }
            Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
        }
    }
}

/// Os botões do cabeçalho no modo pessoal.
pub(crate) fn acoes(ui: &mut egui::Ui, estado: &mut EstadoPessoal) {
    if ui
        .add(Botao::primario("+ Despesa").tecla(ATALHO_NOVO))
        .clicked()
    {
        estado.dlg = DlgPessoal::Novo(Box::new(FormPessoal::novo(TipoPessoal::Despesa)));
    }
    if ui.add(Botao::secundario("+ Receita")).clicked() {
        estado.dlg = DlgPessoal::Novo(Box::new(FormPessoal::novo(TipoPessoal::Receita)));
    }
}

/// O corpo da tela no modo pessoal.
pub(crate) fn corpo(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoPessoal) {
    if !estado.carregado {
        estado.carregar(motor, sessao);
    }
    if let Some(nova) = Abas::nova(&[
        (AbaPessoal::Visao, "Visão e projeção"),
        (AbaPessoal::Lancamentos, "Lançamentos do mês"),
        (AbaPessoal::Cartoes, "Cartões de crédito"),
    ])
    .selecionada(estado.aba)
    .id_salt("abas-pessoal")
    .mostrar(ui)
    {
        estado.aba = nova;
    }
    ui.add_space(Espaco::E16);
    if let Some(erro) = &estado.erro {
        ui.add(
            Rotulo::interface(erro.clone())
                .quebravel()
                .cor(ui.cores().negativo),
        );
        ui.add_space(Espaco::E12);
    }
    navegador_mes(ui, estado);
    ui.add_space(Espaco::E12);
    match estado.aba {
        AbaPessoal::Visao => visao::painel(ui, estado),
        AbaPessoal::Lancamentos => lista::painel(ui, estado),
        AbaPessoal::Cartoes => cartoes::painel(ui, motor, sessao, estado),
    }
}

/// Os diálogos do modo pessoal.
pub(crate) fn dialogos(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoPessoal,
) {
    match estado.dlg {
        DlgPessoal::Fechado => {}
        DlgPessoal::Novo(_) => lancar::dialogo(ctx, motor, sessao, estado),
        DlgPessoal::Ver(id) => lista::dialogo_ver(ctx, motor, sessao, estado, id),
    }
}

/// "‹ outubro de 2026 ›" — o mês em foco das três abas.
fn navegador_mes(ui: &mut egui::Ui, estado: &mut EstadoPessoal) {
    let mes = estado.mes();
    let atual = Data::hoje(Fuso::BRASILIA).competencia();
    ui.horizontal(|ui| {
        if ui.add(Botao::fantasma("‹ Anterior").pequeno()).clicked() {
            estado.mes = Some(mes.anterior());
        }
        ui.add(Rotulo::titulo_secao(capitalizar(&mes.formatar_extenso())));
        if ui.add(Botao::fantasma("Próximo ›").pequeno()).clicked() {
            estado.mes = Some(mes.proxima());
        }
        if mes != atual && ui.add(Botao::secundario("Hoje").pequeno()).clicked() {
            estado.mes = None;
        }
    });
}

fn capitalizar(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|p| p.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

/// "out/26".
fn mes_curto(c: Competencia) -> String {
    const M: [&str; 12] = [
        "jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez",
    ];
    format!(
        "{}/{:02}",
        M[(c.mes().clamp(1, 12) - 1) as usize],
        c.ano() % 100
    )
}

/// Reais em `f64`, só para gráfico.
#[allow(clippy::cast_precision_loss)]
fn reais(d: Dinheiro) -> f64 {
    d.em_centavos() as f64 / 100.0
}

/// Etiqueta da situação de um lançamento.
fn etiqueta_situacao(l: &LancamentoPessoal, hoje: Data) -> Etiqueta {
    match (l.pago(), l.tipo) {
        (true, TipoPessoal::Receita) => Etiqueta::positiva("Recebido"),
        (true, TipoPessoal::Despesa) => Etiqueta::positiva("Pago"),
        (false, _) if l.vencimento < hoje => Etiqueta::negativa("Atrasado"),
        (false, _) => Etiqueta::neutra("Previsto"),
    }
}

/// Valor com sinal: receita positiva, despesa negativa (a cor segue).
fn valor_com_sinal(l: &LancamentoPessoal) -> Dinheiro {
    match l.tipo {
        TipoPessoal::Receita => l.valor,
        TipoPessoal::Despesa => Dinheiro::ZERO - l.valor,
    }
}

fn excluir(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoPessoal,
    id: Id,
    e_seguintes: bool,
) {
    match motor.executar(
        sessao,
        "financeiro.excluir_pessoal.v1",
        &ExcluirPessoal {
            lancamento: id,
            e_seguintes,
        },
    ) {
        Ok(n) => {
            let n: usize = n;
            estado.dlg = DlgPessoal::Fechado;
            estado.carregar(motor, sessao);
            notificar(
                ctx,
                Notificacao::sucesso(if n == 1 {
                    "Lançamento excluído".to_owned()
                } else {
                    format!("{n} lançamentos excluídos")
                }),
            );
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

#[cfg(feature = "demo")]
impl EstadoPessoal {
    /// Leva a parte pessoal à cena de demonstração.
    pub(crate) fn preparar_demo(&mut self, motor: &Motor, sessao: &Sessao, cena: &str) {
        self.carregar(motor, sessao);
        match cena {
            "financeiro-cartoes" => self.aba = AbaPessoal::Cartoes,
            "financeiro-pessoal-novo" => {
                let mut f = FormPessoal::novo(TipoPessoal::Despesa);
                f.demo();
                self.dlg = DlgPessoal::Novo(Box::new(f));
            }
            _ => {}
        }
    }
}
