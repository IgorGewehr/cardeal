//! A alteração em lote do catálogo (prévia, aplicar, histórico, desfazer) pelo `Despachante` real.

#![allow(clippy::result_large_err)] // `ErroArmazenamento` carrega detalhes de propósito

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel as PapelAuth, Sessao};
use cardeal_kernel::{CodigoErro, Erro, Id, Instante, Percentual};
use cardeal_ledger::semear_plano_padrao;
use cardeal_modkit::{Ambiente, Despachante, Modulo, PedidoAtivacao, RegistroModulos};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use mod_estoque::alteracao_em_lote::{Alteracao, FiltroDeLote, Mudanca, MudancaFiscal};
use mod_estoque::fiscal::CamposFiscais;
use mod_estoque::{
    AlteracaoAplicada, AlteracaoDesfeita, AlteracaoRegistrada, AplicarAlteracaoEmLote,
    CriarGrupoProduto, CriarProduto, CriarUnidade, DefinirMargemDoProduto, DesfazerAlteracaoEmLote,
    GrupoProdutoCriado, HistoricoDeAlteracoesEmLote, LinhaDaPrevia, ModuloEstoque,
    PreviaDaAlteracaoEmLote, ProdutoCriado, ProdutoParaLote, ProdutosParaLote, UnidadeCriada,
    MANIFESTO,
};
use mod_estoque::{
    DefinirCamposFiscaisDoGrupo, DefinirCamposFiscaisDoProduto, PerfilFiscalDoProduto,
    PerfilFiscalEfetivo,
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

const PERMISSOES: &[&str] = &[
    "estoque.produto.criar",
    "estoque.produto.editar",
    "estoque.produto.ver",
    "estoque.tributario.editar",
];

struct Cenario {
    _dir: TempDir,
    arm: Armazenamento,
    empresa: Id,
    d: Despachante,
    s: Sessao,
    unidade: Id,
}

impl Cenario {
    fn novo() -> Self {
        let (dir, arm, empresa) = base();
        let d = Despachante::construir(&[&ModuloEstoque]).unwrap();
        let s = sessao(empresa, PERMISSOES);
        let un: UnidadeCriada = postcard::from_bytes(
            &d.executar_comando(
                "estoque.criar_unidade.v1",
                &carga(&CriarUnidade {
                    sigla: "UN".into(),
                    nome: "Un".into(),
                    fracionavel: false,
                }),
                &s,
                &ambiente(empresa),
                arm.escritor(),
            )
            .unwrap(),
        )
        .unwrap();
        Self {
            _dir: dir,
            arm,
            empresa,
            d,
            s,
            unidade: un.unidade,
        }
    }

    fn cmd<T: serde::de::DeserializeOwned>(
        &self,
        nome: &str,
        c: &impl serde::Serialize,
    ) -> Result<T, Erro> {
        self.d
            .executar_comando(
                nome,
                &carga(c),
                &self.s,
                &ambiente(self.empresa),
                self.arm.escritor(),
            )
            .map(|b| postcard::from_bytes(&b).unwrap())
    }

    fn consulta<T: serde::de::DeserializeOwned>(&self, nome: &str, c: &impl serde::Serialize) -> T {
        let b = self
            .d
            .executar_consulta(
                nome,
                &carga(c),
                &self.s,
                &ambiente(self.empresa),
                self.arm.leitor(),
            )
            .unwrap();
        postcard::from_bytes(&b).unwrap()
    }

    fn grupo(&self, codigo: &str) -> Id {
        self.cmd::<GrupoProdutoCriado>(
            "estoque.criar_grupo_produto.v1",
            &CriarGrupoProduto {
                codigo: codigo.into(),
                nome: codigo.into(),
                pai: None,
            },
        )
        .unwrap()
        .grupo_produto
    }

    fn produto(&self, nome: &str, grupo: Id) -> Id {
        self.cmd::<ProdutoCriado>(
            "estoque.criar_produto.v1",
            &CriarProduto {
                grupo_produto: grupo,
                nome: nome.into(),
                ncm: "22021000".into(),
                unidade_padrao: self.unidade,
                codigo_barras: None,
                detalhes_tecnicos: None,
            },
        )
        .unwrap()
        .produto
    }

    fn todos(&self) -> Vec<ProdutoParaLote> {
        self.consulta(
            "estoque.produtos_para_lote.v1",
            &ProdutosParaLote::default(),
        )
    }

    fn do_produto(&self, id: Id) -> ProdutoParaLote {
        self.todos().into_iter().find(|p| p.produto == id).unwrap()
    }

    fn aplicar(&self, produtos: &[Id], alteracao: Alteracao) -> Result<AlteracaoAplicada, Erro> {
        self.cmd(
            "estoque.aplicar_alteracao_em_lote.v1",
            &AplicarAlteracaoEmLote {
                descricao: "teste".into(),
                produtos: produtos.to_vec(),
                alteracao,
            },
        )
    }

    fn historico(&self) -> Vec<AlteracaoRegistrada> {
        self.consulta(
            "estoque.historico_de_alteracoes_em_lote.v1",
            &HistoricoDeAlteracoesEmLote,
        )
    }
}

fn margem(pontos: i64) -> Alteracao {
    Alteracao {
        margem: Mudanca::Definir(Some(Percentual::pontos(pontos))),
        ..Alteracao::default()
    }
}

#[test]
fn previa_mostra_antes_e_depois_sem_gravar_nada() {
    let c = Cenario::novo();
    let g = c.grupo("BEB");
    let (a, b) = (c.produto("Coca", g), c.produto("Guaraná", g));
    let previa: Vec<LinhaDaPrevia> = c.consulta(
        "estoque.previa_da_alteracao_em_lote.v1",
        &PreviaDaAlteracaoEmLote {
            produtos: vec![a, b],
            alteracao: margem(25),
        },
    );
    assert_eq!(previa.len(), 2);
    assert!(previa.iter().all(|l| l.muda()));
    assert_eq!(previa[0].antes.margem, None);
    assert_eq!(previa[0].depois.margem, Some(Percentual::pontos(25)));
    assert_eq!(c.do_produto(a).margem, None, "prévia não grava");
    assert!(c.historico().is_empty());
}

#[test]
fn aplicar_muda_so_o_pedido_e_desfazer_restaura() {
    let c = Cenario::novo();
    let g = c.grupo("BEB");
    let outro = c.grupo("ALI");
    let (a, b, d) = (
        c.produto("Coca", g),
        c.produto("Guaraná", g),
        c.produto("Arroz", outro),
    );
    // O Arroz já tinha 10% próprios: ficam como estão porque não entrou na seleção.
    let _: () = c
        .cmd(
            "estoque.definir_margem_do_produto.v1",
            &DefinirMargemDoProduto {
                produto: d,
                margem: Some(Percentual::pontos(10)),
            },
        )
        .unwrap();

    let r = c
        .aplicar(
            &[a, b],
            Alteracao {
                margem: Mudanca::Definir(Some(Percentual::pontos(25))),
                grupo: Mudanca::Definir(outro),
                ..Alteracao::default()
            },
        )
        .unwrap();
    assert_eq!((r.alterados, r.sem_mudanca), (2, 0));
    for id in [a, b] {
        let p = c.do_produto(id);
        assert_eq!((p.margem, p.grupo), (Some(Percentual::pontos(25)), outro));
    }
    assert_eq!(c.do_produto(d).margem, Some(Percentual::pontos(10)));
    let h = c.historico();
    assert_eq!(h.len(), 1);
    assert_eq!(
        (h[0].campos.as_str(), h[0].quantidade, h[0].pode_desfazer),
        ("margem, grupo", 2, true)
    );

    let u: AlteracaoDesfeita = c
        .cmd(
            "estoque.desfazer_alteracao_em_lote.v1",
            &DesfazerAlteracaoEmLote { lote: r.lote },
        )
        .unwrap();
    assert_eq!((u.restaurados, u.ignorados), (2, 0));
    let p = c.do_produto(a);
    assert_eq!((p.margem, p.grupo), (None, g));
    assert!(c.historico()[0].desfeita_em.is_some());
    assert!(!c.historico()[0].pode_desfazer);

    // Desfazer duas vezes: recusa.
    let e = c
        .cmd::<AlteracaoDesfeita>(
            "estoque.desfazer_alteracao_em_lote.v1",
            &DesfazerAlteracaoEmLote { lote: r.lote },
        )
        .unwrap_err();
    assert_eq!(e.codigo, CodigoErro::REGRA_VIOLADA);
}

#[test]
fn so_a_ultima_desfaz_e_quem_foi_editado_depois_mantem_a_edicao() {
    let c = Cenario::novo();
    let g = c.grupo("BEB");
    let (a, b) = (c.produto("Coca", g), c.produto("Guaraná", g));
    let primeiro = c.aplicar(&[a, b], margem(20)).unwrap();
    let segundo = c.aplicar(&[a], margem(30)).unwrap();

    // O primeiro não é o último: recusa, com a explicação.
    let e = c
        .cmd::<AlteracaoDesfeita>(
            "estoque.desfazer_alteracao_em_lote.v1",
            &DesfazerAlteracaoEmLote {
                lote: primeiro.lote,
            },
        )
        .unwrap_err();
    assert!(e.mensagem.contains("última"), "{}", e.mensagem);
    let h = c.historico();
    assert!(
        h[0].pode_desfazer && !h[1].pode_desfazer,
        "o mais novo é o único desfazível"
    );

    // Edição manual depois do segundo lote: ao desfazê-lo, o produto editado fica como está.
    let _: () = c
        .cmd(
            "estoque.definir_margem_do_produto.v1",
            &DefinirMargemDoProduto {
                produto: a,
                margem: Some(Percentual::pontos(99)),
            },
        )
        .unwrap();
    let u: AlteracaoDesfeita = c
        .cmd(
            "estoque.desfazer_alteracao_em_lote.v1",
            &DesfazerAlteracaoEmLote { lote: segundo.lote },
        )
        .unwrap();
    assert_eq!((u.restaurados, u.ignorados), (0, 1));
    assert_eq!(c.do_produto(a).margem, Some(Percentual::pontos(99)));

    // Agora o primeiro é o último não desfeito.
    let u: AlteracaoDesfeita = c
        .cmd(
            "estoque.desfazer_alteracao_em_lote.v1",
            &DesfazerAlteracaoEmLote {
                lote: primeiro.lote,
            },
        )
        .unwrap();
    assert_eq!(u.restaurados, 1, "só o Guaraná: a Coca foi editada depois");
    assert_eq!(c.do_produto(b).margem, None);
}

#[test]
fn entradas_ruins_sao_recusadas_e_nada_fica_gravado() {
    let c = Cenario::novo();
    let g = c.grupo("BEB");
    let a = c.produto("Coca", g);
    assert!(
        c.aplicar(&[a], Alteracao::default()).is_err(),
        "sem campo para alterar"
    );
    assert!(c.aplicar(&[], margem(10)).is_err(), "sem produto");
    assert!(
        c.aplicar(&[a], margem(100)).is_err(),
        "100% não deixa preço possível"
    );
    assert!(
        c.aplicar(
            &[a],
            Alteracao {
                grupo: Mudanca::Definir(Id::novo()),
                ..Alteracao::default()
            }
        )
        .is_err(),
        "grupo inexistente"
    );
    // Um produto inexistente no meio desfaz o lote todo, inclusive o cabeçalho.
    assert!(c.aplicar(&[a, Id::novo()], margem(20)).is_err());
    assert_eq!(c.do_produto(a).margem, None);
    assert!(c.historico().is_empty());
    // Aplicar o que já está assim não vira registro.
    c.aplicar(&[a], margem(20)).unwrap();
    let e = c.aplicar(&[a], margem(20)).unwrap_err();
    assert!(e.mensagem.contains("nenhum"), "{}", e.mensagem);
    assert_eq!(c.historico().len(), 1);
}

#[test]
fn lote_tambem_desativa_e_reativa_e_a_lista_filtra_por_grupo() {
    let c = Cenario::novo();
    let (g, h) = (c.grupo("BEB"), c.grupo("ALI"));
    let (a, b) = (c.produto("Coca", g), c.produto("Arroz", h));
    c.aplicar(
        &[a],
        Alteracao {
            ativo: Mudanca::Definir(false),
            ..Alteracao::default()
        },
    )
    .unwrap();
    assert!(!c.do_produto(a).ativo);
    let so_bebidas: Vec<ProdutoParaLote> = c.consulta(
        "estoque.produtos_para_lote.v1",
        &ProdutosParaLote {
            grupo: Some(g),
            ..ProdutosParaLote::default()
        },
    );
    assert_eq!(
        so_bebidas.len(),
        1,
        "inativos aparecem na lista do lote para poder reativar"
    );
    assert_eq!(so_bebidas[0].produto, a);
    let busca: Vec<ProdutoParaLote> = c.consulta(
        "estoque.produtos_para_lote.v1",
        &ProdutosParaLote {
            filtro: FiltroDeLote {
                busca: Some("arr".into()),
                ..FiltroDeLote::default()
            },
            ..ProdutosParaLote::default()
        },
    );
    assert_eq!(busca.iter().map(|p| p.produto).collect::<Vec<_>>(), vec![b]);
}

fn fiscal(f: impl FnOnce(&mut CamposFiscais)) -> CamposFiscais {
    let mut c = CamposFiscais::default();
    f(&mut c);
    c
}

#[test]
fn perfil_fiscal_herda_do_grupo_e_o_produto_sobrescreve() {
    let c = Cenario::novo();
    let raiz = c.grupo("BEB");
    let p = c.produto("Coca", raiz);
    let _: () = c
        .cmd(
            "estoque.definir_campos_fiscais_do_grupo.v1",
            &DefinirCamposFiscaisDoGrupo {
                grupo: raiz,
                campos: fiscal(|f| {
                    f.origem = Some(0);
                    f.cst_icms = Some("102".into());
                    f.cfop_saida_dentro_uf = Some("5102".into());
                    f.aliquota_icms = Some(Percentual::pontos(18));
                }),
            },
        )
        .unwrap();
    let _: () = c
        .cmd(
            "estoque.definir_campos_fiscais_do_produto.v1",
            &DefinirCamposFiscaisDoProduto {
                produto: p,
                campos: fiscal(|f| f.cfop_saida_dentro_uf = Some("5405".into())),
            },
        )
        .unwrap();
    let e: PerfilFiscalEfetivo = c.consulta(
        "estoque.perfil_fiscal_do_produto.v1",
        &PerfilFiscalDoProduto { produto: p },
    );
    let linha = |campo: &str| e.linhas.iter().find(|l| l.campo == campo).unwrap().clone();
    assert_eq!(linha("CFOP de saída na UF").valor.as_deref(), Some("5405"));
    assert_eq!(
        linha("CFOP de saída na UF").origem.as_deref(),
        Some("Produto")
    );
    assert_eq!(linha("CST/CSOSN do ICMS").origem.as_deref(), Some("BEB"));
    assert_eq!(linha("Alíquota de ICMS").valor.as_deref(), Some("18,00%"));
    assert_eq!(linha("CEST").valor, None);
    assert_eq!(e.ncm, "22021000");
    // Dado inválido é recusado antes de gravar.
    let e = c
        .cmd::<()>(
            "estoque.definir_campos_fiscais_do_grupo.v1",
            &DefinirCamposFiscaisDoGrupo {
                grupo: raiz,
                campos: fiscal(|f| f.cfop_saida_dentro_uf = Some("6102".into())),
            },
        )
        .unwrap_err();
    assert_eq!(e.codigo, CodigoErro::ENTRADA_INVALIDA);
}

#[test]
fn lote_altera_ncm_e_campos_fiscais_com_filtros_que_nao_sao_hierarquia() {
    let c = Cenario::novo();
    let (g, h) = (c.grupo("BEB"), c.grupo("ALI"));
    // Mesmo NCM em grupos diferentes: o filtro por NCM atravessa a hierarquia.
    let (a, b, d) = (
        c.produto("Coca", g),
        c.produto("Suco", h),
        c.produto("Arroz", h),
    );
    c.aplicar(
        &[d],
        Alteracao {
            ncm: Mudanca::Definir("10063021".into()),
            ..Alteracao::default()
        },
    )
    .unwrap();

    let por_ncm: Vec<ProdutoParaLote> = c.consulta(
        "estoque.produtos_para_lote.v1",
        &ProdutosParaLote {
            filtro: FiltroDeLote {
                ncm_prefixo: Some("2202".into()),
                ..FiltroDeLote::default()
            },
            ..ProdutosParaLote::default()
        },
    );
    let ids: Vec<Id> = por_ncm.iter().map(|p| p.produto).collect();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&a) && ids.contains(&b) && !ids.contains(&d));

    // Aplica CFOP/CST nesses dois (grupos diferentes), sem tocar no resto.
    let r = c
        .aplicar(
            &ids,
            Alteracao {
                fiscal: MudancaFiscal::Mesclar(fiscal(|f| {
                    f.cst_icms = Some("500".into());
                    f.cfop_saida_dentro_uf = Some("5405".into());
                })),
                ..Alteracao::default()
            },
        )
        .unwrap();
    assert_eq!(r.alterados, 2);
    let proprios = |id: Id| {
        c.consulta::<PerfilFiscalEfetivo>(
            "estoque.perfil_fiscal_do_produto.v1",
            &PerfilFiscalDoProduto { produto: id },
        )
        .proprios
    };
    assert_eq!(proprios(a).cst_icms.as_deref(), Some("500"));
    assert!(proprios(d).e_vazio());

    // Filtros combinados: sem perfil fiscal próprio + ativo → só o Arroz.
    let sem: Vec<ProdutoParaLote> = c.consulta(
        "estoque.produtos_para_lote.v1",
        &ProdutosParaLote {
            filtro: FiltroDeLote {
                sem_perfil_fiscal_proprio: true,
                ativo: Some(true),
                ..FiltroDeLote::default()
            },
            ..ProdutosParaLote::default()
        },
    );
    assert_eq!(sem.iter().map(|p| p.produto).collect::<Vec<_>>(), vec![d]);

    // Desfazer devolve o perfil e o NCM de antes.
    let u: AlteracaoDesfeita = c
        .cmd(
            "estoque.desfazer_alteracao_em_lote.v1",
            &DesfazerAlteracaoEmLote { lote: r.lote },
        )
        .unwrap();
    assert_eq!(u.restaurados, 2);
    assert!(proprios(a).e_vazio());
    // Limpar volta a herdar tudo.
    c.aplicar(
        &[a],
        Alteracao {
            fiscal: MudancaFiscal::Mesclar(fiscal(|f| f.cest = Some("1700100".into()))),
            ..Alteracao::default()
        },
    )
    .unwrap();
    c.aplicar(
        &[a],
        Alteracao {
            fiscal: MudancaFiscal::Limpar,
            ..Alteracao::default()
        },
    )
    .unwrap();
    assert!(proprios(a).e_vazio());
}

#[test]
fn lote_fiscal_exige_permissao_de_perfil_tributario_e_valida_formatos() {
    let c = Cenario::novo();
    let g = c.grupo("BEB");
    let a = c.produto("Coca", g);
    let sem = sessao(
        c.empresa,
        &["estoque.produto.editar", "estoque.produto.ver"],
    );
    let tenta = |alt: Alteracao| {
        c.d.executar_comando(
            "estoque.aplicar_alteracao_em_lote.v1",
            &carga(&AplicarAlteracaoEmLote {
                descricao: "x".into(),
                produtos: vec![a],
                alteracao: alt,
            }),
            &sem,
            &ambiente(c.empresa),
            c.arm.escritor(),
        )
    };
    let e = tenta(Alteracao {
        ncm: Mudanca::Definir("19059090".into()),
        ..Alteracao::default()
    })
    .unwrap_err();
    assert_eq!(e.codigo, CodigoErro::SEM_PERMISSAO);
    // Margem continua liberada para quem edita produto.
    assert!(tenta(margem(20)).is_ok());
    // Com permissão, NCM de tamanho errado e CFOP de sentido errado são recusados.
    assert!(c
        .aplicar(
            &[a],
            Alteracao {
                ncm: Mudanca::Definir("123".into()),
                ..Alteracao::default()
            }
        )
        .is_err());
    assert!(c
        .aplicar(
            &[a],
            Alteracao {
                fiscal: MudancaFiscal::Mesclar(fiscal(|f| f.cfop_entrada = Some("5102".into()))),
                ..Alteracao::default()
            }
        )
        .is_err());
}
