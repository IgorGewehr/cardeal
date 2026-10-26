//! Testes ponta a ponta da tela de Financeiro: motor real (SQLite temporário), admin logado.

use super::*;
use crate::testes_comum::motor_de_teste;
use mod_clientes::{CriarPessoa, TipoPessoa};

fn lancar_a_receber(motor: &Motor, sessao: &Sessao, reais: i64) -> Id {
    let hoje = Data::hoje(Fuso::BRASILIA);
    let r: mod_financeiro::TituloAReceberLancado = motor
        .executar(
            sessao,
            "financeiro.lancar_titulo_a_receber.v1",
            &LancarTituloAReceber {
                cliente: None,
                valor_total: Dinheiro::reais(reais),
                emissao: hoje,
                parcelas: 1,
                primeiro_vencimento: hoje,
                intervalo_dias: 0,
                observacao: None,
                categoria: None,
                quitado_agora: None,
            },
        )
        .expect("título");
    r.parcelas[0]
}

#[test]
fn baixa_vai_para_a_parcela_aberta_mesmo_se_a_lista_mudar_de_ordem() {
    // O diálogo guardava a *posição* da parcela no vetor; reordenar a grade com ele
    // aberto fazia a baixa cair na parcela errada.
    let t = motor_de_teste();
    let _a = lancar_a_receber(&t.motor, &t.sessao, 100);
    let b = lancar_a_receber(&t.motor, &t.sessao, 250);
    let mut estado = EstadoTelaFinanceiro {
        aba: Aba::Receber,
        ..Default::default()
    };
    estado.carregar(&t.motor, &t.sessao);
    assert!(estado.erro.is_none(), "{:?}", estado.erro);
    assert_eq!(estado.parcelas.len(), 2);

    let p = estado
        .parcelas
        .iter()
        .find(|p| p.parcela == b)
        .cloned()
        .expect("parcela b");
    estado.dlg = Dlg::Baixar {
        parcela: b,
        valor: p.saldo().formatar(),
        data: Data::hoje(Fuso::BRASILIA).to_string(),
        pagamento: crate::pagamento::EstadoPagamento::novo(MeioPagamento::Dinheiro),
        baixas: Vec::new(),
        baixas_carregadas: true,
        motivo_estorno: String::new(),
        situacao: None,
        valor_sugerido: String::new(),
        renegociar: FormRenegociar::default(),
        ajuste: FormAjuste::default(),
    };
    estado.parcelas.reverse();

    // O diálogo continua na parcela B, e a baixa sai nela.
    let p = parcela_do_dialogo(&estado).expect("parcela do diálogo");
    assert_eq!(p.parcela, b);
    baixar(
        &egui::Context::default(),
        &t.motor,
        &t.sessao,
        &mut estado,
        &p,
    );

    let abertas: Vec<ItemTituloEmAberto> = t
        .motor
        .consultar(
            &t.sessao,
            "financeiro.titulos_a_receber_em_aberto.v1",
            &TitulosAReceberEmAberto,
        )
        .expect("em aberto");
    assert_eq!(abertas.len(), 1);
    assert_eq!(abertas[0].valor_original, Dinheiro::reais(100));
}

#[test]
fn extrato_rotula_o_recebimento_da_os_com_numero_e_cliente() {
    use mod_os::{
        AbrirOrdemServico, FaturarOrdemServico, ItemOrcamentoNovo, MontarOrcamentoOs,
        OrdemServicoAberta, OrdemServicoFaturada, PagamentoNoAto,
    };
    let t = motor_de_teste();
    let cliente: mod_clientes::PessoaCadastrada = t
        .motor
        .executar(
            &t.sessao,
            "clientes.criar_pessoa.v1",
            &CriarPessoa {
                tipo: TipoPessoa::Fisica,
                nome: "Ana Lima".to_owned(),
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
    let os: OrdemServicoAberta = t
        .motor
        .executar(
            &t.sessao,
            "os.abrir_ordem_servico.v1",
            &AbrirOrdemServico {
                cliente: cliente.pessoa,
                equipamento: "Notebook".to_owned(),
                defeito_relatado: "Não liga".to_owned(),
                tecnico_responsavel: t.sessao.usuario(),
                garantia_dias: 90,
                ficha: mod_os::FichaEntrada::default(),
            },
        )
        .expect("OS");
    let _: Id = t
        .motor
        .executar(
            &t.sessao,
            "os.montar_orcamento.v1",
            &MontarOrcamentoOs {
                ordem_servico: os.ordem_servico,
                item: ItemOrcamentoNovo::MaoDeObra {
                    descricao: "Reparo".to_owned(),
                    valor: Dinheiro::reais(150),
                    tecnico: t.sessao.usuario(),
                    horas: None,
                },
            },
        )
        .expect("mão de obra");
    let _: OrdemServicoFaturada = t
        .motor
        .executar(
            &t.sessao,
            "os.faturar_ordem_servico.v1",
            &FaturarOrdemServico {
                ordem_servico: os.ordem_servico,
                parcelas: 1,
                primeiro_vencimento: Data::hoje(Fuso::BRASILIA),
                intervalo_dias: 0,
                pago_no_ato: Some(PagamentoNoAto {
                    meio_pagamento: MeioPagamento::Dinheiro,
                    conta_destino: None,
                }),
            },
        )
        .expect("faturar");

    let mut estado = EstadoTelaFinanceiro {
        aba: Aba::Fluxo,
        ..Default::default()
    };
    estado.carregar(&t.motor, &t.sessao);
    assert!(estado.erro.is_none(), "{:?}", estado.erro);
    let esperado = format!("OS #{} — Ana Lima", os.numero);
    assert!(
        estado.origem_labels.values().any(|r| *r == esperado),
        "rótulos: {:?}",
        estado.origem_labels
    );

    // A mesma origem na coluna das parcelas a receber, e a busca acha pelo número da OS.
    estado.aba = Aba::Receber;
    estado.carregar(&t.motor, &t.sessao);
    let p = estado
        .parcelas
        .iter()
        .find(|p| p.origem_modulo == "os")
        .expect("parcela da OS");
    assert_eq!(estado.origem_da_parcela(p).0, esperado);
    // A baixa dessa parcela mostra o que a OS rendeu (só mão de obra: margem cheia).
    let p = p.clone();
    let m = estado
        .margem_da_os(&t.motor, &t.sessao, &p)
        .expect("margem da OS");
    assert_eq!(m.numero, os.numero);
    assert_eq!(m.margem(), Dinheiro::reais(150));
    assert_eq!(m.percentual(), Some(100));
    estado.busca = format!("os #{}", os.numero);
    let hoje = Data::hoje(Fuso::BRASILIA);
    assert_eq!(
        estado.parcelas_filtradas(FiltroParcelas::Todas, hoje).len(),
        1
    );
}

#[test]
fn lancar_pago_agora_com_fornecedor_novo_cadastra_quita_e_limpa_para_o_proximo() {
    let t = motor_de_teste();
    let mut estado = EstadoTelaFinanceiro {
        aba: Aba::Pagar,
        ..Default::default()
    };
    estado.carregar(&t.motor, &t.sessao);
    let fornecedores_antes = estado.fornecedores.len();

    let mut f = FormLancar::novo(false);
    f.pessoa.nenhuma = false;
    f.pessoa.novo = true;
    f.pessoa.nome = "Distribuidora Peças BH".to_owned();
    f.pessoa.telefone = "31 3333-4444".to_owned();
    f.descricao = "Tela de reposição".to_owned();
    f.valor = "180,00".to_owned();
    f.pagamento = crate::pagamento::EstadoPagamento::novo(MeioPagamento::Dinheiro);
    estado.dlg = Dlg::Lancar(f);
    lancar(
        &egui::Context::default(),
        &t.motor,
        &t.sessao,
        &mut estado,
        true,
    );

    assert_eq!(estado.fornecedores.len(), fornecedores_antes + 1);
    let p = estado
        .parcelas
        .iter()
        .find(|p| p.descricao.as_deref() == Some("Tela de reposição"))
        .expect("parcela lançada");
    assert_eq!(p.estado, EstadoParcela::Quitada);
    assert_eq!(
        p.valor_baixado,
        "180,00".parse::<Dinheiro>().expect("valor")
    );
    assert_eq!(estado.origem_da_parcela(p).0, "Manual");
    assert_eq!(
        estado.nome_contraparte(&p.contraparte),
        "Distribuidora Peças BH"
    );
    // "Lançar e continuar": formulário limpo, ainda a pagar e com o mesmo meio.
    let Dlg::Lancar(f) = &estado.dlg else {
        panic!("o diálogo devia continuar aberto");
    };
    assert!(!f.a_receber);
    assert!(f.valor.is_empty() && f.descricao.is_empty() && !f.pessoa.novo);
    assert_eq!(f.pagamento.condicao.meio, Some(MeioPagamento::Dinheiro));
}

#[test]
fn lancar_a_prazo_fica_em_aberto_nas_parcelas_pedidas() {
    let t = motor_de_teste();
    let mut estado = EstadoTelaFinanceiro {
        aba: Aba::Receber,
        ..Default::default()
    };
    estado.periodo.preset = PresetPeriodo::Ano;
    estado.carregar(&t.motor, &t.sessao);
    let mut f = FormLancar::novo(true);
    f.descricao = "Conserto parcelado".to_owned();
    f.valor = "300,00".to_owned();
    f.pagamento.condicao.a_prazo = true;
    f.pagamento.condicao.parcelas = "3".to_owned();
    f.pagamento.condicao.primeiro_vencimento = Data::hoje(Fuso::BRASILIA).to_string();
    f.pagamento.condicao.intervalo_dias = "1".to_owned();
    estado.dlg = Dlg::Lancar(f);
    lancar(
        &egui::Context::default(),
        &t.motor,
        &t.sessao,
        &mut estado,
        false,
    );

    assert!(matches!(estado.dlg, Dlg::Fechado));
    let lancadas: Vec<_> = estado
        .parcelas
        .iter()
        .filter(|p| p.descricao.as_deref() == Some("Conserto parcelado"))
        .collect();
    assert_eq!(lancadas.len(), 3);
    assert!(lancadas.iter().all(|p| p.estado == EstadoParcela::Aberta));
}

#[test]
fn selecionar_e_baixar_varias_quita_so_as_marcadas() {
    let t = motor_de_teste();
    let a = lancar_a_receber(&t.motor, &t.sessao, 100);
    let b = lancar_a_receber(&t.motor, &t.sessao, 250);
    let _c = lancar_a_receber(&t.motor, &t.sessao, 75);
    let mut estado = EstadoTelaFinanceiro {
        aba: Aba::Receber,
        ..Default::default()
    };
    estado.carregar(&t.motor, &t.sessao);
    estado.selecao = Some(vec![a, b]);
    estado.dlg = Dlg::BaixarLote {
        data: Data::hoje(Fuso::BRASILIA).to_string(),
        pagamento: crate::pagamento::EstadoPagamento::novo(MeioPagamento::Dinheiro),
    };
    let selecionadas: Vec<ItemTituloEmAberto> = estado
        .parcelas
        .iter()
        .filter(|p| p.parcela == a || p.parcela == b)
        .cloned()
        .collect();
    baixar_lote(
        &egui::Context::default(),
        &t.motor,
        &t.sessao,
        &mut estado,
        &selecionadas,
    );

    assert!(estado.selecao.is_none(), "a seleção fecha depois de baixar");
    let abertas: Vec<ItemTituloEmAberto> = t
        .motor
        .consultar(
            &t.sessao,
            "financeiro.titulos_a_receber_em_aberto.v1",
            &TitulosAReceberEmAberto,
        )
        .expect("em aberto");
    assert_eq!(abertas.len(), 1);
    assert_eq!(abertas[0].valor_original, Dinheiro::reais(75));
}

#[test]
fn baixa_de_parcela_vencida_sugere_o_total_com_multa() {
    let t = motor_de_teste();
    let hoje = Data::hoje(Fuso::BRASILIA);
    let lancado: mod_financeiro::TituloAReceberLancado = t
        .motor
        .executar(
            &t.sessao,
            "financeiro.lancar_titulo_a_receber.v1",
            &LancarTituloAReceber {
                cliente: None,
                valor_total: Dinheiro::reais(100),
                emissao: hoje.mais_dias(-30),
                parcelas: 1,
                primeiro_vencimento: hoje.mais_dias(-30),
                intervalo_dias: 0,
                observacao: None,
                categoria: None,
                quitado_agora: None,
            },
        )
        .expect("título");
    let reneg: mod_financeiro::TituloFoiRenegociado = t
        .motor
        .executar(
            &t.sessao,
            "financeiro.renegociar_titulo.v1",
            &mod_financeiro::RenegociarTitulo {
                titulo: lancado.titulo,
                numero_parcelas: 1,
                primeiro_vencimento: hoje.mais_dias(-10),
                intervalo_dias: 0,
                politica_juros: mod_financeiro::PoliticaJuros::Nenhum,
                taxa_juros: None,
                multa: Some(cardeal_kernel::Percentual::pontos(2)),
            },
        )
        .expect("renegociar");
    // Período explícito: o padrão ("Mês") deixa de fora o vencimento de 10 dias atrás nos
    // primeiros dias do mês.
    let mut estado = EstadoTelaFinanceiro {
        aba: Aba::Receber,
        periodo: FiltroPeriodo {
            preset: PresetPeriodo::Personalizado,
            de_personalizado: hoje.mais_dias(-60).to_string(),
            ate_personalizado: hoje.to_string(),
        },
        ..Default::default()
    };
    estado.carregar(&t.motor, &t.sessao);
    let p = estado
        .parcelas
        .iter()
        .find(|p| p.titulo == reneg.titulo_novo)
        .cloned()
        .expect("parcela renegociada");
    estado.dlg = Dlg::Baixar {
        parcela: p.parcela,
        valor: p.saldo().formatar(),
        data: hoje.to_string(),
        pagamento: crate::pagamento::EstadoPagamento::novo(MeioPagamento::Dinheiro),
        baixas: Vec::new(),
        baixas_carregadas: true,
        motivo_estorno: String::new(),
        situacao: None,
        valor_sugerido: String::new(),
        renegociar: FormRenegociar::default(),
        ajuste: FormAjuste::default(),
    };
    atualizar_situacao(&t.motor, &t.sessao, &mut estado, p.parcela);
    let Dlg::Baixar {
        valor, situacao, ..
    } = &estado.dlg
    else {
        panic!("diálogo fechou");
    };
    assert_eq!(valor, "102,00");
    let s = situacao.as_ref().and_then(|(_, s)| *s).expect("situação");
    assert_eq!(
        composicao_do_devido(&s).as_deref(),
        Some("R$ 100,00 + multa R$ 2,00 = R$ 102,00 (10 dias de atraso)")
    );
}

#[test]
fn renegociar_pela_tela_troca_o_saldo_por_parcelas_novas() {
    let t = motor_de_teste();
    let parcela = lancar_a_receber(&t.motor, &t.sessao, 300);
    let mut estado = EstadoTelaFinanceiro {
        aba: Aba::Receber,
        ..Default::default()
    };
    estado.carregar(&t.motor, &t.sessao);
    let titulo = estado
        .parcelas
        .iter()
        .find(|p| p.parcela == parcela)
        .expect("parcela")
        .titulo;

    let mut form = FormRenegociar {
        parcelas: "3".to_owned(),
        primeiro_vencimento: Data::hoje(Fuso::BRASILIA).mais_dias(30).to_string(),
        intervalo_dias: "30".to_owned(),
        multa: "2%".to_owned(),
        confirmar: false,
    };
    let comando = form.validado(titulo).expect("formulário válido");
    assert_eq!(comando.numero_parcelas, 3);
    assert_eq!(comando.multa, Some(cardeal_kernel::Percentual::pontos(2)));
    let _: mod_financeiro::TituloFoiRenegociado = t
        .motor
        .executar(&t.sessao, "financeiro.renegociar_titulo.v1", &comando)
        .expect("renegociar");
    let abertas: Vec<ItemTituloEmAberto> = t
        .motor
        .consultar(
            &t.sessao,
            "financeiro.titulos_a_receber_em_aberto.v1",
            &TitulosAReceberEmAberto,
        )
        .expect("em aberto");
    assert_eq!(abertas.len(), 3);
    assert!(abertas.iter().all(|p| p.titulo != titulo));

    form.parcelas = "0".to_owned();
    assert!(form.validado(titulo).is_err());
}

#[test]
fn visao_geral_mostra_a_margem_das_os_faturadas_no_mes() {
    let t = motor_de_teste();
    let mut estado = EstadoTelaFinanceiro::default();
    estado.carregar(&t.motor, &t.sessao);
    let m = estado.dash_margem_os.expect("admin vê OS");
    assert_eq!(m.ordens, 0);
    assert!(m.margem().e_zero());
}

#[test]
fn custos_por_categoria_mostram_o_mes_e_levam_a_lista_filtrada() {
    let t = motor_de_teste();
    let _: u32 = t
        .motor
        .executar(
            &t.sessao,
            "financeiro.criar_categorias_sugeridas.v1",
            &mod_financeiro::CriarCategoriasSugeridas,
        )
        .expect("sugeridas");
    let mut estado = EstadoTelaFinanceiro {
        aba: Aba::Custos,
        ..Default::default()
    };
    estado.carregar(&t.motor, &t.sessao);
    let aluguel = estado
        .categorias
        .iter()
        .find(|c| c.nome == "Aluguel")
        .map(|c| c.id)
        .expect("aluguel");
    let mut f = FormLancar::novo(false);
    f.descricao = "Aluguel da loja".to_owned();
    f.valor = "1500,00".to_owned();
    f.categoria = Some(aluguel);
    f.pagamento.condicao.a_prazo = true;
    f.pagamento.condicao.parcelas = "1".to_owned();
    f.pagamento.condicao.primeiro_vencimento = Data::hoje(Fuso::BRASILIA).to_string();
    estado.dlg = Dlg::Lancar(f);
    lancar(
        &egui::Context::default(),
        &t.motor,
        &t.sessao,
        &mut estado,
        false,
    );

    assert!(estado.erro.is_none(), "{:?}", estado.erro);
    let linhas = estado.linhas_custos();
    assert_eq!(linhas[0].nome, "Aluguel");
    assert_eq!(linhas[0].por_mes[0], Dinheiro::reais(1500));

    // Clicar na categoria leva à lista "A pagar" só com ela.
    estado.filtro_categoria = Some(FiltroCategoria::Uma(aluguel));
    estado.aba = Aba::Pagar;
    estado.carregar(&t.motor, &t.sessao);
    let hoje = Data::hoje(Fuso::BRASILIA);
    assert_eq!(
        estado.parcelas_filtradas(FiltroParcelas::Todas, hoje).len(),
        1
    );
    estado.filtro_categoria = Some(FiltroCategoria::Sem);
    let sobra: Vec<_> = estado
        .parcelas_filtradas(FiltroParcelas::Todas, hoje)
        .into_iter()
        .map(|i| {
            (
                estado.parcelas[i].descricao.clone(),
                estado.parcelas[i].categoria,
            )
        })
        .collect();
    assert!(sobra.is_empty(), "{sobra:?}");
}

#[test]
fn lancar_sem_pessoa_ignora_quem_estava_escolhido() {
    let t = motor_de_teste();
    let mut estado = EstadoTelaFinanceiro {
        aba: Aba::Pagar,
        ..Default::default()
    };
    estado.carregar(&t.motor, &t.sessao);
    let fornecedores_antes = estado.fornecedores.len();

    // Começa em "Sem favorecido"; mesmo com um nome digitado no "Novo" (e o usuário tendo
    // voltado para "Sem"), nada é cadastrado e o título sai sem contraparte.
    let mut f = FormLancar::novo(false);
    assert!(f.pessoa.nenhuma);
    f.pessoa.novo = true;
    f.pessoa.nome = "Não deveria ser cadastrado".to_owned();
    f.descricao = "Conta de luz".to_owned();
    f.valor = "95,40".to_owned();
    f.pagamento = crate::pagamento::EstadoPagamento::novo(MeioPagamento::Dinheiro);
    estado.dlg = Dlg::Lancar(f);
    lancar(
        &egui::Context::default(),
        &t.motor,
        &t.sessao,
        &mut estado,
        false,
    );

    assert_eq!(estado.fornecedores.len(), fornecedores_antes);
    let p = estado
        .parcelas
        .iter()
        .find(|p| p.descricao.as_deref() == Some("Conta de luz"))
        .expect("parcela lançada");
    assert!(p.contraparte.is_none());
    assert_eq!(p.estado, EstadoParcela::Quitada);
}

#[test]
fn lancar_a_vista_sem_marcar_pago_fica_em_aberto_vencendo_na_data() {
    let t = motor_de_teste();
    let mut estado = EstadoTelaFinanceiro {
        aba: Aba::Pagar,
        ..Default::default()
    };
    estado.carregar(&t.motor, &t.sessao);

    let mut f = FormLancar::novo(false);
    assert!(f.pagamento.condicao.pago, "à vista começa como já pago");
    f.pagamento.condicao.pago = false;
    f.descricao = "Internet de outubro".to_owned();
    f.valor = "120,00".to_owned();
    let data = f.data.parse::<Data>().expect("data padrão");
    estado.dlg = Dlg::Lancar(f);
    lancar(
        &egui::Context::default(),
        &t.motor,
        &t.sessao,
        &mut estado,
        false,
    );

    let p = estado
        .parcelas
        .iter()
        .find(|p| p.descricao.as_deref() == Some("Internet de outubro"))
        .expect("parcela lançada");
    assert_eq!(p.estado, EstadoParcela::Aberta);
    assert_eq!(p.vencimento, data);
    assert!(p.valor_baixado.e_zero());
}

#[test]
fn categorias_empatadas_no_grafico_nao_trocam_de_lugar_a_cada_quadro() {
    // `top_categorias` roda a cada quadro sobre um `HashMap` (ordem aleatória a cada mapa
    // novo): sem desempate, duas categorias com o mesmo valor pulavam de posição sem parar.
    let cat = |nome: &str| mod_financeiro::CategoriaFinanceira {
        id: Id::novo(),
        empresa: Id::novo(),
        nome: nome.to_owned(),
        especie: None,
        ativa: true,
    };
    let (a, b, c) = (cat("Aluguel"), cat("Energia"), cat("Peças"));
    let item = |categoria: Id, reais: i64| mod_financeiro::ItemTotalPorCategoria {
        categoria: Some(categoria),
        especie: EspecieTitulo::Pagar,
        competencia: cardeal_kernel::Competencia::nova(2026, 9),
        total_baixado: Dinheiro::reais(reais),
    };
    let estado = EstadoTelaFinanceiro {
        analise: vec![item(b.id, 300), item(a.id, 300), item(c.id, 900)],
        categorias: vec![a, b, c],
        ..Default::default()
    };
    let ordem = || -> Vec<String> {
        estado
            .top_categorias()
            .into_iter()
            .map(|i| i.rotulo)
            .collect()
    };
    let primeira = ordem();
    assert_eq!(primeira, vec!["Peças", "Aluguel", "Energia"]);
    for _ in 0..200 {
        assert_eq!(ordem(), primeira, "mesmos dados, mesma ordem — sempre");
    }
}

#[test]
fn aba_resultado_apura_o_mes_pelo_razao_e_navega_entre_meses() {
    let t = motor_de_teste();
    lancar_a_receber(&t.motor, &t.sessao, 250);
    let mut estado = EstadoTelaFinanceiro {
        aba: Aba::Resultado,
        ..Default::default()
    };
    estado.carregar(&t.motor, &t.sessao);
    assert!(estado.erro.is_none(), "{:?}", estado.erro);
    let cmp = estado.dre.clone().expect("DRE carregada");
    assert_eq!(cmp.atual.lucro_liquido, Dinheiro::reais(250));
    assert_eq!(cmp.mes_anterior.lucro_liquido, Dinheiro::ZERO);
    // Ainda nada foi recebido: o DFC (realizado) está zerado, ao contrário da DRE (competência).
    assert!(estado.dfc.clone().expect("DFC").variacao.e_zero());
    // O indireto explica por quê: lucro de 250 preso no a receber, mesmo caixa zerado.
    let ind = estado.dfc_indireto.clone().expect("DFC indireto");
    assert_eq!(ind.lucro_liquido, Dinheiro::reais(250));
    assert!(ind.variacao.e_zero() && ind.diferenca_de_datas.e_zero());
    assert!(ind.blocos[0]
        .linhas
        .iter()
        .any(|l| l.rotulo == "(Aumento) redução de contas a receber"
            && l.valor == Dinheiro::reais(-250)));
    // As duas tabelas desenham.
    let ctx = egui::Context::default();
    cardeal_ui::tokens::instalar_fontes(&ctx);
    cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
    estado.visao_resultado = 1;
    for indireto in [false, true] {
        estado.dfc_metodo_indireto = indireto;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                painel_dre(ui, &t.motor, &t.sessao, &mut estado);
            });
        });
    }
    // O balancete do mês: a receita de 250 lançada e fechando.
    let b = estado.balancete.clone().expect("balancete");
    let (anterior, debitos, creditos, atual) = super::balancete::totais(&b);
    assert!(anterior.e_zero() && atual.e_zero());
    assert_eq!(debitos, creditos);
    assert_eq!(debitos, Dinheiro::reais(250));
    assert_eq!(
        super::balancete::com_natureza(Dinheiro::reais(-250)),
        "250,00 C"
    );
    estado.visao_resultado = 2;
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            painel_dre(ui, &t.motor, &t.sessao, &mut estado);
        });
    });
    estado.visao_resultado = 0;

    // Mês passado: nada lançado.
    estado.dre_mes = Some(estado.mes_dre().anterior());
    estado.carregar_dre(&t.motor, &t.sessao);
    assert_eq!(estado.dre.expect("DRE").atual.lucro_liquido, Dinheiro::ZERO);
}

#[test]
fn aba_cobranca_lista_quem_esta_devendo_com_o_atraso() {
    let t = motor_de_teste();
    let cliente: mod_clientes::PessoaCadastrada = t
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
    let vencido = Data::hoje(Fuso::BRASILIA).mais_dias(-40);
    let _: mod_financeiro::TituloAReceberLancado = t
        .motor
        .executar(
            &t.sessao,
            "financeiro.lancar_titulo_a_receber.v1",
            &LancarTituloAReceber {
                cliente: Some(cliente.pessoa),
                valor_total: Dinheiro::reais(300),
                emissao: vencido.mais_dias(-30),
                parcelas: 1,
                primeiro_vencimento: vencido,
                intervalo_dias: 0,
                observacao: None,
                categoria: None,
                quitado_agora: None,
            },
        )
        .expect("título");

    let mut estado = EstadoTelaFinanceiro {
        aba: Aba::Cobranca,
        ..Default::default()
    };
    estado.carregar(&t.motor, &t.sessao);
    assert!(estado.erro.is_none(), "{:?}", estado.erro);
    let aging = estado.aging.clone().expect("aging");
    assert_eq!(aging.vencido_mais_de_30, Dinheiro::reais(300));
    assert_eq!(aging.devedores.len(), 1);
    assert_eq!(estado.nome_devedor(&aging.devedores[0]), "Maria Souza");
    assert!(mensagem("Maria Souza", &aging.devedores[0]).contains("R$ 300,00"));

    // A chave "a quem eu devo" troca de consulta (nada a pagar lançado).
    estado.cobranca_pagar = true;
    estado.carregar_cobranca(&t.motor, &t.sessao);
    assert!(estado.aging.expect("aging").total_aberto.e_zero());
}

#[test]
fn aba_plano_carrega_a_arvore_e_o_modelo_acrescenta_contas_e_liga_categorias() {
    let t = motor_de_teste();
    let mut estado = EstadoTelaFinanceiro {
        aba: Aba::Plano,
        ..Default::default()
    };
    estado.carregar(&t.motor, &t.sessao);
    assert!(estado.erro.is_none(), "{:?}", estado.erro);
    assert!(estado.plano.nos.len() > 40, "o plano padrão já vem pronto");
    assert!(!estado.plano.nos.iter().any(|n| n.codigo == "5.20"));

    let _: u32 = t
        .motor
        .executar(
            &t.sessao,
            "financeiro.criar_categorias_sugeridas.v1",
            &mod_financeiro::CriarCategoriasSugeridas,
        )
        .expect("categorias");
    let r: mod_financeiro::ModeloAplicado = t
        .motor
        .executar(
            &t.sessao,
            "financeiro.aplicar_modelo_de_plano.v1",
            &mod_financeiro::AplicarModeloDePlano {
                modelo: cardeal_ledger::ModeloDePlano::Servicos,
            },
        )
        .expect("modelo");
    assert!(!r.criadas.is_empty());
    estado.plano.carregar(&t.motor, &t.sessao);
    assert!(estado.plano.nos.iter().any(|n| n.codigo == "5.20"));
    assert!(
        estado.plano.vinculos.len() >= 10,
        "aluguel, energia, pró-labore… passaram a lançar nas contas próprias"
    );
}

#[test]
fn visao_geral_carrega_o_score_de_saude() {
    let t = motor_de_teste();
    lancar_a_receber(&t.motor, &t.sessao, 800);
    let mut estado = EstadoTelaFinanceiro {
        aba: Aba::Visao,
        ..Default::default()
    };
    estado.carregar(&t.motor, &t.sessao);
    assert!(estado.erro.is_none(), "{:?}", estado.erro);
    let s = estado.saude.clone().expect("score carregado");
    assert!(s.score <= 100);
    assert_eq!(s.sinais.len(), 5);
}

#[test]
fn aba_conciliacao_importa_ofx_sugere_a_baixa_e_concilia() {
    let t = motor_de_teste();
    let banco: ContaBancariaCriada = t
        .motor
        .executar(
            &t.sessao,
            "financeiro.criar_conta_bancaria.v1",
            &CriarContaBancaria {
                nome: "Nubank".to_owned(),
            },
        )
        .expect("banco");
    let hoje = Data::hoje(Fuso::BRASILIA);
    let parcela = lancar_a_receber(&t.motor, &t.sessao, 340);
    let _: mod_financeiro::RecebimentoBaixado = t
        .motor
        .executar(
            &t.sessao,
            "financeiro.baixar_recebimento.v1",
            &BaixarRecebimento {
                parcela,
                valor: Dinheiro::reais(340),
                data: hoje,
                meio_pagamento: MeioPagamento::Pix,
                conta_destino: Some(banco.conta),
            },
        )
        .expect("baixa");

    let mut estado = EstadoTelaFinanceiro {
        aba: Aba::Conciliacao,
        ..Default::default()
    };
    estado.carregar(&t.motor, &t.sessao);
    assert!(estado.erro.is_none(), "{:?}", estado.erro);
    assert!(
        estado
            .conciliacao
            .contas
            .iter()
            .any(|c| c.conta == banco.conta),
        "a conta bancária aparece para escolher"
    );
    let d = hoje.formatar_iso().replace('-', "");
    let ofx = format!(
        "<OFX><STMTTRN><DTPOSTED>{d}<TRNAMT>340.00<FITID>1<MEMO>PIX RECEBIDO\n</STMTTRN></OFX>"
    );
    estado.conciliacao.conta = Some(banco.conta);
    importar_texto(
        &egui::Context::default(),
        &t.motor,
        &t.sessao,
        &mut estado,
        banco.conta,
        "extrato.ofx",
        &ofx,
    );
    assert_eq!(estado.conciliacao.itens.len(), 1);
    let item = estado.conciliacao.itens[0].clone();
    assert_eq!(item.sugestoes.len(), 1, "a baixa que já existe");
    let _: () = t
        .motor
        .executar(
            &t.sessao,
            "financeiro.conciliar_item.v1",
            &mod_financeiro::ConciliarItem {
                item: item.id,
                alvo: item.sugestoes[0].alvo,
            },
        )
        .expect("conciliar");
    estado.conciliacao.recarregar_itens(&t.motor, &t.sessao);
    assert_eq!(
        estado.conciliacao.itens[0].estado,
        mod_financeiro::EstadoItem::Conciliado
    );
    // Texto em ISO-8859-1 (extratos antigos) também é lido.
    assert_eq!(ler_texto(&[b'a', 0xE7, b'a', b'o']), "açao");
}

#[test]
fn conferencia_do_saldo_le_o_ofx_aceita_saldo_digitado_e_alerta_na_visao() {
    let t = motor_de_teste();
    let banco: ContaBancariaCriada = t
        .motor
        .executar(
            &t.sessao,
            "financeiro.criar_conta_bancaria.v1",
            &CriarContaBancaria {
                nome: "Sicredi".to_owned(),
            },
        )
        .expect("banco");
    let hoje = Data::hoje(Fuso::BRASILIA);
    let mut estado = EstadoTelaFinanceiro {
        aba: Aba::Conciliacao,
        ..Default::default()
    };
    estado.carregar(&t.motor, &t.sessao);
    estado.conciliacao.conta = Some(banco.conta);
    estado.conciliacao.recarregar_itens(&t.motor, &t.sessao);
    assert!(estado
        .conciliacao
        .conferencia()
        .expect("a conta")
        .fechamento
        .is_none());

    // Uma tarifa de 20 dias atrás, nunca conciliada, e o saldo de −15 no LEDGERBAL.
    let iso = |d: Data| d.formatar_iso().replace('-', "");
    let ofx = format!(
        "<OFX><STMTTRN><DTPOSTED>{}<TRNAMT>-15.00<FITID>T1<MEMO>TARIFA
</STMTTRN><LEDGERBAL><BALAMT>-15.00<DTASOF>{}</LEDGERBAL></OFX>",
        iso(hoje.mais_dias(-20)),
        iso(hoje)
    );
    importar_texto(
        &egui::Context::default(),
        &t.motor,
        &t.sessao,
        &mut estado,
        banco.conta,
        "extrato.ofx",
        &ofx,
    );
    let f = estado
        .conciliacao
        .conferencia()
        .and_then(|c| c.fechamento)
        .expect("o saldo do OFX");
    assert_eq!(f.banco.saldo, Dinheiro::reais(-15));
    assert_eq!(
        f.sem_explicacao(),
        Dinheiro::ZERO,
        "a tarifa pendente explica"
    );
    // A tarifa parada há 20 dias vira alerta na Visão geral.
    let alertas = estado.conciliacao.alertas(hoje);
    assert!(
        alertas
            .iter()
            .any(|a| a.starts_with("Sicredi:") && a.contains("20 dias")),
        "{alertas:?}"
    );

    // Saldo digitado pelo formulário.
    estado.conciliacao.saldo_form = Some((hoje.formatar(), "-25,00".to_owned()));
    super::conferencia::informar_saldo(&t.motor, &t.sessao, &mut estado).expect("informar");
    assert!(estado.conciliacao.saldo_form.is_none());
    let f = estado
        .conciliacao
        .conferencia()
        .and_then(|c| c.fechamento)
        .unwrap();
    assert_eq!(f.sem_explicacao(), Dinheiro::reais(-10));
    // Valor ilegível: recusado sem chamar o motor.
    estado.conciliacao.saldo_form = Some((hoje.formatar(), "abc".to_owned()));
    assert!(super::conferencia::informar_saldo(&t.motor, &t.sessao, &mut estado).is_err());

    // Os dois painéis desenham.
    let ctx = egui::Context::default();
    cardeal_ui::tokens::instalar_fontes(&ctx);
    cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
    for aba in [Aba::Conciliacao, Aba::Visao] {
        estado.aba = aba;
        for _ in 0..2 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    mostrar(ui, &t.motor, &t.sessao, &mut estado);
                });
            });
        }
    }
}

#[test]
fn escolher_o_fornecedor_preenche_o_lancamento_com_o_ultimo_dele() {
    let t = motor_de_teste();
    let (m, s) = (&t.motor, &t.sessao);
    let ceee: PessoaCadastrada = m
        .executar(
            s,
            "clientes.criar_pessoa.v1",
            &CriarPessoa {
                tipo: TipoPessoa::Juridica,
                nome: "Companhia de Energia".into(),
                nome_fantasia: None,
                papel_inicial: Papel::Fornecedor,
                documento_tipo: None,
                documento_numero: None,
                data_nascimento: None,
                endereco: None,
                contato: None,
            },
        )
        .expect("fornecedor");
    let ontem = Data::hoje(Fuso::BRASILIA).mais_dias(-1);
    let _: mod_financeiro::TituloAPagarLancado = m
        .executar(
            s,
            "financeiro.lancar_titulo_a_pagar.v1",
            &LancarTituloAPagar {
                fornecedor: Some(ceee.pessoa),
                valor_total: Dinheiro::centavos(31_240),
                emissao: ontem,
                parcelas: 1,
                primeiro_vencimento: ontem,
                intervalo_dias: 0,
                observacao: Some("Conta de luz".into()),
                categoria: None,
                quitado_agora: None,
            },
        )
        .expect("luz");

    let mut estado = EstadoTelaFinanceiro::default();
    estado.carregar(m, s);
    let mut form = FormLancar::novo(false);
    form.valor = "300,00".into(); // o que o usuário já digitou não é sobrescrito
    form.pessoa.nenhuma = false;
    form.pessoa.selecionada = Some(ceee.pessoa);
    estado.dlg = Dlg::Lancar(form);
    let ctx = egui::Context::default();
    cardeal_ui::tokens::instalar_fontes(&ctx);
    cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
    for _ in 0..2 {
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            dialogo_lancar(ctx, m, s, &mut estado);
        });
    }
    let Dlg::Lancar(f) = &estado.dlg else {
        panic!("o diálogo segue aberto");
    };
    assert_eq!(f.descricao, "Conta de luz");
    assert_eq!(f.valor, "300,00");
    let aviso = f.aviso_preenchido.as_deref().expect("o aviso");
    assert!(
        aviso.contains("descrição") && !aviso.contains("valor"),
        "{aviso}"
    );
    // Sem pessoa (ou pessoa sem histórico): nada a preencher.
    let mut vazio = FormLancar::novo(true);
    assert!(preencher_com_o_ultimo(m, s, &mut vazio, ceee.pessoa).is_none());
}

#[test]
fn aba_cheques_recebe_compensa_e_devolve() {
    use mod_financeiro::cheque::EstadoCheque;

    let t = motor_de_teste();
    let (m, s) = (&t.motor, &t.sessao);
    let banco: ContaBancariaCriada = m
        .executar(
            s,
            "financeiro.criar_conta_bancaria.v1",
            &CriarContaBancaria {
                nome: "Sicredi".to_owned(),
            },
        )
        .expect("banco");
    let parcela = lancar_a_receber(m, s, 180);
    let mut estado = EstadoTelaFinanceiro {
        aba: Aba::Cheques,
        ..Default::default()
    };
    estado.carregar(m, s);
    assert!(estado.cheques.erro.is_none(), "{:?}", estado.cheques.erro);
    assert!(estado.cheques.abertas.iter().any(|p| p.parcela == parcela));

    // Recebe pelo formulário (o "bom para" em branco = à vista).
    let hoje = Data::hoje(Fuso::BRASILIA);
    estado.cheques.form = Some(super::cheques::FormCheque {
        parcela: Some(parcela),
        valor: "180,00".into(),
        recebido_em: hoje.formatar(),
        banco: "104".into(),
        agencia: "0001".into(),
        conta: "998877".into(),
        numero: "000045".into(),
        emitente: "Maria Souza".into(),
        ..Default::default()
    });
    let r = super::cheques::registrar(m, s, &mut estado).expect("registrar");
    assert!(r.parcela_quitada);
    assert!(estado.cheques.form.is_none());
    let c = estado.cheques.itens[0].cheque.clone();
    assert!(c.pronto_para_depositar(hoje), "à vista: já pode depositar");

    // Compensa no banco e depois devolve.
    estado.cheques.acao = Some(super::cheques::AcaoCheque::Compensar {
        cheque: c.id,
        conta: Some(banco.conta),
        data: hoje.formatar(),
    });
    super::cheques::executar_acao(m, s, &mut estado).expect("compensar");
    assert_eq!(
        estado.cheques.itens[0].cheque.estado,
        EstadoCheque::Compensado
    );
    estado.cheques.acao = Some(super::cheques::AcaoCheque::Devolver {
        cheque: c.id,
        alinea: Some(11),
        observacao: String::new(),
    });
    super::cheques::executar_acao(m, s, &mut estado).expect("devolver");
    assert_eq!(
        estado.cheques.itens[0].cheque.estado,
        EstadoCheque::Devolvido
    );
    assert!(
        estado.cheques.abertas.iter().any(|p| p.parcela == parcela),
        "a conta voltou a ficar em aberto"
    );

    // A aba desenha, com o formulário e uma ação abertos.
    estado.cheques.filtro = 3;
    estado.cheques.form = Some(super::cheques::FormCheque::default());
    let ctx = egui::Context::default();
    cardeal_ui::tokens::instalar_fontes(&ctx);
    cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
    for _ in 0..2 {
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                mostrar(ui, m, s, &mut estado);
            });
        });
    }
}

#[test]
fn pacote_do_contador_grava_pdf_e_planilha_do_mes() {
    let t = motor_de_teste();
    lancar_a_receber(&t.motor, &t.sessao, 250);
    let mut estado = EstadoTelaFinanceiro {
        aba: Aba::Resultado,
        ..Default::default()
    };
    estado.carregar(&t.motor, &t.sessao);
    let dir = tempfile::tempdir().unwrap();
    let gravado = super::pacote_contador::gravar(&t.motor, &t.sessao, &estado, dir.path())
        .expect("gravar o pacote");
    assert_eq!(
        (gravado.xmls, gravado.pendencias, gravado.buscando),
        (0, 0, 0),
        "mês sem notas: sem ZIP"
    );
    let mut arquivos: Vec<std::path::PathBuf> = std::fs::read_dir(&gravado.pasta)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    arquivos.sort();
    assert_eq!(arquivos.len(), 2, "{arquivos:?}");
    let xlsx = std::fs::read(&arquivos[0]).unwrap();
    let pdf = std::fs::read(&arquivos[1]).unwrap();
    assert!(arquivos[0].to_string_lossy().ends_with(".xlsx"));
    assert!(pdf.starts_with(b"%PDF-1."));
    // O `.xlsx` guarda os XMLs sem compressão: dá para conferir o conteúdo nos bytes.
    let texto = String::from_utf8_lossy(&xlsx);
    assert!(xlsx.starts_with(b"PK"));
    for aba in ["DRE", "DFC direto", "DFC indireto", "Balancete", "Diário"] {
        assert!(texto.contains(&format!("name=\"{aba}\"")), "aba {aba}");
    }
    assert!(
        texto.contains("<v>250.00</v>"),
        "a receita de 250 na planilha"
    );
    // Para conferir num Excel de verdade: CARDEAL_SALVAR_PACOTE=<pasta> cargo test …
    if let Ok(destino) = std::env::var("CARDEAL_SALVAR_PACOTE") {
        for a in &arquivos {
            std::fs::copy(
                a,
                std::path::Path::new(&destino).join(a.file_name().unwrap()),
            )
            .unwrap();
        }
    }
}

#[test]
fn pacote_do_contador_leva_os_xmls_do_mes_num_zip() {
    let t = motor_de_teste();
    let hoje = Data::hoje(Fuso::BRASILIA);
    let chave = "4".repeat(44);
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<nfeProc xmlns="http://www.portalfiscal.inf.br/nfe"><NFe>
<infNFe Id="NFe{chave}" versao="4.00">
  <ide><nNF>77</nNF><serie>1</serie><dhEmi>{}T09:00:00-03:00</dhEmi></ide>
  <emit><CNPJ>45543915000181</CNPJ><xNome>Atacado Beta LTDA</xNome></emit>
  <det nItem="1"><prod><cProd>FER-10</cProd><xProd>FERM BIOLOGICO 10G</xProd><NCM>21021090</NCM>
    <qCom>24.0000</qCom><vUnCom>1.500000</vUnCom><vProd>36.00</vProd></prod></det>
  <total><ICMSTot><vProd>36.00</vProd><vFrete>0.00</vFrete><vSeg>0.00</vSeg><vOutro>0.00</vOutro>
    <vNF>36.00</vNF></ICMSTot></total>
</infNFe></NFe></nfeProc>"#,
        hoje.formatar_iso()
    );
    let _: mod_compras::RelatorioImportacao = t
        .motor
        .executar(
            &t.sessao,
            "compras.importar_nota_de_arquivo_xml.v1",
            &mod_compras::ImportarNotaDeArquivoXml { xml: xml.clone() },
        )
        .expect("importa a nota de compra");
    let mut estado = EstadoTelaFinanceiro {
        aba: Aba::Resultado,
        ..Default::default()
    };
    estado.carregar(&t.motor, &t.sessao);
    let dir = tempfile::tempdir().unwrap();
    let gravado = super::pacote_contador::gravar(&t.motor, &t.sessao, &estado, dir.path())
        .expect("gravar o pacote");
    assert_eq!((gravado.xmls, gravado.pendencias), (1, 0));
    let mes = hoje.competencia();
    let aaaamm = format!("{}-{:02}", mes.ano(), mes.mes());
    let zip = std::fs::read(gravado.pasta.join(format!("XMLs {aaaamm}.zip"))).unwrap();
    assert_eq!(
        cardeal_planilha::zip::ler(&zip),
        vec![(format!("Entradas/{chave}-nfe.xml"), xml.into_bytes())],
        "o XML como veio do fornecedor"
    );
    let xlsx = std::fs::read(gravado.pasta.join(format!("Contabilidade {aaaamm}.xlsx"))).unwrap();
    let texto = String::from_utf8_lossy(&xlsx);
    assert!(texto.contains("name=\"Notas de entrada\""));
    assert!(texto.contains("Atacado Beta LTDA"));
    assert!(
        !texto.contains("name=\"Notas emitidas\""),
        "nenhuma nota emitida: sem a aba"
    );
    // Para conferir o ZIP num descompactador de verdade: CARDEAL_SALVAR_PACOTE=<pasta> cargo test …
    if let Ok(destino) = std::env::var("CARDEAL_SALVAR_PACOTE") {
        for e in std::fs::read_dir(&gravado.pasta).unwrap() {
            let a = e.unwrap().path();
            std::fs::copy(
                &a,
                std::path::Path::new(&destino).join(a.file_name().unwrap()),
            )
            .unwrap();
        }
    }
}

#[test]
fn pacote_do_contador_resume_as_notas_e_lista_as_pendencias() {
    use super::pacote_contador::{
        arquivos_xml, pendencias, planilha, relatorio, DadosDoPacote, NotasDeEntrada, NotasEmitidas,
    };
    use mod_fiscal::{
        AmbienteFiscal, DocumentoResumo, EstadoDocumento, ModeloFiscal, NotaDoMes, NotasDoMes,
        PedidoDocumento, SituacaoNoMes,
    };
    let t = motor_de_teste();
    let mut estado = EstadoTelaFinanceiro {
        aba: Aba::Resultado,
        ..Default::default()
    };
    estado.carregar(&t.motor, &t.sessao);
    let hoje = Data::hoje(Fuso::BRASILIA);
    let nota = |situacao, numero: u64| NotaDoMes {
        modelo: ModeloFiscal::Nfce,
        serie: 1,
        numero,
        situacao,
        data: hoje,
        chave: Some(format!("{numero:044}")),
        protocolo: Some("143260000000001".into()),
        protocolo_evento: None,
        total: Dinheiro::reais(10),
        xml: Some("<nfeProc/>".into()),
        xml_evento: None,
        nota_assinada_guardada: false,
    };
    let rejeitada = DocumentoResumo {
        id: Id::novo(),
        modelo: ModeloFiscal::Nfce,
        serie: 1,
        numero: 9,
        ambiente: AmbienteFiscal::Producao,
        estado: EstadoDocumento::Rejeitada,
        total: Dinheiro::reais(5),
        chave: None,
        protocolo: None,
        codigo: None,
        motivo: None,
        origem: Id::novo(),
        tentativas: 1,
        criado_em: cardeal_kernel::Instante::agora(),
        autorizado_em: None,
        pedido: None,
        anulada: false,
    };
    let emitidas: NotasEmitidas = Ok(NotasDoMes {
        notas: vec![
            nota(SituacaoNoMes::Autorizada, 1),
            nota(SituacaoNoMes::Cancelada, 2),
        ],
        sem_desfecho: vec![
            rejeitada.clone(),
            // A venda estornada já pôs a inutilização na fila: falta transmitir.
            DocumentoResumo {
                numero: 10,
                pedido: Some(PedidoDocumento::Inutilizar),
                ..rejeitada
            },
        ],
        de_homologacao: 3,
    });
    let entradas: NotasDeEntrada = Ok(vec![mod_compras::NotaDeEntradaDoMes {
        nota: Id::novo(),
        fornecedor: "Atacado Beta LTDA".into(),
        chave: Some("5".repeat(44)),
        numero: "12".into(),
        serie: "1".into(),
        data_emissao: hoje,
        valor_total: Dinheiro::reais(36),
        estado: mod_compras::EstadoNotaEntrada::Confirmada,
        xml: None,
    }]);

    // O que precisa de atenção, com o que fazer.
    let p = pendencias(&emitidas, &entradas);
    assert_eq!(p.len(), 4, "{p:?}");
    assert!(p[0].0.starts_with("NFC-e série 1 nº 9 — rejeitada"));
    assert!(p[0].1.contains("inutilize"));
    assert_eq!(
        p[1].1,
        "Inutilização na fila — transmita em Notas fiscais e gere o pacote de novo."
    );
    assert!(p[2].0.starts_with("1 × XML do cancelamento"));
    assert!(p[3].1.contains("Importe o arquivo XML de novo"));
    // No ZIP: as duas notas (a cancelada sem o evento); a entrada não tem XML.
    assert_eq!(arquivos_xml(&emitidas, &entradas).len(), 2);

    let dados = DadosDoPacote {
        mes: hoje.competencia(),
        dre: estado.dre.as_ref().unwrap(),
        direto: estado.dfc.as_ref().unwrap(),
        indireto: estado.dfc_indireto.as_ref().unwrap(),
        balancete: estado.balancete.as_deref().unwrap(),
        diario: &[],
        emitidas: &emitidas,
        entradas: &entradas,
    };
    let r = relatorio(&dados);
    let notas = r
        .secoes
        .iter()
        .find(|s| s.titulo == "Notas fiscais do mês")
        .expect("a seção das notas");
    let linhas: Vec<Vec<String>> = notas.linhas.iter().map(|l| l.celulas.clone()).collect();
    assert_eq!(linhas[0], vec!["NFC-e autorizadas", "1", "R$ 10,00"]);
    assert_eq!(
        linhas[1][..2],
        ["NFC-e canceladas".to_owned(), "1".to_owned()]
    );
    assert_eq!(linhas[2], vec!["Notas de entrada", "1", "R$ 36,00"]);
    assert_eq!(
        linhas[3][..2],
        ["XMLs no pacote".to_owned(), "2".to_owned()]
    );
    assert!(notas
        .nota
        .as_deref()
        .unwrap()
        .contains("3 nota(s) de homologação"));
    assert!(r.secoes.iter().any(|s| s.titulo == "Pendências do mês"));
    let pdf = cardeal_pdf::gerar_relatorio(
        &cardeal_pdf::IdentidadeEmpresa::default(),
        &r,
        cardeal_kernel::Instante::agora(),
    )
    .expect("o PDF");
    assert!(pdf.starts_with(b"%PDF-1."));
    let xlsx = planilha(&dados).gerar();
    let texto = String::from_utf8_lossy(&xlsx);
    assert!(texto.contains("name=\"Notas emitidas\""));
    assert!(texto.contains("Sem desfecho (rejeitada)"));
    assert!(texto.contains("falta o XML do cancelamento"));
    assert!(texto.contains("importe o arquivo XML de novo"));

    // Sem permissão para ver as notas: o PDF diz por quê, e nada vai no ZIP.
    let sem: NotasEmitidas = Err("o seu usuário não vê as notas fiscais".into());
    let nada: NotasDeEntrada = Ok(Vec::new());
    assert!(arquivos_xml(&sem, &nada).is_empty());
    let r = relatorio(&DadosDoPacote {
        emitidas: &sem,
        entradas: &nada,
        ..dados
    });
    let notas = r
        .secoes
        .iter()
        .find(|s| s.titulo == "Notas fiscais do mês")
        .unwrap();
    assert!(notas
        .nota
        .as_deref()
        .unwrap()
        .contains("Notas emitidas: o seu usuário não vê as notas fiscais."));
    assert!(!r.secoes.iter().any(|s| s.titulo == "Pendências do mês"));
}

#[test]
fn regua_de_cobranca_na_aba_cobranca_fila_registro_e_editor() {
    let t = motor_de_teste();
    let (m, s) = (&t.motor, &t.sessao);
    let cliente: PessoaCadastrada = m
        .executar(
            s,
            "clientes.criar_pessoa.v1",
            &CriarPessoa {
                tipo: TipoPessoa::Fisica,
                nome: "Ana Paula".into(),
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
    let hoje = Data::hoje(Fuso::BRASILIA);
    let _: mod_financeiro::TituloAReceberLancado = m
        .executar(
            s,
            "financeiro.lancar_titulo_a_receber.v1",
            &LancarTituloAReceber {
                cliente: Some(cliente.pessoa),
                valor_total: Dinheiro::reais(120),
                emissao: hoje.mais_dias(-20),
                parcelas: 1,
                primeiro_vencimento: hoje.mais_dias(-1),
                intervalo_dias: 0,
                observacao: None,
                categoria: None,
                quitado_agora: None,
            },
        )
        .expect("título");
    let mut estado = EstadoTelaFinanceiro {
        aba: Aba::Cobranca,
        ..Default::default()
    };
    estado.carregar(m, s);
    let fila = super::regua::fila(&estado).expect("fila").clone();
    assert_eq!(fila.itens.len(), 1);
    let item = fila.itens[0].clone();
    assert_eq!(item.etapa.nome, "1 dia de atraso");
    let msg = estado.mensagem_da_fila(&item);
    assert!(
        msg.starts_with("Olá, Ana!") && msg.contains("R$ 120,00"),
        "{msg}"
    );

    // Pular tira a cliente da fila (sem telefone, o WhatsApp não aparece).
    super::regua::registrar(m, s, &mut estado, &item, true).expect("pular");
    assert!(super::regua::fila(&estado).unwrap().itens.is_empty());

    // O editor: dias inválidos são recusados; a régua salva com uma etapa nova.
    estado.regua_edicao = Some(vec![super::regua::EtapaEmEdicao {
        dias: "dez".into(),
        nome: "X".into(),
        mensagem: "m".into(),
    }]);
    assert!(super::regua::salvar_regua(m, s, &mut estado).is_err());
    estado.regua_edicao = Some(vec![super::regua::EtapaEmEdicao {
        dias: "-2".into(),
        nome: "Aviso".into(),
        mensagem: "Olá, {nome}!".into(),
    }]);
    super::regua::salvar_regua(m, s, &mut estado).expect("salvar");
    assert!(estado.regua_edicao.is_none());

    // A Visão geral carrega a fila (vazia agora: a cliente foi pulada) e desenha.
    estado.aba = Aba::Visao;
    estado.carregar(m, s);
    assert!(super::regua::fila(&estado).is_some());
    // A aba desenha, com o editor aberto.
    estado.aba = Aba::Cobranca;
    estado.regua_edicao = Some(vec![super::regua::EtapaEmEdicao::default()]);
    let ctx = egui::Context::default();
    cardeal_ui::tokens::instalar_fontes(&ctx);
    cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
    for _ in 0..2 {
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                mostrar(ui, m, s, &mut estado);
            });
        });
    }
}

#[test]
fn aviso_do_certificado_vencendo_na_visao_pede_a_configuracao_fiscal() {
    let t = motor_de_teste();
    let (m, s) = (&t.motor, &t.sessao);
    let mut estado = EstadoTelaFinanceiro::default();
    estado.carregar(m, s);
    // Sem certificado instalado: nada na Visão (nem toda empresa emite).
    assert!(estado
        .situacao_fiscal
        .as_ref()
        .is_some_and(|f| f.certificado.is_none()));
    estado.situacao_fiscal = Some(mod_fiscal::SituacaoFiscalDaEmpresa {
        certificado: None,
        ambiente: mod_fiscal::AmbienteFiscal::Producao,
        pendencias: vec![mod_fiscal::PendenciaFiscal::CertificadoVencendo { dias: 9 }],
    });
    let ctx = egui::Context::default();
    cardeal_ui::tokens::instalar_fontes(&ctx);
    cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
    for _ in 0..2 {
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                mostrar(ui, m, s, &mut estado);
            });
        });
    }
    assert!(!estado.pedido_config_fiscal);
}

#[test]
fn dialogo_da_parcela_com_ajustes_desenha_e_a_conta_cancelada_sai_como_cancelada() {
    let t = motor_de_teste();
    let parcela = lancar_a_receber(&t.motor, &t.sessao, 120);
    let mut estado = EstadoTelaFinanceiro {
        aba: Aba::Receber,
        ..Default::default()
    };
    estado.carregar(&t.motor, &t.sessao);
    let p = estado
        .parcelas
        .iter()
        .find(|p| p.parcela == parcela)
        .cloned()
        .expect("parcela");
    estado.dlg = Dlg::Baixar {
        parcela,
        valor: p.saldo().formatar(),
        data: Data::hoje(Fuso::BRASILIA).to_string(),
        pagamento: crate::pagamento::EstadoPagamento::novo(MeioPagamento::Dinheiro),
        baixas: Vec::new(),
        baixas_carregadas: true,
        motivo_estorno: String::new(),
        situacao: None,
        valor_sugerido: String::new(),
        renegociar: FormRenegociar::default(),
        ajuste: FormAjuste {
            motivo: "curto".to_owned(),
            confirmar_cancelamento: true,
            ..FormAjuste::default()
        },
    };
    let ctx = egui::Context::default();
    cardeal_ui::tokens::instalar_fontes(&ctx);
    cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
    for _ in 0..2 {
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            dialogo_baixar(ctx, &t.motor, &t.sessao, &mut estado);
        });
    }
    // Motivo curto: avisa e não pede confirmação.
    let Dlg::Baixar { ajuste, .. } = &estado.dlg else {
        panic!("o diálogo continua aberto");
    };
    assert!(!ajuste.confirmar_cancelamento);

    let _: mod_financeiro::TituloFoiCancelado = t
        .motor
        .executar(
            &t.sessao,
            "financeiro.cancelar_titulo.v1",
            &mod_financeiro::CancelarTitulo {
                titulo: p.titulo,
                motivo: "Lançada em duplicidade".to_owned(),
            },
        )
        .expect("cancelar");
    estado.carregar(&t.motor, &t.sessao);
    assert!(estado
        .parcelas
        .iter()
        .filter(|x| x.parcela == parcela)
        .all(|x| x.estado == EstadoParcela::Cancelada));
}

#[test]
fn conta_nova_com_saldo_e_transferencia_aparecem_na_aba_de_bancos() {
    let t = motor_de_teste();
    let mut estado = EstadoTelaFinanceiro {
        aba: Aba::Bancos,
        ..Default::default()
    };
    estado.carregar(&t.motor, &t.sessao);
    let ctx = egui::Context::default();
    cardeal_ui::tokens::instalar_fontes(&ctx);
    cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);

    // A conta nasce já com o saldo de hoje.
    estado.dlg = Dlg::NovaContaBancaria {
        nome: "Sicredi".to_owned(),
        saldo: "1.500,00".to_owned(),
    };
    super::bancos::criar_conta(&ctx, &t.motor, &t.sessao, &mut estado);
    assert!(matches!(estado.dlg, Dlg::Fechado));
    let sicredi = estado
        .contas_disp
        .iter()
        .find(|c| c.nome == "Sicredi")
        .cloned()
        .expect("conta criada");
    assert_eq!(sicredi.saldo, Dinheiro::reais(1_500));

    let caixa = estado
        .contas_disp
        .iter()
        .find(|c| c.e_caixa)
        .map(|c| c.conta)
        .expect("caixa");
    let _: mod_financeiro::TransferenciaFeita = t
        .motor
        .executar(
            &t.sessao,
            "financeiro.transferir_entre_contas.v1",
            &mod_financeiro::TransferirEntreContas {
                origem: sicredi.conta,
                destino: caixa,
                valor: Dinheiro::reais(200),
                data: Data::hoje(Fuso::BRASILIA),
                observacao: None,
            },
        )
        .expect("transferir");
    estado.carregar_fluxo(&t.motor, &t.sessao);
    assert_eq!(estado.transferencias.len(), 1);

    // A aba e os diálogos desenham.
    let dialogos = [
        Dlg::SaldoInicial {
            conta: sicredi.conta,
            nome: "Sicredi".to_owned(),
            valor: String::new(),
            data: Data::hoje(Fuso::BRASILIA).to_string(),
        },
        Dlg::Transferir(super::bancos::FormTransferencia::default()),
        Dlg::CancelarTransferencia {
            id: estado.transferencias[0].id,
            descricao: "R$ 200,00".to_owned(),
            motivo: String::new(),
        },
    ];
    for dlg in dialogos {
        estado.dlg = dlg;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                super::bancos::painel_bancos(ui, &t.sessao, &mut estado);
            });
            match &estado.dlg {
                Dlg::SaldoInicial { .. } => {
                    super::bancos::dialogo_saldo_inicial(ctx, &t.motor, &t.sessao, &mut estado);
                }
                Dlg::Transferir(_) => {
                    super::bancos::dialogo_transferir(ctx, &t.motor, &t.sessao, &mut estado);
                }
                _ => super::bancos::dialogo_cancelar_transferencia(
                    ctx,
                    &t.motor,
                    &t.sessao,
                    &mut estado,
                ),
            }
        });
    }
}
