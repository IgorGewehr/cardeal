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
