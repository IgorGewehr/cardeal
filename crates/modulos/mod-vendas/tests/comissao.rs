//! A comissão do vendedor de ponta a ponta, pelo `Despachante` real contra SQLite: o percentual
//! de cada produto (a comissão por estrutura do estoque) é congelado no faturamento, a apuração
//! do mês lança no Razão uma vez por pedido (rodar de novo não duplica) e o pagamento quita o
//! que o vendedor tem apurado, de uma conta de caixa ou banco.

#![allow(clippy::result_large_err)] // `ErroArmazenamento` carrega detalhes de propósito

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel as PapelAuth, Sessao};
use cardeal_kernel::{
    CodigoErro, Data, Dinheiro, Fuso, Id, Instante, Percentual, Preco, Quantidade,
};
use cardeal_ledger::semear_plano_padrao;
use cardeal_modkit::{Ambiente, Despachante, Modulo, PedidoAtivacao, RegistroModulos};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use mod_clientes::{
    CriarPessoa, ModuloClientes, Papel, PessoaCadastrada, TipoDocumento, TipoPessoa,
};
use mod_estoque::{
    CriarGrupoProduto, CriarLocal, CriarProduto, CriarUnidade, DefinirComissaoDoProduto,
    GrupoProdutoCriado, LocalCriado, ModuloEstoque, ProdutoCriado, RegistrarEntrada, TipoLocal,
    UnidadeCriada,
};
use mod_vendas::{
    resumo_por_vendedor, AdicionarItemPedido, ApurarComissoes, ComissoesApuradas, ComissoesDoMes,
    ComissoesPagas, ConfirmarPedido, CriarPedido, CriarRegraPreco, CriarTabelaPreco,
    EstadoComissao, FaturarPedido, LinhaComissao, ModuloVendas, PagarComissoes, PedidoCriado,
    TabelaPrecoCriada, TipoTabela,
};
use serde::de::DeserializeOwned;
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
        .resolver(
            &PedidoAtivacao::nova()
                .com_modulo("vendas")
                .com_submodulo("vendas", "comissao"),
        )
        .unwrap();
    Ambiente::novo(empresa, efetivo)
}

fn sessao(empresa: Id) -> Sessao {
    let mut papel = PapelAuth::novo(empresa, "Gerente");
    for p in [
        "clientes.pessoa.criar",
        "estoque.produto.criar",
        "estoque.produto.editar",
        "estoque.local.criar",
        "estoque.movimento.entrada",
        "vendas.tabela_preco.criar",
        "vendas.tabela_preco.editar",
        "vendas.pedido.criar",
        "vendas.pedido.editar",
        "vendas.pedido.confirmar",
        "vendas.pedido.faturar",
        "vendas.comissao.ver",
        "vendas.comissao.apurar",
        "vendas.comissao.pagar",
        "financeiro.plano.editar",
    ] {
        papel = papel.com_permissao(p);
    }
    Sessao::abrir(EmissaoSessao::padrao(
        Id::novo(),
        Id::novo(),
        Escopo::empresa_inteira(empresa),
        AutorizacoesEfetivas::consolidar([&papel]),
        Instante::EPOCA,
    ))
}

struct Loja {
    _dir: TempDir,
    arm: Armazenamento,
    empresa: Id,
    d: Despachante,
    s: Sessao,
    amb: Ambiente,
}

impl Loja {
    fn nova() -> Self {
        let (_dir, arm, empresa) = base();
        Self {
            _dir,
            arm,
            empresa,
            d: Despachante::construir(&[
                &ModuloClientes,
                &ModuloEstoque,
                &mod_financeiro::ModuloFinanceiro,
                &ModuloVendas,
            ])
            .unwrap(),
            s: sessao(empresa),
            amb: ambiente(empresa),
        }
    }

    fn comando<T: DeserializeOwned>(
        &self,
        nome: &str,
        carga: &impl serde::Serialize,
    ) -> Result<T, cardeal_kernel::Erro> {
        self.d
            .executar_comando(
                nome,
                &postcard::to_stdvec(carga).unwrap(),
                &self.s,
                &self.amb,
                self.arm.escritor(),
            )
            .map(|b| postcard::from_bytes(&b).unwrap())
    }

    fn ok<T: DeserializeOwned>(&self, nome: &str, carga: &impl serde::Serialize) -> T {
        self.comando(nome, carga).unwrap()
    }

    fn comissoes(&self) -> Vec<LinhaComissao> {
        let bytes = self
            .d
            .executar_consulta(
                "vendas.comissoes_do_mes.v1",
                &postcard::to_stdvec(&ComissoesDoMes {
                    competencia: hoje().competencia(),
                })
                .unwrap(),
                &self.s,
                &self.amb,
                self.arm.leitor(),
            )
            .unwrap();
        postcard::from_bytes(&bytes).unwrap()
    }

    fn escalar(&self, sql: &str) -> i64 {
        let empresa = self.empresa;
        let sql = sql.to_string();
        self.arm
            .leitor()
            .consultar(move |c| {
                c.query_row(&sql, [empresa.em_bytes().as_slice()], |r| r.get(0))
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            })
            .unwrap()
    }

    fn conta(&self, codigo: &str) -> Id {
        let empresa = self.empresa;
        let codigo = codigo.to_string();
        self.arm
            .leitor()
            .consultar(move |c| {
                c.query_row(
                    "SELECT id FROM razao_conta WHERE empresa = ?1 AND codigo = ?2",
                    rusqlite::params![empresa.em_bytes().as_slice(), codigo],
                    |r| r.get::<_, Vec<u8>>(0),
                )
                .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            })
            .map(|b| Id::de_bytes(b.as_slice().try_into().unwrap()))
            .unwrap()
    }

    /// O saldo (débitos − créditos) de uma conta.
    fn saldo(&self, codigo: &str) -> i64 {
        self.escalar(&format!(
            "SELECT COALESCE(SUM(p.valor), 0) FROM razao_partida p
             JOIN razao_conta c ON c.id = p.conta
             WHERE c.empresa = ?1 AND c.codigo = '{codigo}'"
        ))
    }
}

fn hoje() -> Data {
    Data::hoje(Fuso::BRASILIA)
}

struct Cenario {
    cliente: Id,
    produto: Id,
    local: Id,
    tabela: Id,
}

fn cenario(l: &Loja) -> Cenario {
    let cliente: PessoaCadastrada = l.ok(
        "clientes.criar_pessoa.v1",
        &CriarPessoa {
            tipo: TipoPessoa::Fisica,
            nome: "João Silva".to_string(),
            nome_fantasia: None,
            papel_inicial: Papel::Cliente,
            documento_tipo: Some(TipoDocumento::Cpf),
            documento_numero: Some("52998224725".to_string()),
            data_nascimento: None,
            endereco: None,
            contato: None,
        },
    );
    let grupo: GrupoProdutoCriado = l.ok(
        "estoque.criar_grupo_produto.v1",
        &CriarGrupoProduto {
            codigo: "GERAL".to_string(),
            nome: "Geral".to_string(),
            pai: None,
        },
    );
    let unidade: UnidadeCriada = l.ok(
        "estoque.criar_unidade.v1",
        &CriarUnidade {
            sigla: "UN".to_string(),
            nome: "Unidade".to_string(),
            fracionavel: false,
        },
    );
    let produto: ProdutoCriado = l.ok(
        "estoque.criar_produto.v1",
        &CriarProduto {
            grupo_produto: grupo.grupo_produto,
            nome: "Cabo USB-C".to_string(),
            ncm: "85444200".to_string(),
            unidade_padrao: unidade.unidade,
            codigo_barras: None,
            detalhes_tecnicos: None,
        },
    );
    let local: LocalCriado = l.ok(
        "estoque.criar_local.v1",
        &CriarLocal {
            nome: "Loja".to_string(),
            tipo: TipoLocal::Deposito,
        },
    );
    let _: mod_estoque::EntradaRegistrada = l.ok(
        "estoque.registrar_entrada.v1",
        &RegistrarEntrada {
            produto: produto.produto,
            local: local.local,
            quantidade: Quantidade::unidades(50),
            custo_unitario: Preco::reais(30),
        },
    );
    let tabela: TabelaPrecoCriada = l.ok(
        "vendas.criar_tabela_preco.v1",
        &CriarTabelaPreco {
            nome: "Balcão".to_string(),
            tipo: TipoTabela::Venda,
            vigente_de: hoje().mais_dias(-1),
            vigente_ate: None,
        },
    );
    let _: mod_vendas::RegraPrecoCriada = l.ok(
        "vendas.criar_regra_preco.v1",
        &CriarRegraPreco {
            tabela_preco: tabela.tabela_preco,
            alvo_produto: Some(produto.produto),
            alvo_grupo: None,
            quantidade_minima: None,
            preco: Preco::reais(100),
            periodo_de: None,
            periodo_ate: None,
        },
    );
    Cenario {
        cliente: cliente.pessoa,
        produto: produto.produto,
        local: local.local,
        tabela: tabela.tabela_preco,
    }
}

/// Cria, confirma e fatura (à vista) um pedido de `unidades` do produto, a R$ 100.
fn vender(l: &Loja, c: &Cenario, vendedor: Id, unidades: i64) -> Id {
    let pedido: PedidoCriado = l.ok(
        "vendas.criar_pedido.v1",
        &CriarPedido {
            cliente: c.cliente,
            vendedor,
            condicao_pagamento: Id::novo(),
            tabela_preco: c.tabela,
            local_expedicao: c.local,
        },
    );
    let _: mod_vendas::ItemFoiAdicionado = l.ok(
        "vendas.adicionar_item_pedido.v1",
        &AdicionarItemPedido {
            pedido: pedido.pedido,
            produto: c.produto,
            variacao: None,
            quantidade: Quantidade::unidades(unidades),
            desconto_percentual: Percentual::ZERO,
        },
    );
    let (): () = l.ok(
        "vendas.confirmar_pedido.v1",
        &ConfirmarPedido {
            pedido: pedido.pedido,
        },
    );
    let _: mod_vendas::PedidoFoiFaturado = l.ok(
        "vendas.faturar_pedido.v1",
        &FaturarPedido {
            pedido: pedido.pedido,
            a_vista: true,
        },
    );
    pedido.pedido
}

fn comissao_do_produto(l: &Loja, c: &Cenario, pontos: i64) {
    let (): () = l.ok(
        "estoque.definir_comissao_do_produto.v1",
        &DefinirComissaoDoProduto {
            produto: c.produto,
            comissao: Some(Percentual::pontos(pontos)),
        },
    );
}

#[test]
fn percentual_congelado_no_faturamento_apuracao_idempotente_e_pagamento() {
    let l = Loja::nova();
    let c = cenario(&l);
    let (ana, beto) = (Id::novo(), Id::novo());

    // Ana vende 2 a 5%; depois a comissão do produto sobe para 10% e Beto vende 3.
    comissao_do_produto(&l, &c, 5);
    let pedido_ana = vender(&l, &c, ana, 2);
    comissao_do_produto(&l, &c, 10);
    let pedido_beto = vender(&l, &c, beto, 3);

    // A prévia já mostra o percentual de cada pedido no faturamento, não o de hoje.
    let previa = l.comissoes();
    assert_eq!(previa.len(), 2);
    let da = |linhas: &[LinhaComissao], pedido: Id| {
        linhas.iter().find(|x| x.pedido == pedido).unwrap().clone()
    };
    assert_eq!(da(&previa, pedido_ana).valor, Dinheiro::reais(10)); // 200 × 5%
    assert_eq!(da(&previa, pedido_beto).valor, Dinheiro::reais(30)); // 300 × 10%
    assert!(previa.iter().all(|x| x.estado.is_none()));

    // Apura: um lançamento por pedido, D despesa comercial / C salários e encargos.
    let apuradas: ComissoesApuradas = l.ok(
        "vendas.apurar_comissoes.v1",
        &ApurarComissoes {
            competencia: hoje().competencia(),
        },
    );
    assert_eq!((apuradas.pedidos, apuradas.valor), (2, Dinheiro::reais(40)));
    assert_eq!(l.saldo("5.5"), 4_000);
    assert_eq!(l.saldo("2.1.03"), -4_000);
    // Rodar de novo não duplica.
    let de_novo: ComissoesApuradas = l.ok(
        "vendas.apurar_comissoes.v1",
        &ApurarComissoes {
            competencia: hoje().competencia(),
        },
    );
    assert_eq!(de_novo.pedidos, 0);
    assert_eq!(
        l.escalar(
            "SELECT COUNT(*) FROM razao_lancamento
             WHERE empresa = ?1 AND origem_modulo = 'vendas' AND origem_tipo = 'comissao'"
        ),
        2
    );

    // Paga a Ana pelo caixa.
    let caixa = l.conta("1.1.01");
    let pago: ComissoesPagas = l.ok(
        "vendas.pagar_comissoes.v1",
        &PagarComissoes {
            vendedor: ana,
            competencia: hoje().competencia(),
            conta: caixa,
            data: hoje(),
        },
    );
    assert_eq!((pago.pedidos, pago.valor), (1, Dinheiro::reais(10)));
    assert_eq!(l.saldo("2.1.03"), -3_000);

    // Pagar de novo, de uma conta que não é caixa/banco, ou com data futura: recusado.
    let erro = |p: &PagarComissoes| {
        l.comando::<ComissoesPagas>("vendas.pagar_comissoes.v1", p)
            .unwrap_err()
            .codigo
    };
    let mut p = PagarComissoes {
        vendedor: ana,
        competencia: hoje().competencia(),
        conta: caixa,
        data: hoje(),
    };
    assert_eq!(erro(&p), CodigoErro::REGRA_VIOLADA);
    p.vendedor = beto;
    p.conta = l.conta("4.1");
    assert_eq!(erro(&p), CodigoErro::ENTRADA_INVALIDA);
    p.conta = caixa;
    p.data = hoje().mais_dias(1);
    assert_eq!(erro(&p), CodigoErro::ENTRADA_INVALIDA);

    // O fechamento do mês: Ana paga, Beto a pagar.
    let linhas = l.comissoes();
    assert_eq!(da(&linhas, pedido_ana).estado, Some(EstadoComissao::Paga));
    assert_eq!(
        da(&linhas, pedido_beto).estado,
        Some(EstadoComissao::Apurada)
    );
    let resumo = resumo_por_vendedor(&linhas);
    let r_beto = resumo.iter().find(|r| r.vendedor == beto).unwrap();
    assert_eq!(r_beto.a_pagar, Dinheiro::reais(30));
}

#[test]
fn sem_comissao_definida_nada_a_apurar_e_conta_do_modelo_comercio_e_usada() {
    let l = Loja::nova();
    let c = cenario(&l);
    // Sem comissão no produto, no grupo nem padrão: nada a apurar.
    vender(&l, &c, Id::novo(), 1);
    assert!(l.comissoes().is_empty());

    // Com o modelo Comércio aplicado, a despesa vai para "Comissões de vendedores" (5.42).
    let _: mod_financeiro::ModeloAplicado = l.ok(
        "financeiro.aplicar_modelo_de_plano.v1",
        &mod_financeiro::AplicarModeloDePlano {
            modelo: cardeal_ledger::ModeloDePlano::Comercio,
        },
    );
    comissao_do_produto(&l, &c, 4);
    vender(&l, &c, Id::novo(), 5);
    let apuradas: ComissoesApuradas = l.ok(
        "vendas.apurar_comissoes.v1",
        &ApurarComissoes {
            competencia: hoje().competencia(),
        },
    );
    assert_eq!(apuradas.valor, Dinheiro::reais(20)); // 500 × 4%
    assert_eq!(l.saldo("5.42"), 2_000);
    assert_eq!(l.saldo("5.5"), 0);
}
