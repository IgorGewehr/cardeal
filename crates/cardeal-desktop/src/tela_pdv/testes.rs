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
        codigo: None,
        quantidade,
        preco,
        desconto: Percentual::ZERO,
        total: Dinheiro::de_total(quantidade, preco, Arredondamento::MeioAcima),
        cancelado: false,
        minimo: None,
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
    fn consulta_de_preco() -> Self {
        let mut b = Self::nova(0);
        b.estado.dlg = Dlg::ConsultaPreco {
            termo: String::new(),
            resultado: None,
        };
        b.quadro(vec![]);
        b
    }

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
        let _ = self.ctx.run(entrada, |ctx| match estado.dlg {
            Dlg::Pagamento(_) => dialogo_pagamento(ctx, estado),
            Dlg::ConsultaPreco { .. } => dialogo_consulta_preco(ctx, estado),
            Dlg::Cliente { .. } => dialogo_cliente(ctx, estado, true),
            _ => {}
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
fn cinco_escolhe_a_prazo_e_a_condicao_nasce_valida() {
    let mut b = Bancada::nova(30);
    b.quadro(vec![tecla(egui::Key::Num5)]);
    assert_eq!(b.p().ativa, pagamento::PRAZO);
    b.quadro(vec![]);
    let c = condicao_digitada(b.p()).expect("hoje + 30 dias, 1 parcela");
    assert_eq!((c.parcelas, c.intervalo_dias), (1, 30));
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
            codigo_barras: None,
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

#[test]
fn f10_pede_a_consulta_de_preco() {
    let e = apertar(caixa_aberto(), &[egui::Key::F10]);
    assert!(pediu(&e, |a| matches!(a, Acao::AbrirConsultaPreco)));
}

#[test]
fn enter_na_consulta_de_preco_pede_a_consulta_e_o_campo_ja_nasce_focado() {
    let mut b = Bancada::consulta_de_preco();
    // Sem clicar em nada: o que o leitor "digita" já cai no campo.
    b.quadro(vec![texto("7"), texto("8")]);
    match &b.estado.dlg {
        Dlg::ConsultaPreco { termo, .. } => assert_eq!(termo, "78"),
        _ => panic!("o diálogo fechou"),
    }
    b.quadro(vec![tecla(egui::Key::Enter)]);
    assert!(pediu(&b.estado, |a| matches!(a, Acao::ConsultarPreco)));
}

#[test]
fn enter_confirma_mesmo_com_focar_primeiro_no_mesmo_quadro() {
    // O padrão de todo diálogo de um campo: desenha, foca o primeiro, pergunta pelo Enter.
    // Regressão: refocar no quadro em que o Enter soltou o foco fazia `lost_focus()` dar
    // `false`, e o Enter nunca confirmava nada (só o clique no botão).
    let ctx = egui::Context::default();
    cardeal_ui::tokens::instalar_fontes(&ctx);
    cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
    let mut texto_campo = String::new();
    let mut confirmou = false;
    let mut quadro = |eventos: Vec<egui::Event>| {
        let entrada = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            events: eventos,
            ..Default::default()
        };
        confirmou = false;
        let _ = ctx.run(entrada, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let r = ui.add(cardeal_ui::molecules::Campo::novo(
                    "Valor",
                    &mut texto_campo,
                ));
                super::dialogos::focar_primeiro(ctx, &r);
                if r.lost_focus() && super::dialogos::enter_pressionado(ui) {
                    confirmou = true;
                }
            });
        });
        confirmou
    };
    assert!(!quadro(vec![]));
    assert!(!quadro(vec![texto("5")]));
    assert!(
        quadro(vec![tecla(egui::Key::Enter)]),
        "Enter tem que confirmar"
    );
}

fn cliente(nome: &str, documento: &str) -> mod_clientes::ItemPessoa {
    mod_clientes::ItemPessoa {
        pessoa: Id::novo(),
        nome: nome.to_owned(),
        documento: Some(documento.to_owned()),
        telefone: None,
    }
}

fn bancada_de_clientes() -> Bancada {
    let mut b = Bancada::nova(0);
    b.estado.clientes = vec![
        cliente("Maria Souza", "529.982.247-25"),
        cliente("Mário Andrade", "111.444.777-35"),
        cliente("Pedro Lima", "390.533.447-05"),
    ];
    b.estado.dlg = Dlg::Cliente {
        busca: String::new(),
        sel: 0,
        novo: None,
    };
    b.quadro(vec![]);
    b
}

fn escolhido(b: &Bancada) -> Option<Option<String>> {
    b.estado.pendentes.iter().find_map(|a| match a {
        Acao::EscolherCliente(c) => Some(c.as_ref().map(|(_, n)| n.clone())),
        _ => None,
    })
}

#[test]
fn f6_pede_a_escolha_de_cliente() {
    let e = apertar(caixa_aberto(), &[egui::Key::F6]);
    assert!(pediu(&e, |a| matches!(a, Acao::AbrirCliente)));
}

#[test]
fn buscar_pelo_nome_e_enter_escolhe_o_primeiro_que_casa() {
    let mut b = bancada_de_clientes();
    b.quadro(vec![texto("m"), texto("a")]); // "ma": Maria e Mário
    b.quadro(vec![tecla(egui::Key::Enter)]);
    assert_eq!(escolhido(&b), Some(Some("Maria Souza".to_owned())));
}

#[test]
fn seta_para_baixo_e_enter_escolhem_o_segundo() {
    let mut b = bancada_de_clientes();
    b.quadro(vec![texto("m"), texto("a")]);
    b.quadro(vec![tecla(egui::Key::ArrowDown)]);
    b.quadro(vec![tecla(egui::Key::Enter)]);
    assert_eq!(escolhido(&b), Some(Some("Mário Andrade".to_owned())));
}

#[test]
fn buscar_pelo_documento_tambem_acha() {
    let mut b = bancada_de_clientes();
    b.quadro(vec![texto("3"), texto("9"), texto("0")]); // parte do CPF do Pedro
    b.quadro(vec![tecla(egui::Key::Enter)]);
    assert_eq!(escolhido(&b), Some(Some("Pedro Lima".to_owned())));
}

#[test]
fn nenhum_resultado_nao_escolhe_ninguem() {
    let mut b = bancada_de_clientes();
    b.quadro(vec![texto("z"), texto("z"), texto("z")]);
    b.quadro(vec![tecla(egui::Key::Enter)]);
    assert_eq!(escolhido(&b), None);
}

#[test]
fn linha_abaixo_do_preco_minimo_e_sinalizada_so_se_o_estoque_sabe_o_minimo() {
    // Mínimo de R$ 80 por unidade; vende 2 a R$ 100.
    let mut l = linha("2", "100,00");
    assert!(!l.abaixo_do_minimo(), "sem mínimo conhecido, nada a avisar");
    l.minimo = Some(Dinheiro::reais(80));
    assert!(!l.abaixo_do_minimo(), "R$ 200 ≥ 2 × R$ 80");
    // 25% de desconto → R$ 150, abaixo de 2 × R$ 80 = R$ 160.
    l.com_desconto(Percentual::pontos(25));
    assert!(l.abaixo_do_minimo());
    // Item cancelado nunca avisa.
    l.cancelado = true;
    assert!(!l.abaixo_do_minimo());
}

#[test]
fn nfce_da_venda_so_quando_ligada_e_explica_quando_nao_sai() {
    let t = crate::testes_comum::motor_de_teste();
    let (m, s) = (&t.motor, &t.sessao);
    let ctx = egui::Context::default();
    let mut e = EstadoTelaPdv::default();
    e.carregar(m, s);
    assert!(!e.nfce_automatica, "desligada por padrão");
    assert_eq!(emitir_nfce(&ctx, m, s, &mut e, Id::novo()), None);

    e.nfce_automatica = true;
    let frase = emitir_nfce(&ctx, m, s, &mut e, Id::novo()).unwrap();
    assert!(frase.starts_with("NFC-e não saiu"), "{frase}");
    assert!(e.nfce_ultima.is_none());
}

#[test]
fn shift_f5_pede_o_desconto_no_total_e_f5_o_do_item() {
    let shift_f5 = egui::Event::Key {
        key: egui::Key::F5,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::SHIFT,
    };
    let ctx = egui::Context::default();
    let mut e = caixa_aberto();
    let _ = ctx.run(
        egui::RawInput {
            events: vec![shift_f5],
            ..Default::default()
        },
        |ctx| atalhos(ctx, &mut e),
    );
    assert!(pediu(&e, |a| matches!(a, Acao::AbrirDescontoTotal)));
    assert!(!pediu(&e, |a| matches!(a, Acao::AbrirDesconto)));
}

#[test]
fn o_total_da_tela_tira_o_desconto_no_total_com_a_conta_do_dominio() {
    let mut e = EstadoTelaPdv {
        carrinho: vec![linha("1", "70,00"), linha("1", "30,00")],
        ..EstadoTelaPdv::default()
    };
    e.desconto_no_total = Some(mod_pdv::DescontoNoTotal::Percentual(Percentual::pontos(10)));
    e.recalcular_total();
    assert_eq!(e.total, reais(90));
    // Cancelar um item: o percentual acompanha.
    e.carrinho[1].cancelado = true;
    e.recalcular_total();
    assert_eq!(e.total, reais(63));
}

#[test]
fn cpf_com_mascara_e_telefone_acham_o_cliente() {
    let mut maria = cliente("Maria Souza", "52998224725");
    maria.telefone = Some("(51) 99999-0000".into());
    let clientes = vec![maria, cliente("Pedro Lima", "39053344705")];
    let nomes = |busca: &str| -> Vec<String> {
        super::dialogos::clientes_que_casam(&clientes, busca)
            .into_iter()
            .map(|c| c.nome.clone())
            .collect()
    };
    assert_eq!(nomes("529.982"), vec!["Maria Souza"]);
    assert_eq!(nomes("99999-00"), vec!["Maria Souza"]);
    assert_eq!(nomes("390.533.447-05"), vec!["Pedro Lima"]);
    assert_eq!(nomes("pedro"), vec!["Pedro Lima"]);
}

#[test]
fn cadastrar_cliente_no_f6_comeca_com_o_que_foi_digitado() {
    let novo = super::dialogos::novo_a_partir_da_busca;
    assert_eq!(novo("529.982.247-25").documento, "52998224725");
    assert_eq!(novo("Fulano de Tal").nome, "Fulano de Tal");
    assert_eq!(novo("(51) 99999-0000").telefone, "51999990000");
    assert!(novo("12").nome.is_empty() && novo("12").documento.is_empty());

    // O botão abre o formulário; "Cadastrar e usar" pede o cadastro.
    let mut b = bancada_de_clientes();
    if let Dlg::Cliente { busca, novo, .. } = &mut b.estado.dlg {
        "Joana Prado".clone_into(busca);
        *novo = Some(super::dialogos::novo_a_partir_da_busca(busca));
    }
    b.quadro(vec![]);
    let Dlg::Cliente {
        novo: Some(form), ..
    } = &b.estado.dlg
    else {
        panic!("o formulário do cadastro continua aberto");
    };
    let criar = form.para_criar(mod_clientes::Papel::Cliente, None).unwrap();
    assert_eq!(criar.nome, "Joana Prado");
}

#[test]
fn desconto_no_total_vai_ao_motor_e_o_cadastro_rapido_identifica_o_cliente() {
    let t = crate::testes_comum::motor_de_teste();
    let (m, s) = (&t.motor, &t.sessao);
    let cupom = super::vendas_do_dia::testes::cupom_com_um_item(m, s);
    let ctx = egui::Context::default();
    cardeal_ui::tokens::instalar_fontes(&ctx);
    cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
    let mut e = EstadoTelaPdv {
        cupom: Some(cupom),
        carrinho: vec![linha("1", "25,00")],
        total: reais(25),
        dlg: Dlg::DescontoTotal {
            texto: "10%".into(),
        },
        ..EstadoTelaPdv::default()
    };
    let quadro = |e: &mut EstadoTelaPdv, eventos: Vec<egui::Event>| {
        let entrada = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            events: eventos,
            ..Default::default()
        };
        let _ = ctx.run(entrada, |ctx| super::desconto_total::dialogo(ctx, m, s, e));
    };
    quadro(&mut e, vec![]);
    quadro(&mut e, vec![tecla(egui::Key::Enter)]);
    assert!(matches!(e.dlg, Dlg::Fechado), "aplicado, o diálogo fecha");
    assert_eq!(e.total, Dinheiro::centavos(2250));
    e.recalcular_total();
    assert_eq!(
        e.total,
        Dinheiro::centavos(2250),
        "a conta da tela bate com a do motor"
    );

    // F6 › Cadastrar cliente › Cadastrar e usar: cadastra e identifica na venda.
    let criar = super::dialogos::novo_a_partir_da_busca("Joana Prado")
        .para_criar(mod_clientes::Papel::Cliente, None)
        .unwrap();
    cadastrar_cliente(&ctx, m, s, &mut e, criar);
    assert_eq!(
        e.cliente.as_ref().map(|c| c.1.as_str()),
        Some("Joana Prado")
    );
    assert!(e.clientes.iter().any(|c| c.nome == "Joana Prado"));
}
