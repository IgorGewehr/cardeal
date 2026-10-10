//! A aba "Análise": curva ABC, cobertura em dias, giro e risco de ruptura, pelo que saiu do
//! estoque na janela escolhida. Responde "o que não pode faltar", "quanto dura" e "o que está
//! parado".

use super::*;
use cardeal_kernel::Dinheiro;
use cardeal_ui::atoms::ValorDinheiro;
use cardeal_ui::molecules::Abas;
use mod_estoque::analise::{ClasseAbc, ItemDeAnalise};
use mod_estoque::AnaliseDeEstoque;

/// O recorte da lista.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Recorte {
    #[default]
    Todos,
    A,
    B,
    C,
    Parados,
    Ruptura,
}

/// O estado da aba.
#[derive(Default)]
pub(super) struct EstadoAnalise {
    dias: u32,
    pub(super) itens: Vec<ItemDeAnalise>,
    recorte: Recorte,
}

impl EstadoAnalise {
    pub(super) fn carregar(&mut self, motor: &Motor, sessao: &Sessao) {
        if self.dias == 0 {
            self.dias = 90;
        }
        if let Ok(i) = motor.consultar(
            sessao,
            "estoque.analise_de_estoque.v1",
            &AnaliseDeEstoque { dias: self.dias },
        ) {
            self.itens = i;
        }
    }

    fn filtrados(&self) -> Vec<&ItemDeAnalise> {
        self.itens
            .iter()
            .filter(|i| match self.recorte {
                Recorte::Todos => true,
                Recorte::A => i.classe == ClasseAbc::A,
                Recorte::B => i.classe == ClasseAbc::B,
                Recorte::C => i.classe == ClasseAbc::C,
                Recorte::Parados => i.classe == ClasseAbc::SemGiro && i.saldo.e_positiva(),
                Recorte::Ruptura => i.risco_de_ruptura,
            })
            .collect()
    }
}

fn etiqueta(c: ClasseAbc) -> Etiqueta {
    match c {
        ClasseAbc::A => Etiqueta::positiva("A"),
        ClasseAbc::B => Etiqueta::info("B"),
        ClasseAbc::C => Etiqueta::neutra("C"),
        ClasseAbc::SemGiro => Etiqueta::atencao("sem giro"),
    }
}

pub(super) fn aba(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
) {
    ui.horizontal_wrapped(|ui| {
        ui.add(Rotulo::sobrelinha("Janela"));
        ui.add_space(Espaco::E8);
        if let Some(d) = Abas::nova(&[
            (30_u32, "30 dias"),
            (60, "60 dias"),
            (90, "90 dias"),
            (180, "180 dias"),
        ])
        .selecionada(estado.analise.dias.max(1))
        .id_salt("analise-dias")
        .mostrar(ui)
        {
            estado.analise.dias = d;
            estado.analise.carregar(motor, sessao);
        }
    });
    ui.add_space(Espaco::E12);

    let itens = &estado.analise.itens;
    let parado: Dinheiro = itens
        .iter()
        .filter(|i| i.classe == ClasseAbc::SemGiro)
        .map(|i| i.valor_em_estoque)
        .sum();
    let em_a = itens.iter().filter(|i| i.classe == ClasseAbc::A).count();
    let ruptura = itens.iter().filter(|i| i.risco_de_ruptura).count();
    FaixaKpi::nova(vec![
        CartaoKpi::contagem("Produtos classe A", em_a).variacao("80% do que sai"),
        CartaoKpi::novo("Dinheiro parado (sem giro)", parado).tom(if parado.e_positivo() {
            Tom::Atencao
        } else {
            Tom::Neutro
        }),
        CartaoKpi::contagem("Risco de ruptura", ruptura)
            .tom(if ruptura > 0 {
                Tom::Negativo
            } else {
                Tom::Positivo
            })
            .variacao("duram menos de 7 dias"),
    ])
    .mostrar(ui);
    ui.add_space(Espaco::E12);

    if let Some(r) = Abas::nova(&[
        (Recorte::Todos, "Todos"),
        (Recorte::A, "A"),
        (Recorte::B, "B"),
        (Recorte::C, "C"),
        (Recorte::Parados, "Parados"),
        (Recorte::Ruptura, "Risco de ruptura"),
    ])
    .selecionada(estado.analise.recorte)
    .id_salt("analise-recorte")
    .mostrar(ui)
    {
        estado.analise.recorte = r;
    }
    ui.add_space(Espaco::E8);
    ui.add(Rotulo::apoio(
        "A curva é pelo valor que saiu do estoque ao custo (venda, PDV e OS). A cobertura diz \
         quantos dias o saldo dura no ritmo da janela; o giro, quantas vezes o valor em estoque \
         saiu nela.",
    ));
    ui.add_space(Espaco::E8);

    let linhas = estado.analise.filtrados();
    Grade::nova(vec![
        ColunaGrade::nova("Classe").largura(90.0),
        ColunaGrade::nova("Produto").largura(260.0),
        ColunaGrade::nova("Saiu").largura(90.0).numero(),
        ColunaGrade::nova("Valor saído").largura(120.0).numero(),
        ColunaGrade::nova("% acum.").largura(80.0).numero(),
        ColunaGrade::nova("Saldo").largura(90.0).numero(),
        ColunaGrade::nova("Cobertura").largura(110.0).numero(),
        ColunaGrade::nova("Giro").largura(80.0).numero(),
        ColunaGrade::nova("Em estoque").largura(120.0).numero(),
    ])
    .id_salt("analise-estoque")
    .vazio("Nenhum produto neste recorte.")
    .mostrar(ui, linhas.len(), |i, row| {
        let l = linhas[i];
        row.col(|ui| {
            ui.add(etiqueta(l.classe));
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(l.nome.clone()));
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(l.saidas.to_string()));
        });
        row.col(|ui| {
            ui.add(ValorDinheiro::novo(l.valor_saidas).neutro());
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(format!("{}%", l.acumulada.formatar(0))));
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(l.saldo.to_string()));
        });
        row.col(|ui| match l.cobertura_dias {
            Some(d) if l.risco_de_ruptura => {
                ui.add(Etiqueta::negativa(format!("{d} dias")));
            }
            Some(d) => {
                ui.add(Rotulo::interface(format!("{d} dias")));
            }
            None => {
                ui.add(Rotulo::interface("—").cor(ui.cores().texto_fraco));
            }
        });
        row.col(|ui| match l.giro {
            Some(g) => {
                ui.add(Rotulo::interface(format!(
                    "{}x",
                    g.unidades_internas() / 100_000_000
                )));
            }
            None => {
                ui.add(Rotulo::interface("—").cor(ui.cores().texto_fraco));
            }
        });
        row.col(|ui| {
            ui.add(ValorDinheiro::novo(l.valor_em_estoque).neutro());
        });
    });
}
