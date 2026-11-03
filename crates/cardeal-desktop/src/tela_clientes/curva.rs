//! Clientes › "Curva de clientes": quem compra quanto nos últimos 12 meses (classes A, B e C,
//! relativas à carteira da empresa), o ritmo de cada um, quem sumiu, como paga e o que fazer
//! (`mod_pdv::CurvaDeClientes`). Clicar abre o cliente.

use super::*;
use mod_financeiro::pagamento::SeloDePagamento;
use mod_pdv::{ClasseDoCliente, ClienteNaCurva, CurvaDeClientes, SituacaoDoCliente};

/// Quem vê a curva.
pub(super) const PERMISSAO: &str = "pdv.clientes.ver";

pub(super) fn carregar(motor: &Motor, sessao: &Sessao) -> Result<Vec<ClienteNaCurva>, String> {
    motor
        .consultar(sessao, "pdv.curva_de_clientes.v1", &CurvaDeClientes)
        .map_err(|e| e.mensagem)
}

/// A etiqueta do selo de pagamento.
pub(super) fn etiqueta_do_selo(selo: SeloDePagamento) -> Etiqueta {
    match selo {
        SeloDePagamento::BomPagador => Etiqueta::positiva(selo.rotulo()),
        SeloDePagamento::Regular | SeloDePagamento::SemHistorico => Etiqueta::neutra(selo.rotulo()),
        SeloDePagamento::Atencao => Etiqueta::atencao(selo.rotulo()),
        SeloDePagamento::Inadimplente => Etiqueta::negativa(selo.rotulo()),
    }
}

fn etiqueta_da_classe(c: ClasseDoCliente) -> Etiqueta {
    match c {
        ClasseDoCliente::A => Etiqueta::positiva("A"),
        ClasseDoCliente::B => Etiqueta::neutra("B"),
        ClasseDoCliente::C => Etiqueta::neutra("C"),
    }
}

fn etiqueta_da_situacao(s: SituacaoDoCliente) -> Etiqueta {
    match s {
        SituacaoDoCliente::Sumido => Etiqueta::atencao(s.rotulo()),
        SituacaoDoCliente::Novo => Etiqueta::positiva(s.rotulo()),
        SituacaoDoCliente::Ativo | SituacaoDoCliente::Esporadico => Etiqueta::neutra(s.rotulo()),
    }
}

/// Desenha a curva. Devolve o cliente clicado, para abrir.
pub(super) fn mostrar(
    ui: &mut egui::Ui,
    curva: &Result<Vec<ClienteNaCurva>, String>,
) -> Option<Id> {
    let curva = match curva {
        Ok(c) => c,
        Err(e) => {
            ui.add(
                Rotulo::interface(e.clone())
                    .quebravel()
                    .cor(ui.cores().negativo),
            );
            return None;
        }
    };
    if curva.is_empty() {
        EstadoVazio::novo(
            Icone::Pessoas,
            "Nenhuma venda com o cliente identificado nos últimos 12 meses.",
        )
        .mostrar(ui);
        return None;
    }
    let classe_a = curva
        .iter()
        .filter(|c| c.classe == ClasseDoCliente::A)
        .count();
    let sumidos = curva
        .iter()
        .filter(|c| c.situacao == SituacaoDoCliente::Sumido && c.classe != ClasseDoCliente::C)
        .count();
    let inadimplentes: Vec<&ClienteNaCurva> = curva
        .iter()
        .filter(|c| {
            c.pagamento
                .as_ref()
                .is_some_and(|p| p.selo == SeloDePagamento::Inadimplente)
        })
        .collect();
    let vencido = inadimplentes
        .iter()
        .filter_map(|c| c.pagamento.as_ref())
        .map(|p| p.vencido)
        .sum::<cardeal_kernel::Dinheiro>();
    FaixaKpi::nova(vec![
        CartaoKpi::contagem("Clientes que compraram", curva.len()).variacao("nos últimos 12 meses"),
        CartaoKpi::contagem("Classe A", classe_a).variacao("fazem 80% do faturamento"),
        CartaoKpi::contagem("A e B sumidos", sumidos).variacao("passaram do ritmo deles"),
        CartaoKpi::novo("Vencido há mais de 30 dias", vencido)
            .variacao(format!("{} cliente(s)", inadimplentes.len())),
    ])
    .mostrar(ui);
    ui.add(Rotulo::apoio(
        "Vendas do balcão com o cliente identificado e pedidos faturados. A classe é relativa à \
         sua carteira; \"sumido\" é passar do dobro do intervalo habitual do próprio cliente (e de \
         30 dias). O pagamento conta só as vendas a prazo.",
    ));
    ui.add_space(Espaco::E8);
    let mut aberto = None;
    let resposta = Grade::nova(vec![
        ColunaGrade::nova("Cliente"),
        ColunaGrade::nova("Classe").largura(70.0),
        ColunaGrade::nova("12 meses").largura(130.0).numero(),
        ColunaGrade::nova("Compras").largura(80.0).numero(),
        ColunaGrade::nova("Última").largura(100.0),
        ColunaGrade::nova("Ritmo").largura(120.0),
        ColunaGrade::nova("Situação").largura(110.0),
        ColunaGrade::nova("Pagamento").largura(130.0),
        ColunaGrade::nova("O que fazer"),
    ])
    .selecionavel(None)
    .id_salt("curva-de-clientes")
    .mostrar(ui, curva.len(), |i, row| {
        let c = &curva[i];
        row.col(|ui| {
            ui.add(Rotulo::interface(c.nome.clone()));
        });
        row.col(|ui| {
            ui.add(etiqueta_da_classe(c.classe));
        });
        row.col(|ui| {
            ui.add(ValorDinheiro::novo(c.faturamento).neutro());
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(c.compras.to_string()));
        });
        row.col(|ui| {
            ui.add(Rotulo::campo(c.ultima_compra.formatar()));
        });
        row.col(|ui| {
            ui.add(Rotulo::campo(c.intervalo_dias.map_or_else(
                || "uma compra".to_owned(),
                |d| format!("a cada {d} dias"),
            )));
        });
        row.col(|ui| {
            ui.add(etiqueta_da_situacao(c.situacao));
        });
        row.col(|ui| match &c.pagamento {
            Some(p) => {
                ui.add(etiqueta_do_selo(p.selo));
            }
            None => {
                ui.add(Rotulo::campo("—"));
            }
        });
        row.col(|ui| {
            ui.add(Rotulo::campo(c.acao.clone().unwrap_or_default()).quebravel());
        });
    });
    if let Some(i) = resposta.linha_clicada {
        aberto = Some(curva[i].cliente);
    }
    aberto
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::testes_comum::motor_de_teste;

    #[test]
    fn a_curva_carrega_pelo_motor_e_a_tela_desenha_com_a_ficha_e_o_selo() {
        let t = motor_de_teste();
        let criado: PessoaCadastrada = t
            .motor
            .executar(
                &t.sessao,
                "clientes.criar_pessoa.v1",
                &CriarPessoa {
                    tipo: TipoPessoa::Fisica,
                    nome: "Maria Souza".to_owned(),
                    nome_fantasia: None,
                    papel_inicial: Papel::Cliente,
                    documento_tipo: None,
                    documento_numero: None,
                    data_nascimento: None,
                    endereco: None,
                    contato: None,
                },
            )
            .expect("cliente");
        // Sem vendas ainda: a curva vem vazia, mas vem (o módulo e a permissão estão lá).
        assert_eq!(carregar(&t.motor, &t.sessao), Ok(Vec::new()));
        // A ficha traz o pagamento: sem histórico.
        let ficha = super::super::ficha::Ficha::carregar(&t.motor, &t.sessao, criado.pessoa);
        assert_eq!(
            ficha.pagamento.map(|p| p.selo),
            Some(SeloDePagamento::SemHistorico)
        );
        let mut estado = EstadoTelaClientes {
            na_curva: true,
            ..Default::default()
        };
        let ctx = egui::Context::default();
        cardeal_ui::tokens::instalar_fontes(&ctx);
        cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                super::super::mostrar(ui, &t.motor, &t.sessao, &mut estado);
            });
        });
        assert!(estado.curva.is_some());
    }
}
