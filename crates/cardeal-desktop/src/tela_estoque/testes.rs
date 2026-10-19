//! Testes ponta a ponta das abas Estrutura e Preços: motor real (SQLite temporário).

use super::*;
use crate::testes_comum::motor_de_teste;
use cardeal_kernel::Percentual;
use mod_estoque::precificacao::ArredondamentoDePreco;
use mod_estoque::{DefinirMargemDoGrupo, DefinirParametrosPreco, ParametrosPreco};

fn grupo(m: &Motor, s: &Sessao, codigo: &str, pai: Option<Id>) -> Id {
    let g: GrupoProdutoCriado = m
        .executar(
            s,
            "estoque.criar_grupo_produto.v1",
            &CriarGrupoProduto {
                codigo: codigo.to_owned(),
                nome: codigo.to_owned(),
                pai,
            },
        )
        .expect("grupo");
    g.grupo_produto
}

#[test]
fn arvore_mostra_niveis_herda_margem_e_so_oferece_destinos_que_cabem() {
    let t = motor_de_teste();
    let a = grupo(&t.motor, &t.sessao, "A", None);
    let b = grupo(&t.motor, &t.sessao, "B", Some(a));
    let c = grupo(&t.motor, &t.sessao, "C", Some(b));
    let x = grupo(&t.motor, &t.sessao, "X", None);
    let y = grupo(&t.motor, &t.sessao, "Y", Some(x));
    t.motor
        .executar(
            &t.sessao,
            "estoque.definir_margem_do_grupo.v1",
            &DefinirMargemDoGrupo {
                grupo: a,
                margem: Some(Percentual::pontos(20)),
            },
        )
        .expect("margem");

    let mut estado = EstadoTelaEstoque::default();
    estado.carregar(&t.motor, &t.sessao);
    assert!(estado.erro.is_none(), "{:?}", estado.erro);

    let niveis: Vec<(String, u8)> = estado
        .est
        .nos
        .iter()
        .map(|n| (n.codigo.clone(), n.nivel))
        .collect();
    assert_eq!(
        niveis,
        [
            ("A".into(), 1),
            ("B".into(), 2),
            ("C".into(), 3),
            ("X".into(), 1),
            ("Y".into(), 2)
        ]
    );
    // C não tem margem própria: herda de A, passando por B.
    let (m, de) = estado.est.margem_herdada(c).expect("herdada");
    assert_eq!((m, de.as_str()), (Percentual::pontos(20), "A"));
    assert!(estado.est.margem_herdada(a).is_none());

    // B (altura 2) pode ir para a raiz ou para X (nível 2); sob Y (nível 2) chegaria ao 4: cabe.
    let nomes: Vec<String> = estado
        .est
        .destinos_validos(b)
        .into_iter()
        .map(|d| d.1)
        .collect();
    assert!(nomes.iter().any(|n| n.contains("primeiro nível")));
    assert!(nomes.iter().any(|n| n.ends_with("› Y")));
    // A (altura 3) não cabe sob Y (nível 2 → 5): fica de fora; nem dentro de si mesmo.
    let de_a: Vec<Id> = estado
        .est
        .destinos_validos(a)
        .into_iter()
        .map(|d| d.0)
        .collect();
    assert!(!de_a.contains(&y) && !de_a.contains(&c) && !de_a.contains(&a));
    assert!(de_a.contains(&x));
}

/// MEI com 25% de custo operacional, grupo com 15% de margem e a Coca 2L a R$ 50 de custo:
/// 50 ÷ (1 − 0,25 − 0,15) = R$ 83,33.
fn cenario_de_preco(t: &crate::testes_comum::MotorDeTeste) -> Id {
    let g = grupo(&t.motor, &t.sessao, "BEB", None);
    t.motor
        .executar(
            &t.sessao,
            "estoque.definir_margem_do_grupo.v1",
            &DefinirMargemDoGrupo {
                grupo: g,
                margem: Some(Percentual::pontos(15)),
            },
        )
        .expect("margem");
    let mut p = ParametrosPreco::padrao_para("MEI");
    p.custo_operacional = Percentual::pontos(25);
    p.custo_operacional_derivado = false;
    t.motor
        .executar(
            &t.sessao,
            "estoque.definir_parametros_preco.v1",
            &DefinirParametrosPreco { parametros: p },
        )
        .expect("parâmetros");
    let un: UnidadeCriada = t
        .motor
        .executar(
            &t.sessao,
            "estoque.criar_unidade.v1",
            &CriarUnidade {
                sigla: "UN".into(),
                nome: "Un".into(),
                fracionavel: false,
            },
        )
        .expect("unidade");
    let prod: ProdutoCriado = t
        .motor
        .executar(
            &t.sessao,
            "estoque.criar_produto.v1",
            &CriarProduto {
                grupo_produto: g,
                nome: "Coca 2L".into(),
                ncm: "22021000".into(),
                unidade_padrao: un.unidade,
                codigo_barras: None,
                detalhes_tecnicos: None,
            },
        )
        .expect("produto");
    let loc: LocalCriado = t
        .motor
        .executar(
            &t.sessao,
            "estoque.criar_local.v1",
            &CriarLocal {
                nome: "Depósito".into(),
                tipo: TipoLocal::Deposito,
            },
        )
        .expect("local");
    let _: EntradaRegistrada = t
        .motor
        .executar(
            &t.sessao,
            "estoque.registrar_entrada.v1",
            &RegistrarEntrada {
                produto: prod.produto,
                local: loc.local,
                quantidade: Quantidade::unidades(10),
                custo_unitario: Preco::reais(50),
            },
        )
        .expect("entrada");

    prod.produto
}

#[test]
fn aba_precos_calcula_pelo_regime_e_valida_o_formulario() {
    let t = motor_de_teste();
    cenario_de_preco(&t);
    let mut estado = EstadoTelaEstoque {
        aba: AbaEstoque::Precos,
        ..Default::default()
    };
    estado.carregar(&t.motor, &t.sessao);
    // MEI: sem imposto; 25% + 15% de margem → 50 / 0,60 = 83,33.
    let s = &estado.prc.sugestoes[0];
    assert_eq!(s.nome, "Coca 2L");
    assert_eq!(
        s.sugerido.expect("sugerido").preco,
        cardeal_kernel::Dinheiro::centavos(8_333)
    );
    // O formulário refletiu o que foi salvo.
    assert!(estado.prc.montar().is_ok());

    // Texto que não é número vira mensagem com o nome do campo.
    estado.prc.comissao = "abc".into();
    assert!(estado.prc.montar().unwrap_err().contains("Comissão"));
}

#[test]
fn aplicar_preco_sugerido_cria_a_regra_na_tabela_de_vendas() {
    use mod_vendas::{CriarTabelaPreco, RegrasDaTabela, TabelaPrecoCriada, TipoTabela};
    let t = motor_de_teste();
    let produto = cenario_de_preco(&t);
    let tabela: TabelaPrecoCriada = t
        .motor
        .executar(
            &t.sessao,
            "vendas.criar_tabela_preco.v1",
            &CriarTabelaPreco {
                nome: "Balcão".into(),
                tipo: TipoTabela::Venda,
                vigente_de: cardeal_kernel::Data::hoje(Fuso::BRASILIA).mais_dias(-1),
                vigente_ate: None,
            },
        )
        .expect("tabela");

    let mut estado = EstadoTelaEstoque {
        aba: AbaEstoque::Precos,
        ..Default::default()
    };
    estado.carregar(&t.motor, &t.sessao);
    estado.prc.abrir_aplicar(&t.motor, &t.sessao);
    assert_eq!(
        estado.prc.tabela,
        Some(tabela.tabela_preco),
        "já vem escolhida"
    );
    estado.prc.arredondamento = ArredondamentoDePreco::FinalNoventa;
    precos::aplicar(
        &egui::Context::default(),
        &t.motor,
        &t.sessao,
        &mut estado,
        vec![produto],
    );

    let regras: Vec<mod_vendas::RegraPreco> = t
        .motor
        .consultar(
            &t.sessao,
            "vendas.regras_da_tabela.v1",
            &RegrasDaTabela {
                tabela: tabela.tabela_preco,
            },
        )
        .expect("regras");
    assert_eq!(regras.len(), 1);
    assert_eq!(regras[0].preco, Preco::centavos(8_390));
}

#[test]
fn aba_lote_filtra_aplica_e_desfaz_pela_tela() {
    let t = motor_de_teste();
    let a = grupo(&t.motor, &t.sessao, "A", None);
    let b = grupo(&t.motor, &t.sessao, "B", None);
    let un: UnidadeCriada = t
        .motor
        .executar(
            &t.sessao,
            "estoque.criar_unidade.v1",
            &CriarUnidade {
                sigla: "UN".into(),
                nome: "Un".into(),
                fracionavel: false,
            },
        )
        .expect("unidade");
    let mut ids = Vec::new();
    for (nome, g) in [("Coca", a), ("Guaraná", a), ("Arroz", b)] {
        let p: ProdutoCriado = t
            .motor
            .executar(
                &t.sessao,
                "estoque.criar_produto.v1",
                &CriarProduto {
                    grupo_produto: g,
                    nome: nome.into(),
                    ncm: "22021000".into(),
                    unidade_padrao: un.unidade,
                    codigo_barras: None,
                    detalhes_tecnicos: None,
                },
            )
            .expect("produto");
        ids.push(p.produto);
    }
    let mut estado = EstadoTelaEstoque {
        aba: AbaEstoque::Lote,
        ..Default::default()
    };
    estado.carregar(&t.motor, &t.sessao);
    estado.lote.grupo = Some(a);
    estado.lote.carregar(&t.motor, &t.sessao);
    assert_eq!(estado.lote.produtos.len(), 2, "só o grupo A");

    // Sem escolher o que mudar, a tela avisa em vez de chamar o motor.
    assert!(estado.lote.alteracao().is_err());
    estado.lote.margem_modo = 1;
    estado.lote.margem = "abc".into();
    assert!(estado.lote.alteracao().unwrap_err().contains("Margem"));
    estado.lote.margem = "25".into();
    assert!(estado.lote.alteracao().is_ok());

    let marcar: Vec<Id> = estado.lote.produtos.iter().map(|p| p.produto).collect();
    estado.lote.marcados.extend(marcar);
    lote::aplicar(&egui::Context::default(), &t.motor, &t.sessao, &mut estado);
    assert!(estado
        .lote
        .produtos
        .iter()
        .all(|p| p.margem == Some(Percentual::pontos(25))));
    assert_eq!(estado.lote.historico.len(), 1);
    assert!(estado.lote.historico[0].pode_desfazer);
}

#[test]
fn lote_monta_filtros_e_mudancas_fiscais_do_formulario() {
    let mut l = lote::EstadoLote::default();
    l.fabricante = "samsung".into();
    l.ncm_prefixo = "8517".into();
    l.situacao = 1;
    l.so_sem_fiscal = true;
    l.custo_min = "10,50".into();
    let f = l.filtro().unwrap();
    assert_eq!(f.fabricante.as_deref(), Some("samsung"));
    assert_eq!(f.ativo, Some(true));
    assert!(f.sem_perfil_fiscal_proprio);
    assert_eq!(f.custo_minimo, Some(1_050));
    l.custo_max = "abc".into();
    assert!(l.filtro().unwrap_err().contains("Custo máximo"));

    // Mudança fiscal: NCM + preencher só alguns campos.
    l.ncm_novo = "85177099".into();
    l.fiscal_modo = 1;
    l.fisc.cst_icms = "500".into();
    let a = l.alteracao().unwrap();
    assert!(a.mexe_em_fiscal());
    l.ncm_novo = "12".into();
    assert!(l.alteracao().unwrap_err().contains("NCM"));
    l.ncm_novo.clear();
    l.fisc.cst_icms.clear();
    assert!(l.alteracao().is_err(), "preencher sem preencher nada");
    l.fiscal_modo = 2;
    assert!(
        l.alteracao().is_ok(),
        "apagar os campos próprios é uma mudança"
    );
}

#[test]
fn aba_analise_lista_o_produto_parado_com_o_dinheiro_que_ele_prende() {
    let t = motor_de_teste();
    let produto = cenario_de_preco(&t); // 10 un a R$ 50, nenhuma saída
    let mut estado = EstadoTelaEstoque {
        aba: AbaEstoque::Analise,
        ..Default::default()
    };
    estado.analise.carregar(&t.motor, &t.sessao);
    let i = estado
        .analise
        .itens
        .iter()
        .find(|i| i.produto == produto)
        .expect("produto na análise");
    assert_eq!(i.classe, mod_estoque::analise::ClasseAbc::SemGiro);
    assert_eq!(i.valor_em_estoque, cardeal_kernel::Dinheiro::reais(500));
}

#[test]
fn aba_precos_muda_o_custo_de_referencia_e_abre_o_historico_de_compras() {
    let t = motor_de_teste();
    let produto = cenario_de_preco(&t); // 10 un a R$ 50
    let local = t
        .motor
        .consultar(&t.sessao, "estoque.locais.v1", &Locais)
        .expect("locais");
    let _: EntradaRegistrada = t
        .motor
        .executar(
            &t.sessao,
            "estoque.registrar_entrada.v1",
            &RegistrarEntrada {
                produto,
                local: local[0].id,
                quantidade: Quantidade::unidades(10),
                custo_unitario: Preco::reais(70),
            },
        )
        .expect("segunda entrada");

    let mut estado = EstadoTelaEstoque {
        aba: AbaEstoque::Precos,
        ..Default::default()
    };
    estado.carregar(&t.motor, &t.sessao);
    assert_eq!(
        estado.prc.sugestoes[0].custo,
        Some(cardeal_kernel::Dinheiro::reais(60)),
        "custo médio por padrão"
    );

    // O seletor do formulário vai para os parâmetros salvos e muda o custo do preço.
    estado.prc.custo_ref = mod_estoque::CustoDeReferencia::UltimaCompra;
    let p = estado.prc.montar().expect("parâmetros");
    t.motor
        .executar(
            &t.sessao,
            "estoque.definir_parametros_preco.v1",
            &DefinirParametrosPreco { parametros: p },
        )
        .expect("salvar");
    estado.prc.carregar(&t.motor, &t.sessao);
    assert_eq!(
        estado.prc.sugestoes[0].custo,
        Some(cardeal_kernel::Dinheiro::reais(70))
    );

    // Clicar no produto carrega o histórico: 70 (+40% sobre 50) e 50.
    assert!(precos_historico::abrir_historico(
        &t.motor,
        &t.sessao,
        &mut estado.prc,
        produto,
        "Coca 2L".into()
    ));
    let (nome, h) = estado.prc.historico.as_ref().expect("histórico");
    assert_eq!(nome, "Coca 2L");
    assert_eq!(h.compras.len(), 2);
    assert_eq!(h.compras[0].variacao, Some(Percentual::pontos(40)));
    assert_eq!(h.media_ultimas_3, Some(cardeal_kernel::Dinheiro::reais(60)));
}

#[test]
fn secao_de_defasados_lista_o_prejuizo_e_aplicar_leva_ao_alvo() {
    use mod_vendas::{
        CriarRegraPreco, CriarTabelaPreco, SituacaoDoPreco, TabelaPrecoCriada, TipoTabela,
    };
    let t = motor_de_teste();
    let produto = cenario_de_preco(&t); // custo 50; empate 66,67; sugerido 83,33
    let tabela: TabelaPrecoCriada = t
        .motor
        .executar(
            &t.sessao,
            "vendas.criar_tabela_preco.v1",
            &CriarTabelaPreco {
                nome: "Balcão".into(),
                tipo: TipoTabela::Venda,
                vigente_de: cardeal_kernel::Data::hoje(Fuso::BRASILIA).mais_dias(-1),
                vigente_ate: None,
            },
        )
        .expect("tabela");
    let _: mod_vendas::RegraPrecoCriada = t
        .motor
        .executar(
            &t.sessao,
            "vendas.criar_regra_preco.v1",
            &CriarRegraPreco {
                tabela_preco: tabela.tabela_preco,
                alvo_produto: Some(produto),
                alvo_grupo: None,
                quantidade_minima: None,
                preco: Preco::reais(60),
                periodo_de: None,
                periodo_ate: None,
            },
        )
        .expect("regra");

    let mut estado = EstadoTelaEstoque {
        aba: AbaEstoque::Precos,
        ..Default::default()
    };
    estado.carregar(&t.motor, &t.sessao);
    assert_eq!(estado.prc.def.tabela, Some(tabela.tabela_preco));
    assert_eq!(estado.prc.def.itens.len(), 1);
    assert_eq!(estado.prc.def.itens[0].situacao, SituacaoDoPreco::Prejuizo);
    assert_eq!(estado.prc.def.a_corrigir(), vec![produto]);

    estado.prc.arredondamento = ArredondamentoDePreco::FinalNoventa;
    let corrigir = estado.prc.def.a_corrigir();
    precos::aplicar_em(
        &egui::Context::default(),
        &t.motor,
        &t.sessao,
        &mut estado,
        tabela.tabela_preco,
        corrigir,
    );
    assert_eq!(estado.prc.def.itens[0].situacao, SituacaoDoPreco::NoAlvo);
    assert!(estado.prc.def.a_corrigir().is_empty());
}

/// Uma venda faturada de 2 Coca a R$ 100 (custo R$ 50): a receita e a margem sobem pela árvore
/// até o grupo de cima, e o que está só em rascunho não conta.
#[test]
fn aba_rentabilidade_soma_a_venda_faturada_no_grupo_e_nos_de_cima() {
    use cardeal_kernel::Dinheiro;
    use mod_clientes::{CriarPessoa, Papel, PessoaCadastrada, TipoPessoa};
    use mod_vendas::{
        AdicionarItemPedido, ConfirmarPedido, CriarPedido, CriarRegraPreco, CriarTabelaPreco,
        FaturarPedido, PedidoCriado, TabelaPrecoCriada, TipoTabela,
    };

    let t = motor_de_teste();
    let (m, s) = (&t.motor, &t.sessao);
    // Cria o grupo de baixo (REF dentro de BEB) e o produto nele.
    let beb = grupo(m, s, "BEB", None);
    let produto = {
        let un: UnidadeCriada = m
            .executar(
                s,
                "estoque.criar_unidade.v1",
                &CriarUnidade {
                    sigla: "UN".into(),
                    nome: "Un".into(),
                    fracionavel: false,
                },
            )
            .expect("unidade");
        let ref_ = grupo(m, s, "REF", Some(beb));
        let p: ProdutoCriado = m
            .executar(
                s,
                "estoque.criar_produto.v1",
                &CriarProduto {
                    grupo_produto: ref_,
                    nome: "Coca 2L".into(),
                    ncm: "22021000".into(),
                    unidade_padrao: un.unidade,
                    codigo_barras: None,
                    detalhes_tecnicos: None,
                },
            )
            .expect("produto");
        let loc: LocalCriado = m
            .executar(
                s,
                "estoque.criar_local.v1",
                &CriarLocal {
                    nome: "Loja".into(),
                    tipo: TipoLocal::Deposito,
                },
            )
            .expect("local");
        let _: EntradaRegistrada = m
            .executar(
                s,
                "estoque.registrar_entrada.v1",
                &RegistrarEntrada {
                    produto: p.produto,
                    local: loc.local,
                    quantidade: Quantidade::unidades(10),
                    custo_unitario: Preco::reais(50),
                },
            )
            .expect("entrada");
        (p.produto, loc.local, ref_)
    };
    let (produto, local, ref_) = produto;
    let cliente: PessoaCadastrada = m
        .executar(
            s,
            "clientes.criar_pessoa.v1",
            &CriarPessoa {
                tipo: TipoPessoa::Fisica,
                nome: "Cliente".into(),
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
    let tabela: TabelaPrecoCriada = m
        .executar(
            s,
            "vendas.criar_tabela_preco.v1",
            &CriarTabelaPreco {
                nome: "Balcão".into(),
                tipo: TipoTabela::Venda,
                vigente_de: cardeal_kernel::Data::hoje(Fuso::BRASILIA).mais_dias(-1),
                vigente_ate: None,
            },
        )
        .expect("tabela");
    let _: mod_vendas::RegraPrecoCriada = m
        .executar(
            s,
            "vendas.criar_regra_preco.v1",
            &CriarRegraPreco {
                tabela_preco: tabela.tabela_preco,
                alvo_produto: Some(produto),
                alvo_grupo: None,
                quantidade_minima: None,
                preco: Preco::reais(100),
                periodo_de: None,
                periodo_ate: None,
            },
        )
        .expect("regra");
    let pedido = |qtd: i64| -> Id {
        let p: PedidoCriado = m
            .executar(
                s,
                "vendas.criar_pedido.v1",
                &CriarPedido {
                    cliente: cliente.pessoa,
                    vendedor: s.usuario(),
                    condicao_pagamento: Id::novo(),
                    tabela_preco: tabela.tabela_preco,
                    local_expedicao: local,
                },
            )
            .expect("pedido");
        let _: mod_vendas::ItemFoiAdicionado = m
            .executar(
                s,
                "vendas.adicionar_item_pedido.v1",
                &AdicionarItemPedido {
                    pedido: p.pedido,
                    produto,
                    variacao: None,
                    quantidade: Quantidade::unidades(qtd),
                    desconto_percentual: Percentual::ZERO,
                },
            )
            .expect("item");
        p.pedido
    };
    let faturado = pedido(2);
    let _rascunho = pedido(5);
    let _: () = m
        .executar(
            s,
            "vendas.confirmar_pedido.v1",
            &ConfirmarPedido { pedido: faturado },
        )
        .expect("confirmar");
    let _: mod_vendas::PedidoFoiFaturado = m
        .executar(
            s,
            "vendas.faturar_pedido.v1",
            &FaturarPedido {
                pedido: faturado,
                a_vista: true,
            },
        )
        .expect("faturar");

    let mut estado = EstadoTelaEstoque {
        aba: AbaEstoque::Rentabilidade,
        ..Default::default()
    };
    estado.rent.carregar(m, s);
    let r = &estado.rent.resultado;
    assert_eq!(estado.rent.origens, ["Vendas", "PDV", "OS"]);
    assert!(estado.rent.ausentes.is_empty());

    let no = |id: Id| r.nos.iter().find(|n| n.id == id).expect("nó");
    for id in [ref_, beb] {
        assert_eq!(no(id).receita, Dinheiro::reais(200), "só o faturado");
        assert_eq!(no(id).custo, Dinheiro::reais(100));
        assert_eq!(no(id).margem_bruta, Dinheiro::reais(100));
        assert_eq!(no(id).margem_bruta_pct, Some(Percentual::pontos(50)));
        assert_eq!(no(id).produtos, 1);
    }
    assert_eq!(no(beb).receita_propria, Dinheiro::ZERO, "só o filho vendeu");
    assert_eq!(no(ref_).receita_propria, Dinheiro::reais(200));
    assert_eq!(r.total.receita, Dinheiro::reais(200));
    assert_eq!(r.total.itens_sem_custo, 0);
    // A margem líquida é estimada: nunca passa da bruta.
    assert!(r.total.margem_liquida_estimada <= r.total.margem_bruta);

    // Clicar no grupo de cima mostra o produto do de baixo; um grupo sem venda, nenhum.
    estado.rent.selecionar(beb);
    assert_eq!(estado.rent.produtos_visiveis().len(), 1);
    estado.rent.selecionar(beb);
    assert_eq!(estado.rent.selecionado, None);
}

#[test]
fn seletor_em_arvore_do_cadastro_mostra_o_caminho_e_desenha_sem_erro() {
    let t = motor_de_teste();
    let a = grupo(&t.motor, &t.sessao, "A", None);
    let b = grupo(&t.motor, &t.sessao, "B", Some(a));
    let c = grupo(&t.motor, &t.sessao, "C", Some(b));
    let _x = grupo(&t.motor, &t.sessao, "X", None);

    let mut estado = EstadoTelaEstoque::default();
    estado.carregar(&t.motor, &t.sessao);
    let nos = estado.est.nos_arvore();
    assert_eq!(nos.len(), 4);
    assert!(nos[0].tem_filhos && nos[1].tem_filhos && !nos[2].tem_filhos && !nos[3].tem_filhos);
    assert_eq!(
        cardeal_ui::organisms::caminho_do_no(&nos, c).as_deref(),
        Some("A › B › C")
    );

    // O bloco do cadastro, num quadro de verdade do egui, com o grupo já escolhido.
    estado.grupo_selecionado = Some(c);
    let ctx = egui::Context::default();
    cardeal_ui::tokens::instalar_fontes(&ctx);
    cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
    for _ in 0..2 {
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                bloco_grupo(ui, &t.motor, &t.sessao, &mut estado);
            });
        });
    }
    assert_eq!(estado.grupo_selecionado, Some(c));
}

#[test]
fn importar_planilha_mostra_a_previa_e_so_grava_ao_confirmar() {
    use super::estrutura_importacao::{carregar_texto, confirmar};
    let t = motor_de_teste();
    grupo(&t.motor, &t.sessao, "Bebidas", None);
    let mut estado = EstadoTelaEstoque::default();
    estado.carregar(&t.motor, &t.sessao);

    // Planilha com erro: a prévia lista a linha e nada é gravado ao tentar confirmar.
    carregar_texto(
        &t.motor,
        &t.sessao,
        &mut estado.est.imp,
        "ruim.csv",
        "Grupo;Subgrupo\nPeças;;Telas\n".into(),
    )
    .expect("prévia");
    assert!(confirmar(&t.motor, &t.sessao, &mut estado).is_err());
    estado.carregar(&t.motor, &t.sessao);
    assert_eq!(estado.est.nos.len(), 1);

    // Planilha boa: 1 já existe (sem caixa), 3 novos.
    carregar_texto(
        &t.motor,
        &t.sessao,
        &mut estado.est.imp,
        "boa.csv",
        "Grupo;Subgrupo;Categoria\nBEBIDAS;Sucos\nPeças;Telas;LCD\n".into(),
    )
    .expect("prévia");
    // O diálogo desenha a prévia num quadro de verdade do egui.
    estado.dlg = Dlg::ImportarEstrutura;
    let ctx = egui::Context::default();
    cardeal_ui::tokens::instalar_fontes(&ctx);
    cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
    for _ in 0..2 {
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            super::estrutura_importacao::dialogo(ctx, &t.motor, &t.sessao, &mut estado);
        });
    }
    let r = confirmar(&t.motor, &t.sessao, &mut estado).expect("importa");
    assert_eq!((r.criados, r.ja_existiam), (4, 1));
    assert_eq!(estado.est.nos.len(), 5);
}

#[test]
fn importar_produtos_o_modelo_entra_inteiro_e_so_ao_confirmar() {
    use super::produtos_importacao::{carregar_texto, confirmar, MODELO};
    let t = motor_de_teste();
    let mut estado = EstadoTelaEstoque::default();
    estado.carregar(&t.motor, &t.sessao);
    if estado.locais.is_empty() {
        let _: LocalCriado = t
            .motor
            .executar(
                &t.sessao,
                "estoque.criar_local.v1",
                &CriarLocal {
                    nome: "Loja".into(),
                    tipo: TipoLocal::Loja,
                },
            )
            .expect("local");
        estado.carregar(&t.motor, &t.sessao);
    }
    assert_eq!(estado.locais.len(), 1, "o modelo não traz a coluna Local");
    let antes = estado.produtos.len();

    // Planilha com erro: a prévia lista a linha e confirmar não grava nada.
    carregar_texto(
        &t.motor,
        &t.sessao,
        &mut estado.imp_produtos,
        "ruim.csv",
        "Nome;Estoque\nCabo;3\n".into(),
    )
    .expect("prévia");
    let previa = estado.imp_produtos.previa.as_ref().unwrap();
    assert_eq!(previa.erros.len(), 1);
    assert!(confirmar(&t.motor, &t.sessao, &mut estado).is_err());
    estado.carregar(&t.motor, &t.sessao);
    assert_eq!(estado.produtos.len(), antes);

    // O modelo que "Baixar modelo…" salva é uma planilha válida.
    carregar_texto(
        &t.motor,
        &t.sessao,
        &mut estado.imp_produtos,
        "modelo-produtos.csv",
        MODELO.into(),
    )
    .expect("prévia");
    let previa = estado.imp_produtos.previa.as_ref().unwrap();
    assert!(previa.erros.is_empty(), "{:?}", previa.erros);
    assert_eq!((previa.novos, previa.com_estoque), (2, 2));

    // O diálogo desenha a prévia num quadro de verdade do egui.
    estado.dlg = Dlg::ImportarProdutos;
    let ctx = egui::Context::default();
    cardeal_ui::tokens::instalar_fontes(&ctx);
    cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
    for _ in 0..2 {
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            super::produtos_importacao::dialogo(ctx, &t.motor, &t.sessao, &mut estado);
        });
    }
    let r = confirmar(&t.motor, &t.sessao, &mut estado).expect("importa");
    assert_eq!((r.criados, r.com_estoque), (2, 2));
    assert_eq!(estado.produtos.len(), antes + 2);
    let refri = estado
        .produtos
        .iter()
        .find(|p| p.nome == "Refrigerante Cola 2L")
        .unwrap();
    assert_eq!(refri.disponivel, Quantidade::unidades(24));
    assert!(
        estado.imp_produtos.previa.is_none(),
        "o diálogo volta limpo"
    );
}

#[test]
fn comissao_do_grupo_aparece_herdada_no_subgrupo_e_na_grade_de_precos() {
    use mod_estoque::{DefinirComissaoDoGrupo, OrigemComissao};
    let t = motor_de_teste();
    let a = grupo(&t.motor, &t.sessao, "A", None);
    let b = grupo(&t.motor, &t.sessao, "B", Some(a));
    t.motor
        .executar(
            &t.sessao,
            "estoque.definir_comissao_do_grupo.v1",
            &DefinirComissaoDoGrupo {
                grupo: a,
                comissao: Some(Percentual::pontos(4)),
            },
        )
        .expect("comissão");

    let mut estado = EstadoTelaEstoque::default();
    estado.carregar(&t.motor, &t.sessao);
    let (c, de) = estado.est.comissao_herdada(b).expect("herdada");
    assert_eq!((c, de.as_str()), (Percentual::pontos(4), "A"));
    assert!(estado.est.comissao_herdada(a).is_none());
    estado.est.selecionar(a);
    assert_eq!(estado.est.comissao, "4,00", "o formulário mostra a própria");

    // O detalhe do grupo desenha os blocos de margem e comissão num quadro de verdade.
    estado.aba = AbaEstoque::Estrutura;
    let ctx = egui::Context::default();
    cardeal_ui::tokens::instalar_fontes(&ctx);
    cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
    for _ in 0..2 {
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                mostrar(ui, &t.motor, &t.sessao, &mut estado);
            });
        });
    }

    // Um produto em B herda a comissão de A no preço sugerido.
    let un: UnidadeCriada = t
        .motor
        .executar(
            &t.sessao,
            "estoque.criar_unidade.v1",
            &CriarUnidade {
                sigla: "UN".into(),
                nome: "Unidade".into(),
                fracionavel: false,
            },
        )
        .or_else(|_| {
            let u: Vec<ItemUnidade> = t
                .motor
                .consultar(&t.sessao, "estoque.unidades.v1", &Unidades)
                .expect("unidades");
            Ok::<_, cardeal_kernel::Erro>(UnidadeCriada { unidade: u[0].id })
        })
        .expect("unidade");
    let _: ProdutoCriado = t
        .motor
        .executar(
            &t.sessao,
            "estoque.criar_produto.v1",
            &CriarProduto {
                grupo_produto: b,
                nome: "Produto em B".into(),
                ncm: String::new(),
                unidade_padrao: un.unidade,
                codigo_barras: None,
                detalhes_tecnicos: None,
            },
        )
        .expect("produto");
    estado.prc.carregar(&t.motor, &t.sessao);
    let s = estado
        .prc
        .sugestoes
        .iter()
        .find(|s| s.nome == "Produto em B")
        .expect("na grade de preços");
    assert_eq!(s.comissao, Percentual::pontos(4));
    assert_eq!(s.origem_comissao, OrigemComissao::Grupo("A".into()));
}

#[test]
fn inventario_na_tela_e_cego_ate_encerrar_e_mostra_as_diferencas_depois() {
    use mod_estoque::{
        CriarInventario, EncerrarInventario, EstadoInventario, IniciarContagem, InventarioCriado,
    };
    let t = motor_de_teste();
    let g = grupo(&t.motor, &t.sessao, "Mercearia", None);
    let un: UnidadeCriada = t
        .motor
        .executar(
            &t.sessao,
            "estoque.criar_unidade.v1",
            &CriarUnidade {
                sigla: "UN".into(),
                nome: "Unidade".into(),
                fracionavel: false,
            },
        )
        .or_else(|_| {
            let u: Vec<ItemUnidade> = t
                .motor
                .consultar(&t.sessao, "estoque.unidades.v1", &Unidades)
                .expect("unidades");
            Ok::<_, cardeal_kernel::Erro>(UnidadeCriada { unidade: u[0].id })
        })
        .expect("unidade");
    let p: ProdutoCriado = t
        .motor
        .executar(
            &t.sessao,
            "estoque.criar_produto.v1",
            &CriarProduto {
                grupo_produto: g,
                nome: "Arroz 5kg".into(),
                ncm: String::new(),
                unidade_padrao: un.unidade,
                codigo_barras: None,
                detalhes_tecnicos: None,
            },
        )
        .expect("produto");
    let loc: LocalCriado = t
        .motor
        .executar(
            &t.sessao,
            "estoque.criar_local.v1",
            &CriarLocal {
                nome: "Loja".into(),
                tipo: TipoLocal::Loja,
            },
        )
        .expect("local");
    let _: EntradaRegistrada = t
        .motor
        .executar(
            &t.sessao,
            "estoque.registrar_entrada.v1",
            &RegistrarEntrada {
                produto: p.produto,
                local: loc.local,
                quantidade: Quantidade::unidades(10),
                custo_unitario: Preco::reais(20),
            },
        )
        .expect("entrada");
    let inv: InventarioCriado = t
        .motor
        .executar(
            &t.sessao,
            "estoque.criar_inventario.v1",
            &CriarInventario {
                local: loc.local,
                grupo: None,
                descricao: "Balanço".into(),
            },
        )
        .expect("inventário");

    let ctx = egui::Context::default();
    cardeal_ui::tokens::instalar_fontes(&ctx);
    cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
    let mut estado = EstadoTelaEstoque::default();
    estado.carregar(&t.motor, &t.sessao);
    estado.aba = AbaEstoque::Inventario;
    estado.inv.carregar(&t.motor, &t.sessao);
    assert_eq!(
        estado.inv.selecionado,
        Some(inv.inventario),
        "abre no aberto"
    );
    let desenhar = |estado: &mut EstadoTelaEstoque| {
        for _ in 0..2 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    mostrar(ui, &t.motor, &t.sessao, estado);
                });
            });
        }
    };
    desenhar(&mut estado);

    let _: mod_estoque::ContagemIniciada = t
        .motor
        .executar(
            &t.sessao,
            "estoque.iniciar_contagem.v1",
            &IniciarContagem {
                inventario: inv.inventario,
            },
        )
        .expect("iniciar");
    estado.inv.carregar(&t.motor, &t.sessao);
    desenhar(&mut estado);

    // Registrar pela tela: produto escolhido, quantidade digitada.
    estado.inv.produto = Some(p.produto);
    estado.inv.quantidade = "7".into();
    super::inventario::registrar(&ctx, &t.motor, &t.sessao, &mut estado, inv.inventario);
    let pos = estado.inv.posicao.clone().expect("posição");
    assert_eq!(pos.resumo.contados, 1);
    assert_eq!(pos.itens[0].contada, Some(Quantidade::unidades(7)));
    assert!(pos.itens[0].sistema.is_none(), "cega durante a contagem");
    assert!(estado.inv.produto.is_none(), "o formulário volta limpo");
    desenhar(&mut estado);

    let _: mod_estoque::InventarioEncerradoResumo = t
        .motor
        .executar(
            &t.sessao,
            "estoque.encerrar_inventario.v1",
            &EncerrarInventario {
                inventario: inv.inventario,
            },
        )
        .expect("encerrar");
    estado.inv.carregar(&t.motor, &t.sessao);
    let pos = estado.inv.posicao.clone().expect("posição");
    assert_eq!(pos.resumo.estado, EstadoInventario::Encerrado);
    assert_eq!(pos.itens[0].sistema, Some(Quantidade::unidades(10)));
    assert_eq!(pos.itens[0].diferenca, Some(Quantidade::unidades(-3)));
    assert_eq!(
        pos.itens[0].valor_ajuste,
        Some(cardeal_kernel::Dinheiro::reais(60))
    );
    desenhar(&mut estado);
}

#[test]
fn transferencia_na_tela_monta_a_lista_soma_repetido_e_grava() {
    let t = motor_de_teste();
    let g = grupo(&t.motor, &t.sessao, "Mercearia", None);
    let un: UnidadeCriada = t
        .motor
        .executar(
            &t.sessao,
            "estoque.criar_unidade.v1",
            &CriarUnidade {
                sigla: "UN".into(),
                nome: "Unidade".into(),
                fracionavel: false,
            },
        )
        .or_else(|_| {
            let u: Vec<ItemUnidade> = t
                .motor
                .consultar(&t.sessao, "estoque.unidades.v1", &Unidades)
                .expect("unidades");
            Ok::<_, cardeal_kernel::Erro>(UnidadeCriada { unidade: u[0].id })
        })
        .expect("unidade");
    let p: ProdutoCriado = t
        .motor
        .executar(
            &t.sessao,
            "estoque.criar_produto.v1",
            &CriarProduto {
                grupo_produto: g,
                nome: "Arroz 5kg".into(),
                ncm: String::new(),
                unidade_padrao: un.unidade,
                codigo_barras: None,
                detalhes_tecnicos: None,
            },
        )
        .expect("produto");
    let local = |nome: &str, tipo| -> Id {
        let l: LocalCriado = t
            .motor
            .executar(
                &t.sessao,
                "estoque.criar_local.v1",
                &CriarLocal {
                    nome: nome.into(),
                    tipo,
                },
            )
            .expect("local");
        l.local
    };
    let deposito = local("Depósito", TipoLocal::Deposito);
    let loja = local("Loja", TipoLocal::Loja);
    let _: EntradaRegistrada = t
        .motor
        .executar(
            &t.sessao,
            "estoque.registrar_entrada.v1",
            &RegistrarEntrada {
                produto: p.produto,
                local: deposito,
                quantidade: Quantidade::unidades(10),
                custo_unitario: Preco::reais(20),
            },
        )
        .expect("entrada");

    let mut estado = EstadoTelaEstoque::default();
    estado.carregar(&t.motor, &t.sessao);
    estado.aba = AbaEstoque::Transferencias;
    estado.transf.carregar(&t.motor, &t.sessao);
    estado.transf.origem = Some(deposito);
    estado.transf.destino = Some(loja);
    // Quantidade inválida não entra; o mesmo produto duas vezes soma.
    estado.transf.produto = Some(p.produto);
    estado.transf.quantidade = "0".into();
    assert!(estado.transf.adicionar("Arroz 5kg").is_err());
    estado.transf.quantidade = "2".into();
    estado.transf.adicionar("Arroz 5kg").expect("adiciona");
    estado.transf.produto = Some(p.produto);
    estado.transf.quantidade = "1".into();
    estado.transf.adicionar("Arroz 5kg").expect("soma");
    assert_eq!(estado.transf.itens.len(), 1);
    assert_eq!(estado.transf.itens[0].quantidade, Quantidade::unidades(3));

    // A aba desenha num quadro de verdade, com o produto escolhido (o disponível por local).
    estado.transf.produto = Some(p.produto);
    let ctx = egui::Context::default();
    cardeal_ui::tokens::instalar_fontes(&ctx);
    cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
    for _ in 0..2 {
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                mostrar(ui, &t.motor, &t.sessao, &mut estado);
            });
        });
    }

    let r = super::transferencia::transferir(&t.motor, &t.sessao, &mut estado).expect("transfere");
    assert_eq!(r.itens, 1);
    assert_eq!(r.valor, cardeal_kernel::Dinheiro::reais(60));
    assert!(estado.transf.itens.is_empty(), "a lista volta vazia");
    assert_eq!(estado.transf.historico.len(), 1);
    let disponivel: Quantidade = t
        .motor
        .consultar(
            &t.sessao,
            "estoque.saldo_disponivel_do_produto.v1",
            &mod_estoque::SaldoDisponivelDoProduto { produto: p.produto },
        )
        .expect("saldo");
    assert_eq!(
        disponivel,
        Quantidade::unidades(10),
        "o total do CNPJ não muda"
    );
}

#[test]
fn dicionario_de_nomes_adiciona_testa_e_remove() {
    let t = motor_de_teste();
    let mut estado = EstadoTelaEstoque::default();
    estado.carregar(&t.motor, &t.sessao);
    estado.dic.carregar(&t.motor, &t.sessao);
    assert!(estado.dic.termos.is_empty());

    estado.dic.de = "REFRIG".into();
    estado.dic.para = "Refrigerante".into();
    super::dicionario::adicionar(&t.motor, &t.sessao, &mut estado.dic).expect("adiciona");
    estado.dic.de = "pet".into();
    estado.dic.para = String::new();
    super::dicionario::adicionar(&t.motor, &t.sessao, &mut estado.dic).expect("remove palavra");
    assert_eq!(estado.dic.termos.len(), 2);

    estado.dic.teste = "REFRIG COLA PET 2000ML".into();
    super::dicionario::testar(&t.motor, &t.sessao, &mut estado.dic);
    assert_eq!(
        estado.dic.resultado_teste.as_deref(),
        Some("Refrigerante Cola 2Lt")
    );

    // O diálogo desenha num quadro de verdade.
    estado.dlg = Dlg::Dicionario;
    let ctx = egui::Context::default();
    cardeal_ui::tokens::instalar_fontes(&ctx);
    cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
    for _ in 0..2 {
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            super::dicionario::dialogo(ctx, &t.motor, &t.sessao, &mut estado);
        });
    }

    let () = t
        .motor
        .executar(
            &t.sessao,
            "estoque.remover_termo_do_dicionario.v1",
            &mod_estoque::RemoverTermoDoDicionario { de: "pet".into() },
        )
        .expect("remove");
    estado.dic.carregar(&t.motor, &t.sessao);
    assert_eq!(estado.dic.termos.len(), 1);
}

#[test]
fn validade_na_tela_entrada_alerta_e_baixa() {
    use cardeal_kernel::{Data, Fuso};
    let t = motor_de_teste();
    let g = grupo(&t.motor, &t.sessao, "Laticínios", None);
    let un: UnidadeCriada = t
        .motor
        .executar(
            &t.sessao,
            "estoque.criar_unidade.v1",
            &CriarUnidade {
                sigla: "UN".into(),
                nome: "Unidade".into(),
                fracionavel: false,
            },
        )
        .or_else(|_| {
            let u: Vec<ItemUnidade> = t
                .motor
                .consultar(&t.sessao, "estoque.unidades.v1", &Unidades)
                .expect("unidades");
            Ok::<_, cardeal_kernel::Erro>(UnidadeCriada { unidade: u[0].id })
        })
        .expect("unidade");
    let p: ProdutoCriado = t
        .motor
        .executar(
            &t.sessao,
            "estoque.criar_produto.v1",
            &CriarProduto {
                grupo_produto: g,
                nome: "Iogurte Natural".into(),
                ncm: String::new(),
                unidade_padrao: un.unidade,
                codigo_barras: None,
                detalhes_tecnicos: None,
            },
        )
        .expect("produto");
    let loc: LocalCriado = t
        .motor
        .executar(
            &t.sessao,
            "estoque.criar_local.v1",
            &CriarLocal {
                nome: "Loja".into(),
                tipo: TipoLocal::Loja,
            },
        )
        .expect("local");

    let mut estado = EstadoTelaEstoque::default();
    estado.carregar(&t.motor, &t.sessao);
    estado.aba = AbaEstoque::Validade;
    estado.val.carregar(&t.motor, &t.sessao);

    // Entrada pela tela: validade de ontem, código em branco (gerado).
    let ontem = Data::hoje(Fuso::BRASILIA).mais_dias(-1);
    estado.val.ent_produto = Some(p.produto);
    estado.val.ent_local = Some(loc.local);
    estado.val.ent_qtd = "12".into();
    estado.val.ent_custo = "2,50".into();
    estado.val.ent_validade = "abc".into();
    assert!(super::validade::registrar_entrada(&t.motor, &t.sessao, &mut estado).is_err());
    estado.val.ent_validade = ontem.formatar();
    super::validade::registrar_entrada(&t.motor, &t.sessao, &mut estado).expect("entrada");
    assert_eq!(estado.val.lotes.len(), 1);
    assert!(estado.val.lotes[0].vencido());
    assert_eq!(
        estado.lotes_em_alerta, 1,
        "o alerta aparece na aba Produtos"
    );

    // A aba desenha num quadro de verdade, com a baixa aberta.
    estado.val.abrir_baixa(0);
    assert_eq!(
        estado.val.baixa_motivo,
        Some(mod_estoque::MotivoPerda::Vencimento)
    );
    let ctx = egui::Context::default();
    cardeal_ui::tokens::instalar_fontes(&ctx);
    cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
    for _ in 0..2 {
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                mostrar(ui, &t.motor, &t.sessao, &mut estado);
            });
        });
    }

    let r = super::validade::dar_baixa(&t.motor, &t.sessao, &mut estado).expect("baixa");
    assert!(r.lote_esgotado);
    assert_eq!(r.valor, cardeal_kernel::Dinheiro::reais(30));
    assert!(estado.val.lotes.is_empty());
    assert_eq!(estado.lotes_em_alerta, 0);
}
