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
