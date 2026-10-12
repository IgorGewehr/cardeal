//! A análise de estoque (ABC, cobertura, giro) lida dos movimentos reais, pelo `Despachante`.

#![allow(clippy::result_large_err)] // `ErroArmazenamento` carrega detalhes de propósito

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel as PapelAuth, Sessao};
use cardeal_kernel::{Data, Dinheiro, Fuso, Id, Instante, Periodo, Preco, Quantidade};
use cardeal_ledger::semear_plano_padrao;
use cardeal_modkit::{Ambiente, Despachante, Modulo, PedidoAtivacao, RegistroModulos};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use mod_estoque::analise::{ClasseAbc, ItemDeAnalise};
use mod_estoque::{
    AnaliseDeEstoque, CriarGrupoProduto, CriarLocal, CriarProduto, CriarUnidade, EncargosAtuais,
    GrupoProdutoCriado, LocalCriado, ModuloEstoque, ProdutoCriado, RegistrarEntrada,
    RegistrarSaida, SaidaDoProduto, SaidasPorProdutoEOrigem, TipoLocal, UnidadeCriada, MANIFESTO,
};
use tempfile::TempDir;

fn base() -> (TempDir, Armazenamento, Id) {
    let dir = tempfile::tempdir().unwrap();
    let arm =
        Armazenamento::abrir(ConfigArmazenamento::arquivo(dir.path().join("cardeal.db"))).unwrap();
    arm.migrar(&[
        cardeal_ledger::migracoes::conjunto(),
        ModuloEstoque.migracoes(),
    ])
    .unwrap();

    let empresa = Id::novo();
    let ctx = ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo());
    arm.escritor()
        .executar(ctx, move |uow| {
            uow.conexao()
                .execute(
                    "INSERT INTO nucleo_empresa
                       (id, razao_social, nome_fantasia, cnpj, regime, endereco, perfil, criado_em)
                     VALUES (?1,'Teste LTDA','Teste','11222333000181','SimplesNacional','{}','comercio',0)",
                    [empresa.em_bytes().as_slice()],
                )
                .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?;
            semear_plano_padrao(uow, empresa)
                .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
        })
        .unwrap();

    (dir, arm, empresa)
}

fn ambiente(empresa: Id) -> Ambiente {
    let mut rm = RegistroModulos::novo();
    rm.registrar(&MANIFESTO).unwrap();
    let efetivo = rm
        .resolver(&PedidoAtivacao::nova().com_modulo("estoque"))
        .unwrap();
    Ambiente::novo(empresa, efetivo)
}

fn sessao(empresa: Id, permissoes: &[&str]) -> Sessao {
    let mut papel = PapelAuth::novo(empresa, "Testador");
    for p in permissoes {
        papel = papel.com_permissao(*p);
    }
    Sessao::abrir(EmissaoSessao::padrao(
        Id::novo(),
        Id::novo(),
        Escopo::empresa_inteira(empresa),
        AutorizacoesEfetivas::consolidar([&papel]),
        Instante::EPOCA,
    ))
}

fn carga(v: &impl serde::Serialize) -> Vec<u8> {
    postcard::to_stdvec(v).unwrap()
}

#[test]
fn analise_classifica_pelo_que_saiu_e_mede_a_cobertura() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloEstoque]).unwrap();
    let s = sessao(
        empresa,
        &[
            "estoque.produto.criar",
            "estoque.produto.ver",
            "estoque.local.criar",
            "estoque.movimento.entrada",
            "estoque.movimento.saida",
            "estoque.abc.ver",
        ],
    );
    let amb = ambiente(empresa);
    let cmd = |nome: &str, c: &dyn Fn() -> Vec<u8>| {
        d.executar_comando(nome, &c(), &s, &amb, arm.escritor())
            .unwrap()
    };
    let g: GrupoProdutoCriado =
        postcard::from_bytes(&cmd("estoque.criar_grupo_produto.v1", &|| {
            carga(&CriarGrupoProduto {
                codigo: "G".into(),
                nome: "G".into(),
                pai: None,
            })
        }))
        .unwrap();
    let u: UnidadeCriada = postcard::from_bytes(&cmd("estoque.criar_unidade.v1", &|| {
        carga(&CriarUnidade {
            sigla: "UN".into(),
            nome: "Un".into(),
            fracionavel: false,
        })
    }))
    .unwrap();
    let l: LocalCriado = postcard::from_bytes(&cmd("estoque.criar_local.v1", &|| {
        carga(&CriarLocal {
            nome: "Dep".into(),
            tipo: TipoLocal::Deposito,
        })
    }))
    .unwrap();
    let produto = |nome: &str, entrada: i64, saida: i64| -> Id {
        let p: ProdutoCriado = postcard::from_bytes(&cmd("estoque.criar_produto.v1", &|| {
            carga(&CriarProduto {
                grupo_produto: g.grupo_produto,
                nome: nome.into(),
                ncm: "22021000".into(),
                unidade_padrao: u.unidade,
                codigo_barras: None,
                detalhes_tecnicos: None,
            })
        }))
        .unwrap();
        cmd("estoque.registrar_entrada.v1", &|| {
            carga(&RegistrarEntrada {
                produto: p.produto,
                local: l.local,
                quantidade: Quantidade::unidades(entrada),
                custo_unitario: Preco::reais(10),
            })
        });
        if saida > 0 {
            cmd("estoque.registrar_saida.v1", &|| {
                carga(&RegistrarSaida {
                    produto: p.produto,
                    local: l.local,
                    quantidade: Quantidade::unidades(saida),
                })
            });
        }
        p.produto
    };
    // Saídas ao custo de R$ 10: A=900, B=50, C=40, parado=0 → total 990.
    let a = produto("Campeão", 100, 90);
    let b = produto("Médio", 100, 5);
    let c = produto("Cauda", 100, 4);
    let p = produto("Encalhado", 40, 0);

    let saida = d
        .executar_consulta(
            "estoque.analise_de_estoque.v1",
            &carga(&AnaliseDeEstoque { dias: 30 }),
            &s,
            &amb,
            arm.leitor(),
        )
        .unwrap();
    let itens: Vec<ItemDeAnalise> = postcard::from_bytes(&saida).unwrap();
    let item = |id: Id| itens.iter().find(|i| i.produto == id).unwrap();
    assert_eq!(item(a).classe, ClasseAbc::A);
    assert_eq!(item(b).classe, ClasseAbc::B);
    assert_eq!(item(c).classe, ClasseAbc::C);
    assert_eq!(item(p).classe, ClasseAbc::SemGiro);
    assert_eq!(item(a).valor_saidas, Dinheiro::reais(900));
    assert_eq!(item(a).participacao.formatar(0), "90");
    // Campeão: saldo 10, saiu 90 em 30 dias (3/dia) → 3 dias de cobertura: risco de ruptura.
    assert_eq!(item(a).cobertura_dias, Some(3));
    assert!(item(a).risco_de_ruptura);
    // Encalhado: R$ 400 parados, sem cobertura nem giro.
    assert_eq!(item(p).valor_em_estoque, Dinheiro::reais(400));
    assert_eq!(item(p).cobertura_dias, None);
    // Ordem da curva: o maior primeiro, o sem giro por último.
    assert_eq!(itens.first().unwrap().produto, a);
    assert_eq!(itens.last().unwrap().produto, p);
}

#[test]
fn saidas_por_produto_e_origem_somam_o_custo_e_usam_o_medio_quando_falta() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloEstoque]).unwrap();
    let s = sessao(
        empresa,
        &[
            "estoque.produto.criar",
            "estoque.produto.ver",
            "estoque.local.criar",
            "estoque.movimento.entrada",
            "estoque.movimento.saida",
            "estoque.abc.ver",
        ],
    );
    let amb = ambiente(empresa);
    let cmd = |nome: &str, c: Vec<u8>| {
        d.executar_comando(nome, &c, &s, &amb, arm.escritor())
            .unwrap()
    };
    let g: GrupoProdutoCriado = postcard::from_bytes(&cmd(
        "estoque.criar_grupo_produto.v1",
        carga(&CriarGrupoProduto {
            codigo: "G".into(),
            nome: "G".into(),
            pai: None,
        }),
    ))
    .unwrap();
    let u: UnidadeCriada = postcard::from_bytes(&cmd(
        "estoque.criar_unidade.v1",
        carga(&CriarUnidade {
            sigla: "UN".into(),
            nome: "Un".into(),
            fracionavel: false,
        }),
    ))
    .unwrap();
    let l: LocalCriado = postcard::from_bytes(&cmd(
        "estoque.criar_local.v1",
        carga(&CriarLocal {
            nome: "Dep".into(),
            tipo: TipoLocal::Deposito,
        }),
    ))
    .unwrap();
    let p: ProdutoCriado = postcard::from_bytes(&cmd(
        "estoque.criar_produto.v1",
        carga(&CriarProduto {
            grupo_produto: g.grupo_produto,
            nome: "Cabo".into(),
            ncm: "22021000".into(),
            unidade_padrao: u.unidade,
            codigo_barras: None,
            detalhes_tecnicos: None,
        }),
    ))
    .unwrap();
    cmd(
        "estoque.registrar_entrada.v1",
        carga(&RegistrarEntrada {
            produto: p.produto,
            local: l.local,
            quantidade: Quantidade::unidades(100),
            custo_unitario: Preco::reais(10),
        }),
    );
    for qtd in [3, 2, 4] {
        cmd(
            "estoque.registrar_saida.v1",
            carga(&RegistrarSaida {
                produto: p.produto,
                local: l.local,
                quantidade: Quantidade::unidades(qtd),
            }),
        );
    }
    // A saída de 4 vira uma venda do PDV sem custo gravado (cai no custo médio de R$ 10).
    arm.escritor()
        .executar(
            ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo()),
            |uow| {
                uow.conexao()
                    .execute(
                        "UPDATE estoque_movimento SET origem_modulo = 'pdv', custo_unitario = NULL
                         WHERE tipo = 'Saida' AND quantidade = 40000",
                        [],
                    )
                    .map(|_| ())
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            },
        )
        .unwrap();

    let saidas = |periodo: Periodo| -> Vec<SaidaDoProduto> {
        postcard::from_bytes(
            &d.executar_consulta(
                "estoque.saidas_por_produto_e_origem.v1",
                &carga(&SaidasPorProdutoEOrigem { periodo }),
                &s,
                &amb,
                arm.leitor(),
            )
            .unwrap(),
        )
        .unwrap()
    };
    let hoje = Data::hoje(Fuso::BRASILIA);
    let r = saidas(Periodo::novo(hoje.mais_dias(-1), hoje.mais_dias(1)));
    assert_eq!(r.len(), 2, "uma linha por origem; a entrada não conta");
    let origem = |nome: &str| r.iter().find(|x| x.origem_modulo == nome).unwrap();
    assert_eq!(origem("estoque").quantidade, Quantidade::unidades(5));
    assert_eq!(origem("estoque").custo, Dinheiro::reais(50));
    assert_eq!(origem("pdv").quantidade, Quantidade::unidades(4));
    assert_eq!(
        origem("pdv").custo,
        Dinheiro::reais(40),
        "custo médio atual"
    );
    assert!(origem("pdv").produto == p.produto);
    assert!(saidas(Periodo::novo(hoje.mais_dias(-30), hoje.mais_dias(-10))).is_empty());

    let e: mod_estoque::EncargosDaEmpresa = postcard::from_bytes(
        &d.executar_consulta(
            "estoque.encargos_atuais.v1",
            &carga(&EncargosAtuais),
            &s,
            &amb,
            arm.leitor(),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(e.total() >= e.imposto);
}
