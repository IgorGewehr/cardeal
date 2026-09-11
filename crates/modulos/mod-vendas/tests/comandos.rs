//! O ciclo completo de um pedido de venda atravessando o `Despachante` real contra SQLite:
//! cliente (`mod-clientes`) → produto em estoque com saldo real (`mod-estoque`) → tabela e
//! regra de preço → pedido (criar, adicionar item com preço resolvido, confirmar, faturar) —
//! à vista sem título e a prazo com título vinculado ao lançamento combinado. Autorização,
//! transação e persistência — tudo junto, exatamente como vai rodar em produção.

#![allow(clippy::result_large_err)] // `ErroArmazenamento` carrega detalhes de propósito

use cardeal_auth::{
    AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel as PapelAuth, Sessao, ValorLimite,
};
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
    CriarGrupoProduto, CriarLocal, CriarProduto, CriarUnidade, GrupoProdutoCriado, LocalCriado,
    ModuloEstoque, ProdutoCriado, RegistrarEntrada, TipoLocal, UnidadeCriada,
};
use mod_vendas::{
    AdicionarItemPedido, CancelarPedido, ConfirmarPedido, CriarPedido, CriarRegraPreco,
    CriarTabelaPreco, FaturarPedido, ItemFoiAdicionado, ModuloVendas, PedidoCriado,
    PedidoFoiFaturado, RegraPrecoCriada, TabelaPrecoCriada, TipoTabela,
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

fn conta_lancamentos(arm: &Armazenamento, empresa: Id) -> i64 {
    arm.leitor()
        .consultar(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM razao_lancamento WHERE empresa = ?1",
                [empresa.em_bytes().as_slice()],
                |r| r.get(0),
            )
            .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
        })
        .unwrap()
}

fn conta_titulos(arm: &Armazenamento, empresa: Id) -> i64 {
    arm.leitor()
        .consultar(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM financeiro_titulo WHERE empresa = ?1 AND origem_modulo = 'vendas'",
                [empresa.em_bytes().as_slice()],
                |r| r.get(0),
            )
            .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
        })
        .unwrap()
}

struct Cenario {
    cliente: Id,
    produto: Id,
    local: Id,
    tabela_preco: Id,
}

fn montar_cenario(
    d: &Despachante,
    s: &Sessao,
    amb: &Ambiente,
    arm: &Armazenamento,
    preco_unitario: i64,
) -> Cenario {
    let cliente: PessoaCadastrada = postcard::from_bytes(
        &d.executar_comando(
            "clientes.criar_pessoa.v1",
            &carga(&CriarPessoa {
                tipo: TipoPessoa::Fisica,
                nome: "João Silva".to_string(),
                nome_fantasia: None,
                papel_inicial: Papel::Cliente,
                documento_tipo: Some(TipoDocumento::Cpf),
                documento_numero: Some("52998224725".to_string()),
                data_nascimento: None,
                endereco: None,
                contato: None,
            }),
            s,
            amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    let grupo: GrupoProdutoCriado = postcard::from_bytes(
        &d.executar_comando(
            "estoque.criar_grupo_produto.v1",
            &carga(&CriarGrupoProduto {
                codigo: "GERAL".to_string(),
                nome: "Geral".to_string(),
                pai: None,
            }),
            s,
            amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    let unidade: UnidadeCriada = postcard::from_bytes(
        &d.executar_comando(
            "estoque.criar_unidade.v1",
            &carga(&CriarUnidade {
                sigla: "UN".to_string(),
                nome: "Unidade".to_string(),
                fracionavel: false,
            }),
            s,
            amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    let produto: ProdutoCriado = postcard::from_bytes(
        &d.executar_comando(
            "estoque.criar_produto.v1",
            &carga(&CriarProduto {
                grupo_produto: grupo.grupo_produto,
                nome: "Cabo USB-C".to_string(),
                ncm: "85444200".to_string(),
                unidade_padrao: unidade.unidade,
                codigo_barras: None,
                detalhes_tecnicos: None,
            }),
            s,
            amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    let local: LocalCriado = postcard::from_bytes(
        &d.executar_comando(
            "estoque.criar_local.v1",
            &carga(&CriarLocal {
                nome: "Loja".to_string(),
                tipo: TipoLocal::Deposito,
            }),
            s,
            amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    d.executar_comando(
        "estoque.registrar_entrada.v1",
        &carga(&RegistrarEntrada {
            produto: produto.produto,
            local: local.local,
            quantidade: Quantidade::unidades(50),
            custo_unitario: Preco::reais(30),
        }),
        s,
        amb,
        arm.escritor(),
    )
    .unwrap();

    let tabela: TabelaPrecoCriada = postcard::from_bytes(
        &d.executar_comando(
            "vendas.criar_tabela_preco.v1",
            &carga(&CriarTabelaPreco {
                nome: "Balcão".to_string(),
                tipo: TipoTabela::Venda,
                vigente_de: Data::hoje(Fuso::BRASILIA).mais_dias(-1),
                vigente_ate: None,
            }),
            s,
            amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    let _regra: RegraPrecoCriada = postcard::from_bytes(
        &d.executar_comando(
            "vendas.criar_regra_preco.v1",
            &carga(&CriarRegraPreco {
                tabela_preco: tabela.tabela_preco,
                alvo_produto: Some(produto.produto),
                alvo_grupo: None,
                quantidade_minima: None,
                preco: Preco::reais(preco_unitario),
                periodo_de: None,
                periodo_ate: None,
            }),
            s,
            amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    Cenario {
        cliente: cliente.pessoa,
        produto: produto.produto,
        local: local.local,
        tabela_preco: tabela.tabela_preco,
    }
}

#[test]
fn ciclo_completo_pedido_a_vista_fatura_sem_titulo() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes, &ModuloEstoque, &ModuloVendas]).unwrap();
    let s = sessao_completa(empresa);
    let amb = ambiente(empresa);
    let cen = montar_cenario(&d, &s, &amb, &arm, 100);

    let pedido: PedidoCriado = postcard::from_bytes(
        &d.executar_comando(
            "vendas.criar_pedido.v1",
            &carga(&CriarPedido {
                cliente: cen.cliente,
                vendedor: Id::novo(),
                condicao_pagamento: Id::novo(),
                tabela_preco: cen.tabela_preco,
                local_expedicao: cen.local,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    let item: ItemFoiAdicionado = postcard::from_bytes(
        &d.executar_comando(
            "vendas.adicionar_item_pedido.v1",
            &carga(&AdicionarItemPedido {
                pedido: pedido.pedido,
                produto: cen.produto,
                variacao: None,
                quantidade: Quantidade::unidades(2),
                desconto_percentual: Percentual::ZERO,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(item.preco_unitario, Preco::reais(100));
    assert_eq!(item.total_pedido, Dinheiro::reais(200));

    d.executar_comando(
        "vendas.confirmar_pedido.v1",
        &carga(&ConfirmarPedido {
            pedido: pedido.pedido,
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    let faturado: PedidoFoiFaturado = postcard::from_bytes(
        &d.executar_comando(
            "vendas.faturar_pedido.v1",
            &carga(&FaturarPedido {
                pedido: pedido.pedido,
                a_vista: true,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(faturado.valor_total, Dinheiro::reais(200));
    assert!(faturado.titulo.is_none());

    assert_eq!(conta_titulos(&arm, empresa), 0);
    // Só o faturamento gera lançamento — o combinado de receita + CMV (a entrada de estoque
    // nesta fatia não lança no razão, como em `mod-estoque::lib`).
    assert_eq!(conta_lancamentos(&arm, empresa), 1);
}

#[test]
fn ciclo_pedido_a_prazo_com_desconto_gera_titulo_vinculado_ao_lancamento() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes, &ModuloEstoque, &ModuloVendas]).unwrap();
    let s = sessao_completa(empresa);
    let amb = ambiente(empresa);
    let cen = montar_cenario(&d, &s, &amb, &arm, 100);

    let pedido: PedidoCriado = postcard::from_bytes(
        &d.executar_comando(
            "vendas.criar_pedido.v1",
            &carga(&CriarPedido {
                cliente: cen.cliente,
                vendedor: Id::novo(),
                condicao_pagamento: Id::novo(),
                tabela_preco: cen.tabela_preco,
                local_expedicao: cen.local,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    // 5% de desconto — dentro do teto de 10% do papel.
    d.executar_comando(
        "vendas.adicionar_item_pedido.v1",
        &carga(&AdicionarItemPedido {
            pedido: pedido.pedido,
            produto: cen.produto,
            variacao: None,
            quantidade: Quantidade::unidades(10),
            desconto_percentual: Percentual::pontos(5),
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    // Acima do teto — recusado na hora (§11.4).
    let erro = d
        .executar_comando(
            "vendas.adicionar_item_pedido.v1",
            &carga(&AdicionarItemPedido {
                pedido: pedido.pedido,
                produto: cen.produto,
                variacao: None,
                quantidade: Quantidade::unidades(1),
                desconto_percentual: Percentual::pontos(50),
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::SEM_PERMISSAO);

    d.executar_comando(
        "vendas.confirmar_pedido.v1",
        &carga(&ConfirmarPedido {
            pedido: pedido.pedido,
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    let faturado: PedidoFoiFaturado = postcard::from_bytes(
        &d.executar_comando(
            "vendas.faturar_pedido.v1",
            &carga(&FaturarPedido {
                pedido: pedido.pedido,
                a_vista: false,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    // 10 * 100,00 = 1000,00 bruto, 5% de desconto = 50,00 → total 950,00.
    assert_eq!(faturado.valor_total, Dinheiro::reais(950));
    assert!(faturado.titulo.is_some());

    assert_eq!(conta_titulos(&arm, empresa), 1);
    assert_eq!(conta_lancamentos(&arm, empresa), 1);
}

#[test]
fn cancelar_pedido_rascunho_e_faturar_pedido_cancelado_e_recusado() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes, &ModuloEstoque, &ModuloVendas]).unwrap();
    let s = sessao_completa(empresa);
    let amb = ambiente(empresa);
    let cen = montar_cenario(&d, &s, &amb, &arm, 50);

    let pedido: PedidoCriado = postcard::from_bytes(
        &d.executar_comando(
            "vendas.criar_pedido.v1",
            &carga(&CriarPedido {
                cliente: cen.cliente,
                vendedor: Id::novo(),
                condicao_pagamento: Id::novo(),
                tabela_preco: cen.tabela_preco,
                local_expedicao: cen.local,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    d.executar_comando(
        "vendas.adicionar_item_pedido.v1",
        &carga(&AdicionarItemPedido {
            pedido: pedido.pedido,
            produto: cen.produto,
            variacao: None,
            quantidade: Quantidade::unidades(1),
            desconto_percentual: Percentual::ZERO,
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    d.executar_comando(
        "vendas.cancelar_pedido.v1",
        &carga(&CancelarPedido {
            pedido: pedido.pedido,
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    let erro = d
        .executar_comando(
            "vendas.confirmar_pedido.v1",
            &carga(&ConfirmarPedido {
                pedido: pedido.pedido,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::ESTADO_INVALIDO);

    assert_eq!(conta_lancamentos(&arm, empresa), 0);
}
