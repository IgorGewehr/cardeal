//! Os comandos de estoque atravessando o `Despachante` real contra SQLite: autorização,
//! transação e persistência — tudo junto.

#![allow(clippy::result_large_err)] // `ErroArmazenamento` carrega detalhes de propósito

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel as PapelAuth, Sessao};
use cardeal_kernel::{CodigoErro, Id, Instante, Preco, Quantidade};
use cardeal_ledger::semear_plano_padrao;
use cardeal_modkit::{Ambiente, Despachante, Modulo, PedidoAtivacao, RegistroModulos};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use mod_estoque::{
    AjustarSaldo, CriarGrupoProduto, CriarLocal, CriarProduto, CriarUnidade, DetalhesTecnicos,
    EditarDetalhesTecnicosProduto, EntradaRegistrada, GrupoProdutoCriado, ItemProdutoComSaldo,
    LocalCriado, ModuloEstoque, Produto, ProdutoCriado, ProdutoPorCodigoBarras, ProdutosComSaldo,
    RegistrarEntrada, RegistrarSaida, SaidaRegistrada, SaldoAjustado, TipoLocal, UnidadeCriada,
    MANIFESTO,
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

const PERMISSOES_CADASTRO: &[&str] = &["estoque.produto.criar", "estoque.local.criar"];

fn cadastro_basico(d: &Despachante, arm: &Armazenamento, empresa: Id, s: &Sessao) -> (Id, Id, Id) {
    let grupo = d
        .executar_comando(
            "estoque.criar_grupo_produto.v1",
            &carga(&CriarGrupoProduto {
                codigo: "PECAS".to_string(),
                nome: "Peças".to_string(),
                pai: None,
            }),
            s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap();
    let grupo: GrupoProdutoCriado = postcard::from_bytes(&grupo).unwrap();

    let unidade = d
        .executar_comando(
            "estoque.criar_unidade.v1",
            &carga(&CriarUnidade {
                sigla: "UN".to_string(),
                nome: "Unidade".to_string(),
                fracionavel: false,
            }),
            s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap();
    let unidade: UnidadeCriada = postcard::from_bytes(&unidade).unwrap();

    let produto = d
        .executar_comando(
            "estoque.criar_produto.v1",
            &carga(&CriarProduto {
                grupo_produto: grupo.grupo_produto,
                nome: "Bateria original 6 células".to_string(),
                ncm: "85076000".to_string(),
                unidade_padrao: unidade.unidade,
                codigo_barras: None,
                detalhes_tecnicos: None,
            }),
            s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap();
    let produto: ProdutoCriado = postcard::from_bytes(&produto).unwrap();

    let local = d
        .executar_comando(
            "estoque.criar_local.v1",
            &carga(&CriarLocal {
                nome: "Depósito".to_string(),
                tipo: TipoLocal::Deposito,
            }),
            s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap();
    let local: LocalCriado = postcard::from_bytes(&local).unwrap();

    (produto.produto, local.local, unidade.unidade)
}

#[test]
fn entrada_recalcula_custo_medio_e_saida_usa_o_vigente() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloEstoque]).unwrap();
    let s = sessao(
        empresa,
        &[
            "estoque.produto.criar",
            "estoque.local.criar",
            "estoque.movimento.entrada",
            "estoque.movimento.saida",
        ],
    );
    let (produto, local, _unidade) = cadastro_basico(&d, &arm, empresa, &s);

    let saida1 = d
        .executar_comando(
            "estoque.registrar_entrada.v1",
            &carga(&RegistrarEntrada {
                produto,
                local,
                quantidade: Quantidade::unidades(10),
                custo_unitario: Preco::reais(100),
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap();
    let e1: EntradaRegistrada = postcard::from_bytes(&saida1).unwrap();
    assert_eq!(e1.custo_medio, Preco::reais(100));

    let saida2 = d
        .executar_comando(
            "estoque.registrar_entrada.v1",
            &carga(&RegistrarEntrada {
                produto,
                local,
                quantidade: Quantidade::unidades(10),
                custo_unitario: Preco::reais(200),
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap();
    let e2: EntradaRegistrada = postcard::from_bytes(&saida2).unwrap();
    // (10*100 + 10*200) / 20 = 150
    assert_eq!(e2.custo_medio, Preco::reais(150));

    let saida = d
        .executar_comando(
            "estoque.registrar_saida.v1",
            &carga(&RegistrarSaida {
                produto,
                local,
                quantidade: Quantidade::unidades(5),
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap();
    let sr: SaidaRegistrada = postcard::from_bytes(&saida).unwrap();
    assert_eq!(sr.custo_unitario, Preco::reais(150));
    assert!(!sr.gerou_divergencia);
}

#[test]
fn saida_acima_do_saldo_e_aceita_e_sinalizada() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloEstoque]).unwrap();
    let s = sessao(
        empresa,
        &[
            "estoque.produto.criar",
            "estoque.local.criar",
            "estoque.movimento.entrada",
            "estoque.movimento.saida",
        ],
    );
    let (produto, local, _unidade) = cadastro_basico(&d, &arm, empresa, &s);

    d.executar_comando(
        "estoque.registrar_entrada.v1",
        &carga(&RegistrarEntrada {
            produto,
            local,
            quantidade: Quantidade::unidades(5),
            custo_unitario: Preco::reais(10),
        }),
        &s,
        &ambiente(empresa),
        arm.escritor(),
    )
    .unwrap();

    let saida = d
        .executar_comando(
            "estoque.registrar_saida.v1",
            &carga(&RegistrarSaida {
                produto,
                local,
                quantidade: Quantidade::unidades(8),
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap();
    let sr: SaidaRegistrada = postcard::from_bytes(&saida).unwrap();
    assert!(sr.gerou_divergencia);
}

#[test]
fn sem_permissao_registrar_entrada_e_recusado() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloEstoque]).unwrap();
    let s = sessao(empresa, PERMISSOES_CADASTRO);
    let (produto, local, _unidade) = cadastro_basico(&d, &arm, empresa, &s);

    let erro = d
        .executar_comando(
            "estoque.registrar_entrada.v1",
            &carga(&RegistrarEntrada {
                produto,
                local,
                quantidade: Quantidade::unidades(1),
                custo_unitario: Preco::reais(1),
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::SEM_PERMISSAO);
}

#[test]
fn produtos_com_saldo_soma_entre_locais() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloEstoque]).unwrap();
    let s = sessao(
        empresa,
        &[
            "estoque.produto.criar",
            "estoque.local.criar",
            "estoque.movimento.entrada",
            "estoque.produto.ver",
        ],
    );
    let (produto, local, _unidade) = cadastro_basico(&d, &arm, empresa, &s);

    d.executar_comando(
        "estoque.registrar_entrada.v1",
        &carga(&RegistrarEntrada {
            produto,
            local,
            quantidade: Quantidade::unidades(5),
            custo_unitario: Preco::reais(90),
        }),
        &s,
        &ambiente(empresa),
        arm.escritor(),
    )
    .unwrap();

    let saida = d
        .executar_consulta(
            "estoque.produtos_com_saldo.v1",
            &carga(&ProdutosComSaldo),
            &s,
            &ambiente(empresa),
            arm.leitor(),
        )
        .unwrap();
    let itens: Vec<ItemProdutoComSaldo> = postcard::from_bytes(&saida).unwrap();

    assert_eq!(itens.len(), 1);
    assert_eq!(itens[0].produto, produto);
    assert_eq!(itens[0].disponivel, Quantidade::unidades(5));
    assert_eq!(itens[0].custo_medio, Preco::reais(90));
}

#[test]
fn criar_produto_com_codigo_de_barras_e_encontrado_pelo_bipe() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloEstoque]).unwrap();
    let mut permissoes = PERMISSOES_CADASTRO.to_vec();
    permissoes.push("estoque.produto.ver");
    let s = sessao(empresa, &permissoes);

    let grupo: GrupoProdutoCriado = postcard::from_bytes(
        &d.executar_comando(
            "estoque.criar_grupo_produto.v1",
            &carga(&CriarGrupoProduto {
                codigo: "GERAL".to_string(),
                nome: "Geral".to_string(),
                pai: None,
            }),
            &s,
            &ambiente(empresa),
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
            &s,
            &ambiente(empresa),
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
                nome: "Refrigerante Cola 2L".to_string(),
                ncm: "22021000".to_string(),
                unidade_padrao: unidade.unidade,
                codigo_barras: Some("7894900011517".to_string()),
                detalhes_tecnicos: None,
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    // Código inválido (dígito verificador não confere) é recusado na criação.
    let erro = d
        .executar_comando(
            "estoque.criar_produto.v1",
            &carga(&CriarProduto {
                grupo_produto: grupo.grupo_produto,
                nome: "Outro".to_string(),
                ncm: "22021000".to_string(),
                unidade_padrao: unidade.unidade,
                codigo_barras: Some("7894900011518".to_string()),
                detalhes_tecnicos: None,
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::DOCUMENTO_INVALIDO);

    let achado: Option<Produto> = postcard::from_bytes(
        &d.executar_consulta(
            "estoque.produto_por_codigo_barras.v1",
            &carga(&ProdutoPorCodigoBarras {
                codigo_barras: "7894900011517".to_string(),
            }),
            &s,
            &ambiente(empresa),
            arm.leitor(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(achado.unwrap().id, produto.produto);

    let nenhum: Option<Produto> = postcard::from_bytes(
        &d.executar_consulta(
            "estoque.produto_por_codigo_barras.v1",
            &carga(&ProdutoPorCodigoBarras {
                codigo_barras: "0000000000000".to_string(),
            }),
            &s,
            &ambiente(empresa),
            arm.leitor(),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(nenhum.is_none());
}

#[test]
fn ajustar_saldo_corrige_contagem_errada_e_posta_no_razao() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloEstoque]).unwrap();
    let mut permissoes = PERMISSOES_CADASTRO.to_vec();
    permissoes.push("estoque.movimento.entrada");
    permissoes.push("estoque.movimento.ajustar");
    permissoes.push("estoque.produto.ver");
    let s = sessao(empresa, &permissoes);
    let (produto, local, _unidade) = cadastro_basico(&d, &arm, empresa, &s);

    d.executar_comando(
        "estoque.registrar_entrada.v1",
        &carga(&RegistrarEntrada {
            produto,
            local,
            quantidade: Quantidade::unidades(10),
            custo_unitario: Preco::reais(50),
        }),
        &s,
        &ambiente(empresa),
        arm.escritor(),
    )
    .unwrap();

    // Motivo curto demais é recusado.
    let erro = d
        .executar_comando(
            "estoque.ajustar_saldo.v1",
            &carga(&AjustarSaldo {
                produto,
                local,
                nova_quantidade: Quantidade::unidades(8),
                motivo: "errei".to_string(),
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::ENTRADA_INVALIDA);

    // Digitei 10 na entrada, mas a contagem física real é 8 — corrige o saldo e lança a
    // falta (D Perdas / C Estoque) porque havia custo médio apurado.
    let ajustado: SaldoAjustado = postcard::from_bytes(
        &d.executar_comando(
            "estoque.ajustar_saldo.v1",
            &carga(&AjustarSaldo {
                produto,
                local,
                nova_quantidade: Quantidade::unidades(8),
                motivo: "contagem física divergente".to_string(),
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(ajustado.delta, Quantidade::unidades(-2));
    assert!(ajustado.lancamento.is_some());

    let saldo = d
        .executar_consulta(
            "estoque.produtos_com_saldo.v1",
            &carga(&ProdutosComSaldo),
            &s,
            &ambiente(empresa),
            arm.leitor(),
        )
        .unwrap();
    let itens: Vec<ItemProdutoComSaldo> = postcard::from_bytes(&saldo).unwrap();
    assert_eq!(itens[0].disponivel, Quantidade::unidades(8));

    // Ajustar para a mesma quantidade (nada a corrigir) é recusado.
    let erro = d
        .executar_comando(
            "estoque.ajustar_saldo.v1",
            &carga(&AjustarSaldo {
                produto,
                local,
                nova_quantidade: Quantidade::unidades(8),
                motivo: "sem divergência nenhuma".to_string(),
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::ENTRADA_INVALIDA);
}

#[test]
fn produto_com_detalhes_tecnicos_na_criacao_e_editado_depois() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloEstoque]).unwrap();
    let s = sessao(
        empresa,
        &[
            "estoque.produto.criar",
            "estoque.produto.editar",
            "estoque.produto.ver",
        ],
    );

    let grupo: GrupoProdutoCriado = postcard::from_bytes(
        &d.executar_comando(
            "estoque.criar_grupo_produto.v1",
            &carga(&CriarGrupoProduto {
                codigo: "IC".to_string(),
                nome: "Circuitos Integrados".to_string(),
                pai: None,
            }),
            &s,
            &ambiente(empresa),
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
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    // Cadastro já com os detalhes técnicos preenchidos.
    let produto: ProdutoCriado = postcard::from_bytes(
        &d.executar_comando(
            "estoque.criar_produto.v1",
            &carga(&CriarProduto {
                grupo_produto: grupo.grupo_produto,
                nome: "IC de carga USB-C".to_string(),
                ncm: "85423900".to_string(),
                unidade_padrao: unidade.unidade,
                codigo_barras: Some("40170725".to_string()),
                detalhes_tecnicos: Some(DetalhesTecnicos {
                    fabricante: Some("Texas Instruments".to_string()),
                    codigo_fabricante: Some("BQ25895".to_string()),
                    categoria_tecnica: Some("IC".to_string()),
                    especificacao_tecnica: Some("Carregador Li-Ion 5A".to_string()),
                    compatibilidade: Some("iPhone 11 / 11 Pro".to_string()),
                    garantia_fornecedor_dias: Some(90),
                    localizacao_fisica: Some("Gaveta 3".to_string()),
                }),
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    let achado: Produto = postcard::from_bytes::<Option<Produto>>(
        &d.executar_consulta(
            "estoque.produto_por_codigo_barras.v1",
            &carga(&ProdutoPorCodigoBarras {
                codigo_barras: "40170725".to_string(),
            }),
            &s,
            &ambiente(empresa),
            arm.leitor(),
        )
        .unwrap(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(achado.fabricante.as_deref(), Some("Texas Instruments"));
    assert_eq!(achado.codigo_fabricante.as_deref(), Some("BQ25895"));
    assert_eq!(achado.categoria_tecnica.as_deref(), Some("IC"));
    assert_eq!(achado.garantia_fornecedor_dias, Some(90));
    assert_eq!(achado.localizacao_fisica.as_deref(), Some("Gaveta 3"));

    // Edita depois: troca a localização física e limpa a compatibilidade.
    d.executar_comando(
        "estoque.editar_detalhes_tecnicos_produto.v1",
        &carga(&EditarDetalhesTecnicosProduto {
            produto: produto.produto,
            detalhes: DetalhesTecnicos {
                fabricante: Some("Texas Instruments".to_string()),
                codigo_fabricante: Some("BQ25895".to_string()),
                categoria_tecnica: Some("IC".to_string()),
                especificacao_tecnica: Some("Carregador Li-Ion 5A".to_string()),
                compatibilidade: None,
                garantia_fornecedor_dias: Some(90),
                localizacao_fisica: Some("Gaveta 7".to_string()),
            },
        }),
        &s,
        &ambiente(empresa),
        arm.escritor(),
    )
    .unwrap();

    let editado: Produto = postcard::from_bytes::<Option<Produto>>(
        &d.executar_consulta(
            "estoque.produto_por_codigo_barras.v1",
            &carga(&ProdutoPorCodigoBarras {
                codigo_barras: "40170725".to_string(),
            }),
            &s,
            &ambiente(empresa),
            arm.leitor(),
        )
        .unwrap(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(editado.localizacao_fisica.as_deref(), Some("Gaveta 7"));
    assert_eq!(editado.compatibilidade, None);
}
