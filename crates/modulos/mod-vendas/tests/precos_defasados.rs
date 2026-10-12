//! Preços defasados: o preço da tabela contra o que o estoque calcula hoje, pelo `Despachante` real.

#![allow(clippy::result_large_err)] // `ErroArmazenamento` carrega detalhes de propósito

use cardeal_auth::{
    AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel as PapelAuth, Sessao, ValorLimite,
};
use cardeal_kernel::{Data, Dinheiro, Fuso, Id, Instante, Percentual, Preco, Quantidade};
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
    AplicarPrecosSugeridos, CriarRegraPreco, CriarTabelaPreco, ModuloVendas, PrecoDefasado,
    PrecosAplicados, PrecosDefasados, RegraPrecoCriada, SituacaoDoPreco, TabelaPrecoCriada,
    TipoTabela,
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
}

impl Cenario {
    /// Cenário comum: MEI sem custo operacional e margem de 20% no grupo, uma tabela de balcão.
    fn montar() -> (Self, Id, Id) {
        let c = Self::novo();
        let mut p = ParametrosPreco::padrao_para("MEI");
        p.custo_operacional = Percentual::ZERO;
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
                    margem: Some(Percentual::pontos(20)),
                },
            )
            .unwrap();
        let t: TabelaPrecoCriada = c
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
        (c, g.grupo_produto, t.tabela_preco)
    }

    fn regra(&self, tabela: Id, produto: Id, preco: Preco, faixa: Option<Quantidade>) {
        let _: RegraPrecoCriada = self
            .cmd(
                "vendas.criar_regra_preco.v1",
                &CriarRegraPreco {
                    tabela_preco: tabela,
                    alvo_produto: Some(produto),
                    alvo_grupo: None,
                    quantidade_minima: faixa,
                    preco,
                    periodo_de: None,
                    periodo_ate: None,
                },
            )
            .unwrap();
    }

    fn defasados(&self, tabela: Id) -> Vec<PrecoDefasado> {
        let b = self
            .d
            .executar_consulta(
                "vendas.precos_defasados.v1",
                &carga(&PrecosDefasados {
                    tabela_preco: tabela,
                }),
                &self.s,
                &ambiente(self.empresa),
                self.arm.leitor(),
            )
            .unwrap();
        postcard::from_bytes(&b).unwrap()
    }

    fn nova_entrada(&self, produto: Id, custo: Preco) {
        let b = self
            .d
            .executar_consulta(
                "estoque.locais.v1",
                &carga(&mod_estoque::Locais),
                &self.s,
                &ambiente(self.empresa),
                self.arm.leitor(),
            )
            .unwrap();
        let locais: Vec<mod_estoque::ItemLocal> = postcard::from_bytes(&b).unwrap();
        let _: mod_estoque::EntradaRegistrada = self
            .cmd(
                "estoque.registrar_entrada.v1",
                &RegistrarEntrada {
                    produto,
                    local: locais[0].id,
                    quantidade: Quantidade::unidades(10),
                    custo_unitario: custo,
                },
            )
            .unwrap();
    }
}

#[test]
fn preco_defasado_e_classificado_aplicar_corrige_e_compra_mais_cara_reabre() {
    let (c, grupo, tabela) = Cenario::montar();
    // Custo 50, margem 20%: sugerido 62,50, empate 50,00.
    let abaixo = c.produto("Coca 2L", grupo, Some(Preco::reais(50)));
    let prejuizo = c.produto("Guaraná 2L", grupo, Some(Preco::reais(50)));
    let sem_custo = c.produto("Fanta 2L", grupo, None);
    c.regra(tabela, abaixo, Preco::reais(60), None);
    c.regra(tabela, prejuizo, Preco::reais(45), None);
    c.regra(tabela, sem_custo, Preco::reais(10), None);
    // Regra de faixa de atacado do mesmo produto não conta como preço praticado.
    c.regra(
        tabela,
        abaixo,
        Preco::reais(30),
        Some(Quantidade::unidades(10)),
    );

    let l = c.defasados(tabela);
    assert_eq!(l.len(), 3);
    // Ordem: prejuízo, abaixo do alvo, sem cálculo.
    assert_eq!(l[0].produto, prejuizo);
    assert_eq!(l[0].situacao, SituacaoDoPreco::Prejuizo);
    assert_eq!(l[1].produto, abaixo);
    assert_eq!(l[1].situacao, SituacaoDoPreco::AbaixoDoAlvo);
    assert_eq!(l[1].preco_atual, Dinheiro::reais(60));
    assert_eq!(l[1].preco_sugerido, Some(Dinheiro::centavos(6_250)));
    assert_eq!(l[1].preco_minimo, Some(Dinheiro::reais(50)));
    assert_eq!(l[1].margem_desejada, Some(Percentual::pontos(20)));
    assert!(l[1].margem_atual.unwrap() < Percentual::pontos(19));
    assert!(matches!(&l[2].situacao, SituacaoDoPreco::SemCalculo(m) if m.contains("custo")));

    // Aplicar o sugerido aos defasados: ficam no alvo.
    let r: PrecosAplicados = c
        .cmd(
            "vendas.aplicar_precos_sugeridos.v1",
            &AplicarPrecosSugeridos {
                tabela_preco: tabela,
                produtos: vec![prejuizo, abaixo],
                arredondamento: ArredondamentoDePreco::FinalNoventa,
            },
        )
        .unwrap();
    assert_eq!((r.criadas, r.atualizadas), (0, 2));
    let l = c.defasados(tabela);
    assert!(l
        .iter()
        .filter(|d| d.produto != sem_custo)
        .all(|d| d.situacao == SituacaoDoPreco::NoAlvo));

    // Nova entrada mais cara (10 a R$ 70 → custo médio 60): o preço de 62,90 fica abaixo do alvo.
    c.nova_entrada(abaixo, Preco::reais(70));
    let d = c
        .defasados(tabela)
        .into_iter()
        .find(|d| d.produto == abaixo)
        .unwrap();
    assert_eq!(d.situacao, SituacaoDoPreco::AbaixoDoAlvo);
    // E uma entrada ainda mais cara (custo médio 75 > 62,90) leva a prejuízo.
    c.nova_entrada(abaixo, Preco::reais(100));
    let d = c
        .defasados(tabela)
        .into_iter()
        .find(|d| d.produto == abaixo)
        .unwrap();
    assert_eq!(d.situacao, SituacaoDoPreco::Prejuizo);
}

#[test]
fn preco_acima_do_alvo_e_tabela_sem_regras() {
    let (c, grupo, tabela) = Cenario::montar();
    let p = c.produto("Coca 2L", grupo, Some(Preco::reais(50)));
    assert!(c.defasados(tabela).is_empty());
    c.regra(tabela, p, Preco::reais(90), None); // margem 44%
    let l = c.defasados(tabela);
    assert_eq!(l[0].situacao, SituacaoDoPreco::AcimaDoAlvo);
}
