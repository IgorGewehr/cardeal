//! O ciclo completo de uma venda de balcão atravessando o `Despachante` real contra SQLite:
//! cliente (`mod-clientes`) → produto em estoque com saldo real (`mod-estoque`) → tabela e
//! regra de preço (`mod-vendas`) → caixa cadastrado e aberto (`mod-financeiro`) → cupom aberto,
//! item adicionado com preço resolvido, finalizado com uma ou mais formas de pagamento.
//! Autorização, transação e persistência — tudo junto, exatamente como vai rodar em produção.

#![allow(clippy::result_large_err)] // `ErroArmazenamento` carrega detalhes de propósito

use cardeal_analytics::ReceitaDoProduto;
use cardeal_auth::{
    AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel as PapelAuth, Sessao, ValorLimite,
};
use cardeal_kernel::Data;
use cardeal_kernel::{
    CodigoErro, Dinheiro, Fuso, Id, Instante, Percentual, Periodo, Preco, Quantidade,
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
use mod_financeiro::{
    AbrirCaixa, CadastrarCaixa, CaixaCadastrado, CaixaFoiAberto, ModuloFinanceiro,
};
use mod_pdv::{
    AbrirCupom, AdicionarItem, AplicarDescontoItem, CancelarCupom, CancelarItem, CupomAberto,
    EstadoCupom, EstornarVenda, FinalizarVenda, FormaPagamentoPdv, IdentificarCliente,
    ItemFoiAdicionado, ModuloPdv, PagamentoInformado, PrecoConsultado, PrecoDoProduto,
    ReceitaPorProduto, VendaDoPeriodo, VendaFoiEstornada, VendaFoiFinalizada, VendasDoPeriodo,
};
use mod_vendas::{
    CriarRegraPreco, CriarTabelaPreco, ModuloVendas, RegraPrecoCriada, TabelaPrecoCriada,
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
        ModuloFinanceiro.migracoes(),
        ModuloVendas.migracoes(),
        ModuloPdv.migracoes(),
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
    rm.registrar(&mod_pdv::MANIFESTO).unwrap();
    let efetivo = rm
        .resolver(&PedidoAtivacao::nova().com_modulo("pdv"))
        .unwrap();
    Ambiente::novo(empresa, efetivo)
}

fn sessao_completa(empresa: Id) -> Sessao {
    let mut papel = PapelAuth::novo(empresa, "Operador de caixa");
    for p in [
        "clientes.pessoa.criar",
        "estoque.produto.criar",
        "estoque.local.criar",
        "estoque.movimento.entrada",
        "vendas.tabela_preco.criar",
        "vendas.tabela_preco.editar",
        "financeiro.caixa.cadastrar",
        "financeiro.caixa.abrir",
        "pdv.venda.editar",
        "pdv.venda.finalizar",
        "pdv.desconto.aplicar",
        "pdv.item.cancelar",
        "pdv.cupom.cancelar",
        "pdv.preco.consultar",
        "pdv.margem.ver",
        "pdv.venda.estornar",
        "pdv.venda.ver",
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

fn conta_papel_caixa(arm: &Armazenamento, empresa: Id) -> Id {
    let bytes: Vec<u8> = arm
        .leitor()
        .consultar(|c| {
            c.query_row(
                "SELECT id FROM razao_conta WHERE empresa = ?1 AND papel_padrao = 'Caixa'",
                [empresa.em_bytes().as_slice()],
                |r| r.get(0),
            )
            .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
        })
        .unwrap();
    Id::de_bytes(bytes.try_into().expect("id tem 16 bytes"))
}

struct Cenario {
    cliente: Id,
    produto: Id,
    local: Id,
    tabela_preco: Id,
    sessao_caixa: Id,
}

fn montar_cenario(
    d: &Despachante,
    s: &Sessao,
    amb: &Ambiente,
    arm: &Armazenamento,
    empresa: Id,
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
                nome: "Capinha de celular".to_string(),
                ncm: "39269090".to_string(),
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
            custo_unitario: Preco::reais(10),
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

    let caixa: CaixaCadastrado = postcard::from_bytes(
        &d.executar_comando(
            "financeiro.cadastrar_caixa.v1",
            &carga(&CadastrarCaixa {
                nome: "Caixa 1".to_string(),
                local_operacao: None,
                conta_razao: conta_papel_caixa(arm, empresa),
                permite_negativo: false,
            }),
            s,
            amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    let sessao_caixa: CaixaFoiAberto = postcard::from_bytes(
        &d.executar_comando(
            "financeiro.abrir_caixa.v1",
            &carga(&AbrirCaixa {
                caixa: caixa.caixa,
                valor_abertura: Dinheiro::ZERO,
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
        sessao_caixa: sessao_caixa.sessao,
    }
}

fn abrir_cupom(
    d: &Despachante,
    s: &Sessao,
    amb: &Ambiente,
    arm: &Armazenamento,
    cen: &Cenario,
) -> CupomAberto {
    postcard::from_bytes(
        &d.executar_comando(
            "pdv.abrir_cupom.v1",
            &carga(&AbrirCupom {
                sessao_caixa: cen.sessao_caixa,
                terminal: Id::novo(),
                serie_fiscal: 1,
                cliente: Some(cen.cliente),
                tabela_preco: cen.tabela_preco,
                local_expedicao: cen.local,
            }),
            s,
            amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn ciclo_completo_venda_de_balcao_com_um_pagamento() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[
        &ModuloClientes,
        &ModuloEstoque,
        &ModuloFinanceiro,
        &ModuloVendas,
        &ModuloPdv,
    ])
    .unwrap();
    let s = sessao_completa(empresa);
    let amb = ambiente(empresa);
    let cen = montar_cenario(&d, &s, &amb, &arm, empresa, 25);

    let cupom = abrir_cupom(&d, &s, &amb, &arm, &cen);
    assert_eq!(cupom.numero_terminal, 1);

    let item: ItemFoiAdicionado = postcard::from_bytes(
        &d.executar_comando(
            "pdv.adicionar_item.v1",
            &carga(&AdicionarItem {
                cupom: cupom.cupom,
                produto: cen.produto,
                variacao: None,
                quantidade: Quantidade::unidades(2),
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(item.preco_unitario, Preco::reais(25));
    assert_eq!(item.total_cupom, Dinheiro::reais(50));

    let finalizada: VendaFoiFinalizada = postcard::from_bytes(
        &d.executar_comando(
            "pdv.finalizar_venda.v1",
            &carga(&FinalizarVenda {
                cupom: cupom.cupom,
                pagamentos: vec![PagamentoInformado {
                    forma: mod_pdv::FormaPagamentoPdv::Dinheiro,
                    valor: Dinheiro::reais(50),
                }],
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(finalizada.total, Dinheiro::reais(50));
    // 1 lançamento — a venda de balcão (a entrada de estoque nesta fatia não lança no razão).
    assert_eq!(conta_lancamentos(&arm, empresa), 1);
}

#[test]
fn venda_com_multiplas_formas_e_desconto_de_item() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[
        &ModuloClientes,
        &ModuloEstoque,
        &ModuloFinanceiro,
        &ModuloVendas,
        &ModuloPdv,
    ])
    .unwrap();
    let s = sessao_completa(empresa);
    let amb = ambiente(empresa);
    let cen = montar_cenario(&d, &s, &amb, &arm, empresa, 25);
    let cupom = abrir_cupom(&d, &s, &amb, &arm, &cen);

    let item: ItemFoiAdicionado = postcard::from_bytes(
        &d.executar_comando(
            "pdv.adicionar_item.v1",
            &carga(&AdicionarItem {
                cupom: cupom.cupom,
                produto: cen.produto,
                variacao: None,
                quantidade: Quantidade::unidades(4), // 100,00
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    d.executar_comando(
        "pdv.aplicar_desconto_item.v1",
        &carga(&AplicarDescontoItem {
            cupom: cupom.cupom,
            item: item.item,
            desconto_percentual: Percentual::pontos(10), // -10,00 -> total 90,00
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    // Acima do teto do papel (10%) é recusado.
    let erro = d
        .executar_comando(
            "pdv.aplicar_desconto_item.v1",
            &carga(&AplicarDescontoItem {
                cupom: cupom.cupom,
                item: item.item,
                desconto_percentual: Percentual::pontos(50),
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::SEM_PERMISSAO);

    // Soma divergente da forma de pagamento é recusada.
    let erro = d
        .executar_comando(
            "pdv.finalizar_venda.v1",
            &carga(&FinalizarVenda {
                cupom: cupom.cupom,
                pagamentos: vec![PagamentoInformado {
                    forma: mod_pdv::FormaPagamentoPdv::Dinheiro,
                    valor: Dinheiro::reais(80),
                }],
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::REGRA_VIOLADA);

    let finalizada: VendaFoiFinalizada = postcard::from_bytes(
        &d.executar_comando(
            "pdv.finalizar_venda.v1",
            &carga(&FinalizarVenda {
                cupom: cupom.cupom,
                pagamentos: vec![
                    PagamentoInformado {
                        forma: mod_pdv::FormaPagamentoPdv::Dinheiro,
                        valor: Dinheiro::reais(50),
                    },
                    PagamentoInformado {
                        forma: mod_pdv::FormaPagamentoPdv::Credito,
                        valor: Dinheiro::reais(40),
                    },
                ],
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(finalizada.total, Dinheiro::reais(90));
}

#[test]
fn cancelar_item_recalcula_total_e_cancelar_cupom_impede_finalizar() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[
        &ModuloClientes,
        &ModuloEstoque,
        &ModuloFinanceiro,
        &ModuloVendas,
        &ModuloPdv,
    ])
    .unwrap();
    let s = sessao_completa(empresa);
    let amb = ambiente(empresa);
    let cen = montar_cenario(&d, &s, &amb, &arm, empresa, 10);
    let cupom = abrir_cupom(&d, &s, &amb, &arm, &cen);

    let item: ItemFoiAdicionado = postcard::from_bytes(
        &d.executar_comando(
            "pdv.adicionar_item.v1",
            &carga(&AdicionarItem {
                cupom: cupom.cupom,
                produto: cen.produto,
                variacao: None,
                quantidade: Quantidade::unidades(1),
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    d.executar_comando(
        "pdv.cancelar_item.v1",
        &carga(&CancelarItem {
            cupom: cupom.cupom,
            item: item.item,
            motivo: "cliente desistiu do item".to_string(),
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    d.executar_comando(
        "pdv.cancelar_cupom.v1",
        &carga(&CancelarCupom {
            cupom: cupom.cupom,
            motivo: "cliente desistiu da compra".to_string(),
            autorizado_por: Id::novo(),
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    let erro = d
        .executar_comando(
            "pdv.finalizar_venda.v1",
            &carga(&FinalizarVenda {
                cupom: cupom.cupom,
                pagamentos: vec![PagamentoInformado {
                    forma: mod_pdv::FormaPagamentoPdv::Dinheiro,
                    valor: Dinheiro::ZERO,
                }],
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::ESTADO_INVALIDO);
    assert_eq!(conta_lancamentos(&arm, empresa), 0);
}

#[test]
fn consulta_de_preco_devolve_o_mesmo_preco_que_a_venda_cobra() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[
        &ModuloClientes,
        &ModuloEstoque,
        &ModuloFinanceiro,
        &ModuloVendas,
        &ModuloPdv,
    ])
    .unwrap();
    let s = sessao_completa(empresa);
    let amb = ambiente(empresa);
    let cen = montar_cenario(&d, &s, &amb, &arm, empresa, 25);

    let consulta = |produto: Id| {
        d.executar_consulta(
            "pdv.preco_do_produto.v1",
            &carga(&PrecoDoProduto {
                tabela_preco: cen.tabela_preco,
                produto,
                quantidade: Quantidade::unidades(1),
            }),
            &s,
            &amb,
            arm.leitor(),
        )
    };

    let achado: PrecoConsultado = postcard::from_bytes(&consulta(cen.produto).unwrap()).unwrap();
    assert_eq!(achado.preco, Preco::reais(25));
    assert!(
        achado.disponivel > Quantidade::ZERO,
        "traz o saldo do produto"
    );

    // O mesmo preço que `AdicionarItem` congela no item.
    let cupom = abrir_cupom(&d, &s, &amb, &arm, &cen);
    let item: ItemFoiAdicionado = postcard::from_bytes(
        &d.executar_comando(
            "pdv.adicionar_item.v1",
            &carga(&AdicionarItem {
                cupom: cupom.cupom,
                produto: cen.produto,
                variacao: None,
                quantidade: Quantidade::unidades(1),
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(achado.preco, item.preco_unitario);

    // Produto que não existe: erro conversável, não pânico.
    assert!(consulta(Id::novo()).is_err());
}

fn cliente_do_cupom(arm: &Armazenamento, cupom: Id) -> Option<Vec<u8>> {
    arm.leitor()
        .consultar(|c| {
            c.query_row(
                "SELECT cliente FROM pdv_cupom WHERE id = ?1",
                [cupom.em_bytes().as_slice()],
                |r| r.get(0),
            )
            .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
        })
        .unwrap()
}

#[test]
fn identificar_cliente_no_cupom_aberto_e_recusar_depois_de_cancelado() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[
        &ModuloClientes,
        &ModuloEstoque,
        &ModuloFinanceiro,
        &ModuloVendas,
        &ModuloPdv,
    ])
    .unwrap();
    let s = sessao_completa(empresa);
    let amb = ambiente(empresa);
    let cen = montar_cenario(&d, &s, &amb, &arm, empresa, 25);
    let cupom = abrir_cupom(&d, &s, &amb, &arm, &cen);
    let identificar = |cliente: Option<Id>| {
        d.executar_comando(
            "pdv.identificar_cliente.v1",
            &carga(&IdentificarCliente {
                cupom: cupom.cupom,
                cliente,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
    };

    // `abrir_cupom` já abre com o cliente do cenário: troca e depois remove.
    let outro = Id::novo();
    identificar(Some(outro)).unwrap();
    assert_eq!(
        cliente_do_cupom(&arm, cupom.cupom),
        Some(outro.em_bytes().to_vec())
    );
    identificar(None).unwrap();
    assert_eq!(cliente_do_cupom(&arm, cupom.cupom), None);

    // Depois de cancelado, o cliente não muda mais.
    d.executar_comando(
        "pdv.cancelar_cupom.v1",
        &carga(&CancelarCupom {
            cupom: cupom.cupom,
            motivo: "teste".to_string(),
            autorizado_por: Id::novo(),
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();
    assert!(identificar(Some(outro)).is_err());
}

#[test]
fn receita_por_produto_soma_so_cupons_finalizados_sem_itens_cancelados() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[
        &ModuloClientes,
        &ModuloEstoque,
        &ModuloFinanceiro,
        &ModuloVendas,
        &ModuloPdv,
    ])
    .unwrap();
    let s = sessao_completa(empresa);
    let amb = ambiente(empresa);
    let cen = montar_cenario(&d, &s, &amb, &arm, empresa, 25);

    let adicionar = |cupom: Id, qtd: i64| -> ItemFoiAdicionado {
        postcard::from_bytes(
            &d.executar_comando(
                "pdv.adicionar_item.v1",
                &carga(&AdicionarItem {
                    cupom,
                    produto: cen.produto,
                    variacao: None,
                    quantidade: Quantidade::unidades(qtd),
                }),
                &s,
                &amb,
                arm.escritor(),
            )
            .unwrap(),
        )
        .unwrap()
    };

    // Finalizado: 2 vendidos (50) + 1 item cancelado antes de finalizar.
    let finalizado = abrir_cupom(&d, &s, &amb, &arm, &cen);
    adicionar(finalizado.cupom, 2);
    let cancelado_item = adicionar(finalizado.cupom, 1);
    d.executar_comando(
        "pdv.cancelar_item.v1",
        &carga(&CancelarItem {
            cupom: finalizado.cupom,
            item: cancelado_item.item,
            motivo: "cliente desistiu do item".to_string(),
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();
    d.executar_comando(
        "pdv.finalizar_venda.v1",
        &carga(&FinalizarVenda {
            cupom: finalizado.cupom,
            pagamentos: vec![PagamentoInformado {
                forma: mod_pdv::FormaPagamentoPdv::Dinheiro,
                valor: Dinheiro::reais(50),
            }],
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    // Cupom cancelado e cupom ainda em andamento não contam.
    let cancelado = abrir_cupom(&d, &s, &amb, &arm, &cen);
    adicionar(cancelado.cupom, 4);
    d.executar_comando(
        "pdv.cancelar_cupom.v1",
        &carga(&CancelarCupom {
            cupom: cancelado.cupom,
            motivo: "cliente desistiu da compra".to_string(),
            autorizado_por: Id::novo(),
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();
    let aberto = abrir_cupom(&d, &s, &amb, &arm, &cen);
    adicionar(aberto.cupom, 7);

    let receita = |periodo: Periodo| -> Vec<ReceitaDoProduto> {
        postcard::from_bytes(
            &d.executar_consulta(
                "pdv.receita_por_produto.v1",
                &carga(&ReceitaPorProduto { periodo }),
                &s,
                &amb,
                arm.leitor(),
            )
            .unwrap(),
        )
        .unwrap()
    };
    let hoje = Data::hoje(Fuso::BRASILIA);
    let r = receita(Periodo::novo(hoje.mais_dias(-1), hoje.mais_dias(1)));
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].produto, cen.produto);
    assert_eq!(r[0].quantidade, Quantidade::unidades(2));
    assert_eq!(r[0].receita, Dinheiro::reais(50));
    assert!(receita(Periodo::novo(hoje.mais_dias(-30), hoje.mais_dias(-10))).is_empty());
}

/// Soma das partidas (realizadas e estornadas) da conta de um papel — o saldo que o caixa vê.
fn saldo_do_papel(arm: &Armazenamento, empresa: Id, papel: &str) -> i64 {
    arm.leitor()
        .consultar(|c| {
            c.query_row(
                "SELECT COALESCE(SUM(rp.valor), 0)
                 FROM razao_partida rp
                 JOIN razao_lancamento rl ON rl.id = rp.lancamento
                 JOIN razao_conta rc ON rc.id = rp.conta
                 WHERE rc.empresa = ?1 AND rc.papel_padrao = ?2
                   AND rl.estado IN ('Realizado', 'Estornado')",
                rusqlite::params![empresa.em_bytes().as_slice(), papel],
                |r| r.get(0),
            )
            .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
        })
        .unwrap()
}

/// `(quantidade disponível, custo médio)` do produto no local, em unidades internas.
fn saldo_de_estoque(arm: &Armazenamento, produto: Id) -> (i64, i64) {
    arm.leitor()
        .consultar(|c| {
            c.query_row(
                "SELECT quantidade_disponivel, custo_medio FROM estoque_saldo_local WHERE produto = ?1",
                [produto.em_bytes().as_slice()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
        })
        .unwrap()
}

#[test]
fn estornar_venda_finalizada_devolve_estoque_e_zera_o_razao() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[
        &ModuloClientes,
        &ModuloEstoque,
        &ModuloFinanceiro,
        &ModuloVendas,
        &ModuloPdv,
    ])
    .unwrap();
    let s = sessao_completa(empresa);
    let amb = ambiente(empresa);
    let cen = montar_cenario(&d, &s, &amb, &arm, empresa, 25);
    let estoque_antes = saldo_de_estoque(&arm, cen.produto);

    let cupom = abrir_cupom(&d, &s, &amb, &arm, &cen);
    d.executar_comando(
        "pdv.adicionar_item.v1",
        &carga(&AdicionarItem {
            cupom: cupom.cupom,
            produto: cen.produto,
            variacao: None,
            quantidade: Quantidade::unidades(2),
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();
    let estornar = |motivo: &str, sessao: &Sessao| {
        d.executar_comando(
            "pdv.estornar_venda.v1",
            &carga(&EstornarVenda {
                cupom: cupom.cupom,
                motivo: motivo.to_string(),
                autorizado_por: Id::novo(),
            }),
            sessao,
            &amb,
            arm.escritor(),
        )
    };
    // Em andamento não se estorna: cancela-se (F8).
    assert_eq!(
        estornar("Cliente desistiu da compra", &s)
            .unwrap_err()
            .codigo,
        CodigoErro::ESTADO_INVALIDO
    );
    d.executar_comando(
        "pdv.finalizar_venda.v1",
        &carga(&FinalizarVenda {
            cupom: cupom.cupom,
            pagamentos: vec![
                PagamentoInformado {
                    forma: FormaPagamentoPdv::Dinheiro,
                    valor: Dinheiro::reais(30),
                },
                PagamentoInformado {
                    forma: FormaPagamentoPdv::Pix,
                    valor: Dinheiro::reais(20),
                },
            ],
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();
    assert_eq!(saldo_do_papel(&arm, empresa, "Caixa"), 3_000);
    assert_eq!(
        saldo_de_estoque(&arm, cen.produto).0,
        estoque_antes.0 - 20_000
    );

    // Motivo curto e falta de permissão são recusados antes de mexer em qualquer coisa.
    assert_eq!(
        estornar("errei", &s).unwrap_err().codigo,
        CodigoErro::ENTRADA_INVALIDA
    );
    let mut sem_estorno = PapelAuth::novo(empresa, "Caixa sem estorno");
    for p in ["pdv.venda.editar", "pdv.venda.finalizar"] {
        sem_estorno = sem_estorno.com_permissao(p);
    }
    let caixa_simples = Sessao::abrir(EmissaoSessao::padrao(
        Id::novo(),
        Id::novo(),
        Escopo::empresa_inteira(empresa),
        AutorizacoesEfetivas::consolidar([&sem_estorno]),
        Instante::EPOCA,
    ));
    assert_eq!(
        estornar("Cliente desistiu da compra", &caixa_simples)
            .unwrap_err()
            .codigo,
        CodigoErro::SEM_PERMISSAO
    );

    let estorno: VendaFoiEstornada =
        postcard::from_bytes(&estornar("Cliente desistiu da compra", &s).unwrap()).unwrap();
    assert_eq!(estorno.lancamentos.len(), 1);
    assert_eq!(estorno.itens_devolvidos, 1);
    assert_eq!(estorno.devolver.len(), 2, "devolve forma por forma");

    // O Razão volta ao que era: caixa, Pix em trânsito, receita, CMV e estoque.
    for papel in [
        "Caixa",
        "ValoresEmTransito",
        "ReceitaVendas",
        "Cmv",
        "EstoqueMercadorias",
    ] {
        assert_eq!(saldo_do_papel(&arm, empresa, papel), 0, "{papel}");
    }
    // E o estoque também, ao mesmo custo médio.
    assert_eq!(saldo_de_estoque(&arm, cen.produto), estoque_antes);

    // Não se estorna duas vezes.
    assert_eq!(
        estornar("Cliente desistiu da compra", &s)
            .unwrap_err()
            .codigo,
        CodigoErro::ESTADO_INVALIDO
    );

    // A lista de vendas do dia mostra o estorno.
    let hoje = Data::hoje(Fuso::BRASILIA);
    let vendas: Vec<VendaDoPeriodo> = postcard::from_bytes(
        &d.executar_consulta(
            "pdv.vendas_do_periodo.v1",
            &carga(&VendasDoPeriodo {
                de: hoje.mais_dias(-1),
                ate: hoje.mais_dias(1),
            }),
            &s,
            &amb,
            arm.leitor(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(vendas.len(), 1);
    assert_eq!(vendas[0].estado, EstadoCupom::Estornado);
    assert_eq!(vendas[0].total, Dinheiro::reais(50));
    assert_eq!(vendas[0].pagamentos.len(), 2);
    assert_eq!(
        vendas[0].estorno.as_ref().map(|e| e.motivo.as_str()),
        Some("Cliente desistiu da compra")
    );
}
