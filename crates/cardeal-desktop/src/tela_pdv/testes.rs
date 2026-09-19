//! Testes da tela de PDV: estado, conta do desconto e o fluxo de teclado do pagamento.
use super::*;
use mod_pdv::ItemCupom;

fn reais(n: i64) -> Dinheiro {
    Dinheiro::reais(n)
}

/// Uma linha de `qtd` × R$ `preco`, sem desconto.
fn linha(qtd: &str, preco: &str) -> Linha {
    let quantidade: Quantidade = qtd.parse().unwrap();
    let preco: Preco = preco.parse().unwrap();
    Linha {
        item: Id::novo(),
        nome: "Produto".to_owned(),
        quantidade,
        preco,
        desconto: Percentual::ZERO,
        total: Dinheiro::de_total(quantidade, preco, Arredondamento::MeioAcima),
        cancelado: false,
    }
}

#[test]
fn desconto_espelha_a_conta_do_dominio() {
    // O carrinho da tela tem que bater, ao centavo, com o `ItemCupom` que o backend
    // calcula — inclusive nos arredondamentos.
    for (qtd, preco, pct) in [
        ("2", "10,00", 10),
        ("0,450", "18,90", 5),
        ("3", "9,99", 7),
        ("1", "0,33", 15),
    ] {
        let mut l = linha(qtd, preco);
        l.com_desconto(Percentual::pontos(pct));

        let mut item = ItemCupom::novo(
            Id::novo(),
            None,
            qtd.parse().unwrap(),
            preco.parse().unwrap(),
        )
        .unwrap();
        item.aplicar_desconto(Percentual::pontos(pct), Percentual::CEM)
            .unwrap();
        assert_eq!(l.total, item.total_item, "{qtd} × {preco} com {pct}%");
    }
}

#[test]
fn total_ignora_itens_cancelados() {
    let mut e = EstadoTelaPdv::default();
    e.carrinho.push(linha("1", "10,00"));
    e.carrinho.push(linha("2", "5,00"));
    e.recalcular_total();
    assert_eq!(e.total, reais(20));
    e.carrinho[0].cancelado = true;
    e.recalcular_total();
    assert_eq!(e.total, reais(10));
}

#[test]
fn setas_percorrem_so_os_itens_ativos() {
    let mut e = EstadoTelaPdv::default();
    for _ in 0..4 {
        e.carrinho.push(linha("1", "1,00"));
    }
    e.carrinho[1].cancelado = true;
    assert_eq!(e.linha_sel, None);

    e.mover_linha(false); // sem seleção, ↓ vai ao primeiro
    assert_eq!(e.linha_sel, Some(0));
    e.mover_linha(false); // pula o cancelado (1)
    assert_eq!(e.linha_sel, Some(2));
    e.mover_linha(false);
    e.mover_linha(false); // no fim, fica
    assert_eq!(e.linha_sel, Some(3));
    e.mover_linha(true);
    assert_eq!(e.linha_sel, Some(2));
}

#[test]
fn linha_selecionada_cancelada_nao_e_alvo_de_acao() {
    let mut e = EstadoTelaPdv::default();
    e.carrinho.push(linha("1", "1,00"));
    e.linha_sel = Some(0);
    assert_eq!(e.linha_ativa_sel(), Some(0));
    e.carrinho[0].cancelado = true;
    assert_eq!(e.linha_ativa_sel(), None);
}

// ── fluxo de teclado do pagamento, com eventos reais do egui ──────────────

fn tecla(k: egui::Key) -> egui::Event {
    egui::Event::Key {
        key: k,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

fn texto(t: &str) -> egui::Event {
    egui::Event::Text(t.to_owned())
}

/// Um contexto egui de verdade (fontes e tema do design system) e o quadro que o PDV
/// desenharia com o diálogo de pagamento aberto.
struct Bancada {
    ctx: egui::Context,
    estado: EstadoTelaPdv,
    t: f64,
}

impl Bancada {
    fn nova(total: i64) -> Self {
        let ctx = egui::Context::default();
        cardeal_ui::tokens::instalar_fontes(&ctx);
        cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
        let estado = EstadoTelaPdv {
            cupom_numero: Some(1),
            total: reais(total),
            dlg: Dlg::Pagamento(EstadoPagamento::nova()),
            ..EstadoTelaPdv::default()
        };
        let mut b = Self {
            ctx,
            estado,
            t: 0.0,
        };
        b.quadro(vec![]); // assenta o layout e registra o id do campo de valor
        b
    }

    fn quadro(&mut self, eventos: Vec<egui::Event>) {
        self.t += 0.05;
        let entrada = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            time: Some(self.t),
            events: eventos,
            ..Default::default()
        };
        let estado = &mut self.estado;
        let _ = self.ctx.run(entrada, |ctx| {
            if matches!(estado.dlg, Dlg::Pagamento(_)) {
                dialogo_pagamento(ctx, estado);
            }
        });
    }

    fn p(&self) -> &EstadoPagamento {
        match &self.estado.dlg {
            Dlg::Pagamento(p) => p,
            _ => panic!("o diálogo de pagamento fechou"),
        }
    }

    fn quer_finalizar(&self) -> bool {
        self.estado
            .pendentes
            .iter()
            .any(|a| matches!(a, Acao::Finalizar))
    }
}

#[test]
fn dinheiro_exato_e_f2_enter_enter() {
    // O caminho mais comum do balcão: tudo em dinheiro, valor exato.
    let mut b = Bancada::nova(30);
    b.quadro(vec![tecla(egui::Key::Num1), texto("1")]); // escolhe Dinheiro
    b.quadro(vec![]);
    assert_eq!(b.p().ativa, 0);
    assert_eq!(
        b.p().digitado,
        "",
        "o dígito da tecla-atalho não pode vazar para o campo"
    );

    b.quadro(vec![tecla(egui::Key::Enter)]); // Enter vazio = "o que falta"
    assert_eq!(b.p().valores[0], reais(30));
    assert!(!b.quer_finalizar(), "só confirmou o valor, não finalizou");

    b.quadro(vec![]);
    b.quadro(vec![tecla(egui::Key::Enter)]); // já fechou: Enter finaliza
    assert!(b.quer_finalizar());
}

#[test]
fn digitar_um_valor_maior_gera_troco() {
    let mut b = Bancada::nova(30);
    b.quadro(vec![tecla(egui::Key::Num1), texto("1")]);
    b.quadro(vec![]);
    b.quadro(vec![texto("5"), texto("0")]);
    assert_eq!(b.p().digitado, "50");
    b.quadro(vec![tecla(egui::Key::Enter)]);
    assert_eq!(b.p().valores[0], reais(50));
    assert_eq!(
        pagamento::situacao(reais(30), &b.p().valores),
        Situacao::Troco(reais(20))
    );
}

#[test]
fn digito_e_valor_enquanto_digita_nao_escolhe_forma() {
    // Ao digitar "20", o "2" é o valor — não a tecla que escolhe o Pix.
    let mut b = Bancada::nova(30);
    b.quadro(vec![tecla(egui::Key::Num1), texto("1")]);
    b.quadro(vec![]);
    b.quadro(vec![tecla(egui::Key::Num2), texto("2")]);
    assert_eq!(b.p().ativa, 0, "continua em Dinheiro");
    assert_eq!(b.p().digitado, "2");
}

#[test]
fn esc_enquanto_digita_volta_a_escolher_sem_fechar() {
    let mut b = Bancada::nova(30);
    b.quadro(vec![tecla(egui::Key::Num1), texto("1")]);
    b.quadro(vec![]);
    b.quadro(vec![texto("9")]);
    b.quadro(vec![tecla(egui::Key::Escape)]);
    assert!(
        matches!(b.estado.dlg, Dlg::Pagamento(_)),
        "Esc não fecha o diálogo"
    );
    assert_eq!(
        b.p().digitado,
        "",
        "o que estava sendo digitado é descartado"
    );
    b.quadro(vec![]);
    // Agora, sem campo com foco, Esc fecha.
    b.quadro(vec![tecla(egui::Key::Escape)]);
    assert!(matches!(b.estado.dlg, Dlg::Fechado));
}

#[test]
fn duas_formas_somam_e_fecham() {
    // R$ 20 em dinheiro + o resto no Pix.
    let mut b = Bancada::nova(30);
    b.quadro(vec![tecla(egui::Key::Num1), texto("1")]);
    b.quadro(vec![]);
    b.quadro(vec![texto("2"), texto("0")]);
    b.quadro(vec![tecla(egui::Key::Enter)]);
    b.quadro(vec![]);
    b.quadro(vec![tecla(egui::Key::Num2), texto("2")]); // Pix
    b.quadro(vec![]);
    b.quadro(vec![tecla(egui::Key::Enter)]); // vazio = o que falta (10)
    assert_eq!(b.p().valores[0], reais(20));
    assert_eq!(b.p().valores[1], reais(10));
    assert_eq!(
        pagamento::situacao(reais(30), &b.p().valores),
        Situacao::Fecha
    );
}

#[test]
fn f2_sem_ter_fechado_ainda_pede_finalizar_para_a_validacao_avisar() {
    // `F2` nunca "some": quem decide se pode é `pedir_finalizar`, que avisa o que falta.
    let mut b = Bancada::nova(30);
    b.quadro(vec![tecla(egui::Key::F2)]);
    assert!(b.quer_finalizar());
}

#[test]
fn busca_por_nome_ignora_acento_e_caixa_e_memoriza() {
    let produtos = ["Açúcar Cristal 1kg", "Arroz 5kg", "Refrigerante Cola 2L"]
        .iter()
        .map(|n| ItemProdutoComSaldo {
            produto: Id::novo(),
            nome: (*n).to_owned(),
            ncm: String::new(),
            disponivel: Quantidade::UM,
            reservado: Quantidade::ZERO,
            custo_medio: "1,00".parse().unwrap(),
        })
        .collect();
    let mut e = EstadoTelaPdv {
        produtos,
        ..EstadoTelaPdv::default()
    };

    e.entrada = "acucar cristal".to_owned();
    e.atualizar_resultados();
    assert_eq!(e.resultados.1, vec![0]);

    e.entrada = "kg".to_owned();
    e.atualizar_resultados();
    assert_eq!(e.resultados.1, vec![0, 1]);

    // Um código de barras não é busca por nome.
    e.entrada = "7891234567895".to_owned();
    e.atualizar_resultados();
    assert!(e.resultados.1.is_empty());

    // Um caractere só é pouco para buscar.
    e.entrada = "a".to_owned();
    e.atualizar_resultados();
    assert!(e.resultados.1.is_empty());
}

// ── teclas da tela de venda ───────────────────────────────────────────────────

/// Roda `atalhos` num quadro com as teclas dadas e devolve o estado resultante.
fn apertar(mut estado: EstadoTelaPdv, teclas: &[egui::Key]) -> EstadoTelaPdv {
    let ctx = egui::Context::default();
    let entrada = egui::RawInput {
        events: teclas.iter().map(|k| tecla(*k)).collect(),
        ..Default::default()
    };
    let _ = ctx.run(entrada, |ctx| atalhos(ctx, &mut estado));
    estado
}

fn caixa_aberto() -> EstadoTelaPdv {
    let caixa = Id::novo();
    EstadoTelaPdv {
        caixas: vec![mod_financeiro::ItemCaixa {
            caixa,
            nome: "Caixa 1".to_owned(),
            conta_razao: Id::novo(),
            sessao_aberta: Some(Id::novo()),
        }],
        caixa_sel: Some(caixa),
        ..EstadoTelaPdv::default()
    }
}

fn pediu(e: &EstadoTelaPdv, f: impl Fn(&Acao) -> bool) -> bool {
    e.pendentes.iter().any(f)
}

#[test]
fn f9_pede_sangria_e_f12_pede_fechamento() {
    let e = apertar(caixa_aberto(), &[egui::Key::F9]);
    assert!(pediu(&e, |a| matches!(a, Acao::AbrirSangria)));
    let e = apertar(caixa_aberto(), &[egui::Key::F12]);
    assert!(pediu(&e, |a| matches!(a, Acao::AbrirFechamento)));
}

#[test]
fn f12_com_o_caixa_fechado_abre_o_caixa() {
    let mut e = caixa_aberto();
    e.caixas[0].sessao_aberta = None;
    let e = apertar(e, &[egui::Key::F12]);
    assert!(matches!(e.dlg, Dlg::AbrirCaixa { .. }));
    assert!(!pediu(&e, |a| matches!(a, Acao::AbrirFechamento)));
}

#[test]
fn com_dialogo_aberto_as_teclas_da_venda_ficam_mudas() {
    let mut e = caixa_aberto();
    e.dlg = Dlg::Sangria {
        valor: String::new(),
        motivo: String::new(),
    };
    let e = apertar(e, &[egui::Key::F2, egui::Key::F9, egui::Key::F12]);
    assert!(
        e.pendentes.is_empty(),
        "F2/F9/F12 não podem agir por baixo de um diálogo"
    );
}

#[test]
fn as_teclas_de_venda_geram_as_acoes_certas() {
    let e = apertar(
        caixa_aberto(),
        &[egui::Key::F2, egui::Key::F5, egui::Key::F7, egui::Key::F8],
    );
    assert!(pediu(&e, |a| matches!(a, Acao::AbrirPagamento)));
    assert!(pediu(&e, |a| matches!(a, Acao::AbrirDesconto)));
    assert!(pediu(&e, |a| matches!(a, Acao::CancelarLinha)));
    assert!(pediu(&e, |a| matches!(a, Acao::PedirCancelarCupom)));
}

// ── foco em diálogos com vários campos ────────────────────────────────────────

#[test]
fn focar_primeiro_nao_rouba_o_foco_do_segundo_campo() {
    // Regressão: "pede foco se este não tem" devolvia o foco ao primeiro campo a cada quadro,
    // e o usuário nunca conseguia digitar no segundo.
    let ctx = egui::Context::default();
    cardeal_ui::tokens::instalar_fontes(&ctx);
    cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
    let (mut a, mut b) = (String::new(), String::new());
    let quadro = |eventos: Vec<egui::Event>, a: &mut String, b: &mut String| {
        let entrada = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            events: eventos,
            ..Default::default()
        };
        let _ = ctx.run(entrada, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let r1 = ui.add(cardeal_ui::molecules::Campo::novo("Primeiro", a));
                super::dialogos::focar_primeiro(ctx, &r1);
                ui.add(cardeal_ui::molecules::Campo::novo("Segundo", b));
            });
        });
    };
    quadro(vec![], &mut a, &mut b); // o primeiro ganha o foco
    quadro(vec![texto("x")], &mut a, &mut b);
    assert_eq!(a, "x", "o primeiro campo começa focado");
    quadro(vec![tecla(egui::Key::Tab)], &mut a, &mut b); // vai para o segundo
    quadro(vec![texto("y")], &mut a, &mut b);
    assert_eq!(b, "y", "o segundo campo tem que aceitar digitação");
    assert_eq!(a, "x");
}
