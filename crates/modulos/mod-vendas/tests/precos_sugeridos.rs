//! O preço sugerido pelo estoque chegando à tabela de preço de vendas, pelo `Despachante` real.

#![allow(clippy::result_large_err)] // `ErroArmazenamento` carrega detalhes de propósito

use cardeal_auth::{
    AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel as PapelAuth, Sessao, ValorLimite,
};
use cardeal_kernel::{CodigoErro, Data, Fuso, Id, Instante, Percentual, Preco, Quantidade};
use cardeal_ledger::semear_plano_padrao;
use cardeal_modkit::{Ambiente, Despachante, Modulo, PedidoAtivacao, RegistroModulos};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use mod_clientes::ModuloClientes;
use mod_estoque::precificacao::ArredondamentoDePreco;
use mod_estoque::{
    CriarGrupoProduto, CriarLocal, CriarProduto, CriarUnidade, DefinirMargemDoGrupo,
    DefinirParametrosPreco, GrupoProdutoCriado, LocalCriado, ModuloEstoque, ParametrosPreco,
    ProdutoCriado, RegistrarEntrada, TipoLocal, UnidadeCriada,
};
use mod_vendas::{
    AplicarPrecosSugeridos, CriarTabelaPreco, ModuloVendas, PrecosAplicados, RegrasDaTabela,
    TabelaPrecoCriada, TipoTabela,
};
use tempfile::TempDir;

fn base() -> (TempDir, Armazenamento, Id) {
    let dir = tempfile::tempdir().unwrap();
    let arm =
        Armazenamento::abrir(ConfigArmazenamento::arquivo(dir.path().join("cardeal.db"))).unwrap();
    arm.migrar(&[
        cardeal_ledger::migracoes::conjunto(),
        ModuloClientes.migracoes(),
        ModuloEstoque.migracoes(),
        mod_financeiro::ModuloFinanceiro.migracoes(),
        ModuloVendas.migracoes(),
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
    rm.registrar(&mod_clientes::MANIFESTO).unwrap();
    rm.registrar(&mod_estoque::MANIFESTO).unwrap();
    rm.registrar(&mod_financeiro::MANIFESTO).unwrap();
    rm.registrar(&mod_vendas::MANIFESTO).unwrap();
    let efetivo = rm
        .resolver(&PedidoAtivacao::nova().com_modulo("vendas"))
        .unwrap();
    Ambiente::novo(empresa, efetivo)
}

fn sessao_completa(empresa: Id) -> Sessao {
    let mut papel = PapelAuth::novo(empresa, "Testador");
    for p in [
        "clientes.pessoa.criar",
        "estoque.produto.criar",
        "estoque.local.criar",
        "estoque.movimento.entrada",
        "vendas.tabela_preco.criar",
        "vendas.tabela_preco.editar",
        "vendas.pedido.criar",
        "vendas.pedido.editar",
        "vendas.pedido.confirmar",
        "vendas.pedido.faturar",
        "vendas.pedido.cancelar",
        "vendas.tabela_preco.ver",
        "estoque.produto.editar",
        "estoque.tributario.editar",
        "estoque.produto.ver",
    ] {
        papel = papel.com_permissao(p);
    }
    papel = papel.com_limite(
        "vendas.desconto_maximo",
        ValorLimite::Percentual(Percentual::pontos(10)),
    );
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

struct Cenario {
    _dir: TempDir,
    arm: Armazenamento,
    empresa: Id,
    d: Despachante,
    s: Sessao,
}

impl Cenario {
    fn novo() -> Self {
        let (dir, arm, empresa) = base();
        Self {
            _dir: dir,
            arm,
            empresa,
            d: Despachante::construir(&[
                &ModuloClientes,
                &ModuloEstoque,
                &mod_financeiro::ModuloFinanceiro,
                &ModuloVendas,
            ])
            .unwrap(),
            s: sessao_completa(empresa),
        }
    }

    fn cmd<T: serde::de::DeserializeOwned>(
        &self,
        nome: &str,
        c: &impl serde::Serialize,
    ) -> Result<T, cardeal_kernel::Erro> {
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

    fn produto(&self, nome: &str, grupo: Id, custo: Option<Preco>) -> Id {
        let un: UnidadeCriada = self
            .cmd(
                "estoque.criar_unidade.v1",
                &CriarUnidade {
                    sigla: "UN".into(),
                    nome: "Un".into(),
                    fracionavel: false,
                },
            )
            .unwrap_or_else(|_| {
                let b = self
                    .d
                    .executar_consulta(
                        "estoque.unidades.v1",
                        &carga(&mod_estoque::Unidades),
                        &self.s,
                        &ambiente(self.empresa),
                        self.arm.leitor(),
                    )
                    .unwrap();
                let u: Vec<mod_estoque::ItemUnidade> = postcard::from_bytes(&b).unwrap();
                UnidadeCriada { unidade: u[0].id }
            });
        let p: ProdutoCriado = self
            .cmd(
                "estoque.criar_produto.v1",
                &CriarProduto {
                    grupo_produto: grupo,
                    nome: nome.into(),
                    ncm: "22021000".into(),
                    unidade_padrao: un.unidade,
                    codigo_barras: None,
                    detalhes_tecnicos: None,
                },
            )
            .unwrap();
        if let Some(custo) = custo {
            let l: LocalCriado = self
                .cmd(
                    "estoque.criar_local.v1",
                    &CriarLocal {
                        nome: format!("Dep {nome}"),
                        tipo: TipoLocal::Deposito,
                    },
                )
                .unwrap();
            let _: mod_estoque::EntradaRegistrada = self
                .cmd(
                    "estoque.registrar_entrada.v1",
                    &RegistrarEntrada {
                        produto: p.produto,
                        local: l.local,
                        quantidade: Quantidade::unidades(10),
                        custo_unitario: custo,
                    },
                )
                .unwrap();
        }
        p.produto
    }

    fn regras(&self, tabela: Id) -> Vec<mod_vendas::RegraPreco> {
        let b = self
            .d
            .executar_consulta(
                "vendas.regras_da_tabela.v1",
                &carga(&RegrasDaTabela { tabela }),
                &self.s,
                &ambiente(self.empresa),
                self.arm.leitor(),
            )
            .unwrap();
        postcard::from_bytes(&b).unwrap()
    }
}

#[test]
fn preco_sugerido_vira_regra_cria_na_primeira_e_atualiza_nas_seguintes() {
    let c = Cenario::novo();
    // MEI, custo operacional 25% informado, margem 15% no grupo: 50 / 0,60 = 83,33 → ,90.
    let mut p = ParametrosPreco::padrao_para("MEI");
    p.custo_operacional = Percentual::pontos(25);
    p.custo_operacional_derivado = false;
    let _: () = c
        .cmd(
            "estoque.definir_parametros_preco.v1",
            &DefinirParametrosPreco { parametros: p },
        )
        .unwrap();
    let g: GrupoProdutoCriado = c
        .cmd(
            "estoque.criar_grupo_produto.v1",
            &CriarGrupoProduto {
                codigo: "BEB".into(),
                nome: "Bebidas".into(),
                pai: None,
            },
        )
        .unwrap();
    let _: () = c
        .cmd(
            "estoque.definir_margem_do_grupo.v1",
            &DefinirMargemDoGrupo {
                grupo: g.grupo_produto,
                margem: Some(Percentual::pontos(15)),
            },
        )
        .unwrap();
    let com_custo = c.produto("Coca 2L", g.grupo_produto, Some(Preco::reais(50)));
    let sem_custo = c.produto("Guaraná 2L", g.grupo_produto, None);
    let tabela: TabelaPrecoCriada = c
        .cmd(
            "vendas.criar_tabela_preco.v1",
            &CriarTabelaPreco {
                nome: "Balcão".into(),
                tipo: TipoTabela::Venda,
                vigente_de: Data::hoje(Fuso::BRASILIA).mais_dias(-1),
                vigente_ate: None,
            },
        )
        .unwrap();

    let aplicar = |produtos: Vec<Id>| -> PrecosAplicados {
        c.cmd(
            "vendas.aplicar_precos_sugeridos.v1",
            &AplicarPrecosSugeridos {
                tabela_preco: tabela.tabela_preco,
                produtos,
                arredondamento: ArredondamentoDePreco::FinalNoventa,
            },
        )
        .unwrap()
    };

    let r = aplicar(vec![com_custo, sem_custo]);
    assert_eq!((r.criadas, r.atualizadas), (1, 0));
    assert_eq!(r.ignorados.len(), 1);
    assert_eq!(r.ignorados[0].0, sem_custo);
    assert!(r.ignorados[0].1.contains("custo"), "{}", r.ignorados[0].1);
    let regras = c.regras(tabela.tabela_preco);
    assert_eq!(regras.len(), 1);
    assert_eq!(regras[0].preco, Preco::centavos(8_390));

    // Margem sobe para 25%: 50 / 0,50 = 100,00 → 100,90; a mesma regra é atualizada.
    let _: () = c
        .cmd(
            "estoque.definir_margem_do_grupo.v1",
            &DefinirMargemDoGrupo {
                grupo: g.grupo_produto,
                margem: Some(Percentual::pontos(25)),
            },
        )
        .unwrap();
    let r = aplicar(vec![com_custo]);
    assert_eq!((r.criadas, r.atualizadas), (0, 1));
    let regras = c.regras(tabela.tabela_preco);
    assert_eq!(regras.len(), 1, "não duplica");
    assert_eq!(regras[0].preco, Preco::centavos(10_090));
}

#[test]
fn tabela_promocional_e_tabela_inexistente_sao_recusadas() {
    let c = Cenario::novo();
    let promo: TabelaPrecoCriada = c
        .cmd(
            "vendas.criar_tabela_preco.v1",
            &CriarTabelaPreco {
                nome: "Black Friday".into(),
                tipo: TipoTabela::Promocional,
                vigente_de: Data::hoje(Fuso::BRASILIA),
                vigente_ate: Some(Data::hoje(Fuso::BRASILIA).mais_dias(5)),
            },
        )
        .unwrap();
    let e = c
        .cmd::<PrecosAplicados>(
            "vendas.aplicar_precos_sugeridos.v1",
            &AplicarPrecosSugeridos {
                tabela_preco: promo.tabela_preco,
                produtos: vec![],
                arredondamento: ArredondamentoDePreco::Nenhum,
            },
        )
        .unwrap_err();
    assert_eq!(e.codigo, CodigoErro::REGRA_VIOLADA);
    let e = c
        .cmd::<PrecosAplicados>(
            "vendas.aplicar_precos_sugeridos.v1",
            &AplicarPrecosSugeridos {
                tabela_preco: Id::novo(),
                produtos: vec![],
                arredondamento: ArredondamentoDePreco::Nenhum,
            },
        )
        .unwrap_err();
    assert_eq!(e.codigo, CodigoErro::NAO_ENCONTRADO);
}
