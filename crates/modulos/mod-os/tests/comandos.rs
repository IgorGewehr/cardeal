//! O ciclo completo de uma ordem de serviço atravessando o `Despachante` real contra
//! SQLite: cliente (`mod-clientes`) → peça em estoque (`mod-estoque`) → OS aberta, laudo,
//! orçamento, aprovação, execução (consumindo estoque de verdade) → faturamento, que cria
//! um título a receber real no financeiro. Autorização, transação e persistência — tudo
//! junto, exatamente como vai rodar em produção.

#![allow(clippy::result_large_err)] // `ErroArmazenamento` carrega detalhes de propósito

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel as PapelAuth, Sessao};
use cardeal_kernel::{CodigoErro, Dinheiro, Id, Instante, Preco, Quantidade};
use cardeal_ledger::semear_plano_padrao;
use cardeal_modkit::{Ambiente, Despachante, Modulo, PedidoAtivacao, RegistroModulos};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use mod_clientes::{
    CriarPessoa, ModuloClientes, Papel, PessoaCadastrada, TipoDocumento, TipoPessoa,
};
use mod_estoque::{
    AparelhoOrigemRegistrado, CriarGrupoProduto, CriarLocal, CriarProduto, CriarUnidade,
    DetalheDoLotePorCodigo, DetalheLote, EntradaComLoteRegistrada, GrupoProdutoCriado,
    LocalCriado, ModuloEstoque, OrigemLote, ProdutoCriado, RegistrarAparelhoOrigem,
    RegistrarEntradaComLote, TipoLocal, UnidadeCriada,
};
use mod_os::{
    AbrirOrdemServico, AplicarPeca, AprovarOrcamentoOs, BuscarDetalheOrdem, CancelarOrdemServico,
    ConcluirExecucao, DetalheOrdem, EditarDadosDaOrdem, EnviarParaAprovacao, FaturarOrdemServico,
    HistoricoDoEquipamento, IniciarExecucao, ItemAguardandoEstoque, ItemOrcamentoNovo, ModuloOs,
    MontarOrcamentoOs, OrdemServicoAberta, OrdemServicoCancelada, OrdemServicoFaturada,
    OrdensAguardandoAprovacao, OrdensEmAberto, PecaFoiAplicada, PecasAguardandoEstoque,
    RegistrarLaudo, RemoverItemOrcamento, TipoItemOrcamento,
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
        ModuloOs.migracoes(),
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
    rm.registrar(&mod_os::MANIFESTO).unwrap();
    let efetivo = rm
        .resolver(
            &PedidoAtivacao::nova()
                .com_modulo("clientes")
                .com_modulo("estoque")
                .com_modulo("os"),
        )
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
        "estoque.movimento.entrada_com_lote",
        "estoque.aparelho_origem.criar",
        "estoque.lote.ver",
        "os.ordem.criar",
        "os.ordem.editar_dados",
        "os.laudo.registrar",
        "os.orcamento.montar",
        "os.orcamento.enviar",
        "os.orcamento.aprovar",
        "os.execucao.iniciar",
        "os.peca.aplicar",
        "os.execucao.registrar_mao_de_obra",
        "os.execucao.concluir",
        "os.faturar",
        "os.ordem.ver",
        "os.ordem.cancelar",
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
                "SELECT COUNT(*) FROM financeiro_titulo WHERE empresa = ?1 AND origem_modulo = 'os'",
                [empresa.em_bytes().as_slice()],
                |r| r.get(0),
            )
            .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
        })
        .unwrap()
}

#[test]
fn ciclo_completo_de_os_abre_orca_aprova_executa_e_fatura() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes, &ModuloEstoque, &ModuloOs]).unwrap();
    let s = sessao_completa(empresa);
    let amb = ambiente(empresa);

    // Cliente.
    let saida = d
        .executar_comando(
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
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap();
    let cliente: PessoaCadastrada = postcard::from_bytes(&saida).unwrap();

    // Peça em estoque, já com saldo de verdade.
    let saida = d
        .executar_comando(
            "estoque.criar_grupo_produto.v1",
            &carga(&CriarGrupoProduto {
                codigo: "PECAS".to_string(),
                nome: "Peças".to_string(),
                pai: None,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap();
    let grupo: GrupoProdutoCriado = postcard::from_bytes(&saida).unwrap();

    let saida = d
        .executar_comando(
            "estoque.criar_unidade.v1",
            &carga(&CriarUnidade {
                sigla: "UN".to_string(),
                nome: "Unidade".to_string(),
                fracionavel: false,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap();
    let unidade: UnidadeCriada = postcard::from_bytes(&saida).unwrap();

    let saida = d
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
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap();
    let produto: ProdutoCriado = postcard::from_bytes(&saida).unwrap();

    let saida = d
        .executar_comando(
            "estoque.criar_local.v1",
            &carga(&CriarLocal {
                nome: "Depósito".to_string(),
                tipo: TipoLocal::Deposito,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap();
    let local: LocalCriado = postcard::from_bytes(&saida).unwrap();

    d.executar_comando(
        "estoque.registrar_entrada.v1",
        &carga(&mod_estoque::RegistrarEntrada {
            produto: produto.produto,
            local: local.local,
            quantidade: Quantidade::unidades(5),
            custo_unitario: Preco::reais(90),
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    // Abre a OS.
    let saida = d
        .executar_comando(
            "os.abrir_ordem_servico.v1",
            &carga(&AbrirOrdemServico {
                cliente: cliente.pessoa,
                equipamento: "Notebook Dell XPS 13".to_string(),
                defeito_relatado: "Não liga".to_string(),
                tecnico_responsavel: Id::novo(),
                garantia_dias: 90,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap();
    let os: OrdemServicoAberta = postcard::from_bytes(&saida).unwrap();
    assert_eq!(os.numero, 1);

    // Laudo.
    d.executar_comando(
        "os.registrar_laudo.v1",
        &carga(&RegistrarLaudo {
            ordem_servico: os.ordem_servico,
            descricao_problema: "Não liga".to_string(),
            diagnostico: Some("Bateria não seguraincom carga".to_string()),
            tecnico: Id::novo(),
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    // Orçamento: 1 peça + mão de obra.
    let item_peca: Id = postcard::from_bytes(
        &d.executar_comando(
            "os.montar_orcamento.v1",
            &carga(&MontarOrcamentoOs {
                ordem_servico: os.ordem_servico,
                item: ItemOrcamentoNovo::Peca {
                    produto: produto.produto,
                    quantidade: Quantidade::unidades(1),
                    preco_unitario: Preco::reais(160),
                },
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    d.executar_comando(
        "os.montar_orcamento.v1",
        &carga(&MontarOrcamentoOs {
            ordem_servico: os.ordem_servico,
            item: ItemOrcamentoNovo::MaoDeObra {
                descricao: "Diagnóstico + troca de bateria".to_string(),
                valor: Dinheiro::reais(80),
                tecnico: Id::novo(),
                horas: None,
            },
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    // Envia, aprova.
    d.executar_comando(
        "os.enviar_para_aprovacao.v1",
        &carga(&EnviarParaAprovacao {
            ordem_servico: os.ordem_servico,
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    d.executar_comando(
        "os.aprovar_orcamento.v1",
        &carga(&AprovarOrcamentoOs {
            ordem_servico: os.ordem_servico,
            identificacao_aprovador: "João Silva - CPF 529.982.247-25".to_string(),
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    // Executa: inicia, aplica a peça, conclui.
    d.executar_comando(
        "os.iniciar_execucao.v1",
        &carga(&IniciarExecucao {
            ordem_servico: os.ordem_servico,
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    let saida = d
        .executar_comando(
            "os.aplicar_peca.v1",
            &carga(&AplicarPeca {
                ordem_servico: os.ordem_servico,
                item_peca,
                local: local.local,
                lote: None,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap();
    let aplicada: PecaFoiAplicada = postcard::from_bytes(&saida).unwrap();
    assert_eq!(aplicada.custo_unitario, Preco::reais(90));

    d.executar_comando(
        "os.concluir_execucao.v1",
        &carga(&ConcluirExecucao {
            ordem_servico: os.ordem_servico,
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    // Fatura: confere o título a receber e o lançamento combinado.
    let saida = d
        .executar_comando(
            "os.faturar_ordem_servico.v1",
            &carga(&FaturarOrdemServico {
                ordem_servico: os.ordem_servico,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap();
    let faturada: OrdemServicoFaturada = postcard::from_bytes(&saida).unwrap();
    assert_eq!(faturada.valor_total, Dinheiro::reais(240)); // 160 (peça) + 80 (mão de obra)

    assert_eq!(conta_titulos(&arm, empresa), 1);
    // `RegistrarEntrada` do estoque nesta fatia não lança no razão (ver mod-estoque::lib);
    // só o faturamento da OS gera lançamento — o combinado de receita + custo de serviço.
    assert_eq!(conta_lancamentos(&arm, empresa), 1);
}

#[test]
fn concluir_execucao_com_peca_pendente_e_recusado() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes, &ModuloEstoque, &ModuloOs]).unwrap();
    let s = sessao_completa(empresa);
    let amb = ambiente(empresa);

    let cliente: PessoaCadastrada = postcard::from_bytes(
        &d.executar_comando(
            "clientes.criar_pessoa.v1",
            &carga(&CriarPessoa {
                tipo: TipoPessoa::Fisica,
                nome: "Maria Souza".to_string(),
                nome_fantasia: None,
                papel_inicial: Papel::Cliente,
                documento_tipo: Some(TipoDocumento::Cpf),
                documento_numero: Some("11144477735".to_string()),
                data_nascimento: None,
                endereco: None,
                contato: None,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    let os: OrdemServicoAberta = postcard::from_bytes(
        &d.executar_comando(
            "os.abrir_ordem_servico.v1",
            &carga(&AbrirOrdemServico {
                cliente: cliente.pessoa,
                equipamento: "Celular".to_string(),
                defeito_relatado: "Não liga".to_string(),
                tecnico_responsavel: Id::novo(),
                garantia_dias: 90,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    d.executar_comando(
        "os.montar_orcamento.v1",
        &carga(&MontarOrcamentoOs {
            ordem_servico: os.ordem_servico,
            item: ItemOrcamentoNovo::Peca {
                produto: Id::novo(),
                quantidade: Quantidade::unidades(1),
                preco_unitario: Preco::reais(50),
            },
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();
    d.executar_comando(
        "os.enviar_para_aprovacao.v1",
        &carga(&EnviarParaAprovacao {
            ordem_servico: os.ordem_servico,
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();
    d.executar_comando(
        "os.aprovar_orcamento.v1",
        &carga(&AprovarOrcamentoOs {
            ordem_servico: os.ordem_servico,
            identificacao_aprovador: "Maria Souza".to_string(),
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();
    d.executar_comando(
        "os.iniciar_execucao.v1",
        &carga(&IniciarExecucao {
            ordem_servico: os.ordem_servico,
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    let erro = d
        .executar_comando(
            "os.concluir_execucao.v1",
            &carga(&ConcluirExecucao {
                ordem_servico: os.ordem_servico,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::REGRA_VIOLADA);
}

#[test]
fn ordens_em_aberto_lista_e_detalhe_traz_laudo_e_itens() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes, &ModuloEstoque, &ModuloOs]).unwrap();
    let s = sessao_completa(empresa);
    let amb = ambiente(empresa);

    let saida = d
        .executar_comando(
            "clientes.criar_pessoa.v1",
            &carga(&CriarPessoa {
                tipo: TipoPessoa::Fisica,
                nome: "Maria Souza".to_string(),
                nome_fantasia: None,
                papel_inicial: Papel::Cliente,
                documento_tipo: Some(TipoDocumento::Cpf),
                documento_numero: Some("52998224725".to_string()),
                data_nascimento: None,
                endereco: None,
                contato: None,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap();
    let cliente: PessoaCadastrada = postcard::from_bytes(&saida).unwrap();

    let saida = d
        .executar_comando(
            "os.abrir_ordem_servico.v1",
            &carga(&AbrirOrdemServico {
                cliente: cliente.pessoa,
                equipamento: "Impressora HP".to_string(),
                defeito_relatado: "Não liga".to_string(),
                tecnico_responsavel: Id::novo(),
                garantia_dias: 90,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap();
    let os: OrdemServicoAberta = postcard::from_bytes(&saida).unwrap();

    d.executar_comando(
        "os.registrar_laudo.v1",
        &carga(&RegistrarLaudo {
            ordem_servico: os.ordem_servico,
            descricao_problema: "Não imprime".to_string(),
            diagnostico: None,
            tecnico: Id::novo(),
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    let saida = d
        .executar_consulta(
            "os.ordens_em_aberto.v1",
            &carga(&OrdensEmAberto),
            &s,
            &amb,
            arm.leitor(),
        )
        .unwrap();
    let abertas: Vec<mod_os::OrdemServico> = postcard::from_bytes(&saida).unwrap();
    assert_eq!(abertas.len(), 1);
    assert_eq!(abertas[0].id, os.ordem_servico);
    assert_eq!(abertas[0].equipamento, "Impressora HP");

    let saida = d
        .executar_consulta(
            "os.buscar_detalhe_ordem.v1",
            &carga(&BuscarDetalheOrdem {
                ordem_servico: os.ordem_servico,
            }),
            &s,
            &amb,
            arm.leitor(),
        )
        .unwrap();
    let detalhe: Option<DetalheOrdem> = postcard::from_bytes(&saida).unwrap();
    let detalhe = detalhe.unwrap();
    assert_eq!(detalhe.ordem.id, os.ordem_servico);
    assert_eq!(detalhe.laudo.unwrap().descricao_problema, "Não imprime");
    assert!(detalhe.itens_peca.is_empty());
    assert!(detalhe.itens_mao_de_obra.is_empty());
}

#[test]
fn cancelar_ordem_servico_antes_de_concluida_e_recusado_depois() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes, &ModuloEstoque, &ModuloOs]).unwrap();
    let s = sessao_completa(empresa);
    let amb = ambiente(empresa);

    let cliente: PessoaCadastrada = postcard::from_bytes(
        &d.executar_comando(
            "clientes.criar_pessoa.v1",
            &carga(&CriarPessoa {
                tipo: TipoPessoa::Fisica,
                nome: "Maria Souza".to_string(),
                nome_fantasia: None,
                papel_inicial: Papel::Cliente,
                documento_tipo: Some(TipoDocumento::Cpf),
                documento_numero: Some("52998224725".to_string()),
                data_nascimento: None,
                endereco: None,
                contato: None,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    let os: OrdemServicoAberta = postcard::from_bytes(
        &d.executar_comando(
            "os.abrir_ordem_servico.v1",
            &carga(&AbrirOrdemServico {
                cliente: cliente.pessoa,
                equipamento: "Celular".to_string(),
                defeito_relatado: "Não liga".to_string(),
                tecnico_responsavel: Id::novo(),
                garantia_dias: 90,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    // Cancela ainda em Aberta — cliente desistiu antes do laudo.
    d.executar_comando(
        "os.cancelar_ordem_servico.v1",
        &carga(&CancelarOrdemServico {
            ordem_servico: os.ordem_servico,
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    // Uma ordem cancelada não aceita mais laudo.
    let erro = d
        .executar_comando(
            "os.registrar_laudo.v1",
            &carga(&RegistrarLaudo {
                ordem_servico: os.ordem_servico,
                descricao_problema: "Não liga".to_string(),
                diagnostico: None,
                tecnico: Id::novo(),
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::ESTADO_INVALIDO);

    // E cancelar de novo também é recusado (já é terminal).
    let erro = d
        .executar_comando(
            "os.cancelar_ordem_servico.v1",
            &carga(&CancelarOrdemServico {
                ordem_servico: os.ordem_servico,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::ESTADO_INVALIDO);
}

/// Monta cliente + produto + local + saldo em estoque — o mesmo início de
/// `ciclo_completo_de_os_abre_orca_aprova_executa_e_fatura`, fatorado para os testes de
/// cancelamento/estorno e de rastreabilidade por lote.
/// Cria um produto + local com saldo de verdade, com nomes/códigos sufixados por `sufixo`
/// para poder chamar mais de uma vez na mesma empresa sem colidir em `UNIQUE`.
fn produto_local_com_saldo(
    d: &Despachante,
    arm: &Armazenamento,
    s: &Sessao,
    amb: &Ambiente,
    sufixo: &str,
    quantidade: Quantidade,
    custo_unitario: Preco,
) -> (Id, Id) {
    let grupo: GrupoProdutoCriado = postcard::from_bytes(
        &d.executar_comando(
            "estoque.criar_grupo_produto.v1",
            &carga(&CriarGrupoProduto {
                codigo: format!("PECAS-{sufixo}"),
                nome: "Peças".to_string(),
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
                sigla: format!("UN-{sufixo}"),
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
                nome: format!("Tela original {sufixo}"),
                ncm: "85076000".to_string(),
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
                nome: format!("Depósito {sufixo}"),
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
        &carga(&mod_estoque::RegistrarEntrada {
            produto: produto.produto,
            local: local.local,
            quantidade,
            custo_unitario,
        }),
        s,
        amb,
        arm.escritor(),
    )
    .unwrap();

    (produto.produto, local.local)
}

/// Como [`produto_local_com_saldo`], mas também cadastra um cliente — o começo comum dos
/// testes de OS que precisam só de um produto/local.
fn cliente_produto_local_com_saldo(
    d: &Despachante,
    arm: &Armazenamento,
    s: &Sessao,
    amb: &Ambiente,
    quantidade: Quantidade,
    custo_unitario: Preco,
) -> (Id, Id, Id) {
    let cliente: PessoaCadastrada = postcard::from_bytes(
        &d.executar_comando(
            "clientes.criar_pessoa.v1",
            &carga(&CriarPessoa {
                tipo: TipoPessoa::Fisica,
                nome: "Cliente Teste".to_string(),
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

    let (produto, local) =
        produto_local_com_saldo(d, arm, s, amb, "A", quantidade, custo_unitario);
    (cliente.pessoa, produto, local)
}

fn saldo_disponivel(arm: &Armazenamento, produto: Id) -> Quantidade {
    let saida = arm
        .leitor()
        .consultar(|c| {
            c.query_row(
                "SELECT COALESCE(SUM(quantidade_disponivel), 0) FROM estoque_saldo_local \
                 WHERE produto = ?1",
                [produto.em_bytes().as_slice()],
                |r| r.get::<_, i64>(0),
            )
            .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
        })
        .unwrap();
    Quantidade::interna(saida)
}

#[test]
fn cancelar_ordem_em_execucao_estorna_peca_aplicada_ao_estoque() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes, &ModuloEstoque, &ModuloOs]).unwrap();
    let s = sessao_completa(empresa);
    let amb = ambiente(empresa);

    let (cliente, produto, local) =
        cliente_produto_local_com_saldo(&d, &arm, &s, &amb, Quantidade::unidades(5), Preco::reais(90));
    assert_eq!(saldo_disponivel(&arm, produto), Quantidade::unidades(5));

    let os: OrdemServicoAberta = postcard::from_bytes(
        &d.executar_comando(
            "os.abrir_ordem_servico.v1",
            &carga(&AbrirOrdemServico {
                cliente,
                equipamento: "Notebook".to_string(),
                defeito_relatado: "Tela quebrada".to_string(),
                tecnico_responsavel: Id::novo(),
                garantia_dias: 90,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    let item_peca: Id = postcard::from_bytes(
        &d.executar_comando(
            "os.montar_orcamento.v1",
            &carga(&MontarOrcamentoOs {
                ordem_servico: os.ordem_servico,
                item: ItemOrcamentoNovo::Peca {
                    produto,
                    quantidade: Quantidade::unidades(1),
                    preco_unitario: Preco::reais(160),
                },
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    d.executar_comando(
        "os.enviar_para_aprovacao.v1",
        &carga(&EnviarParaAprovacao {
            ordem_servico: os.ordem_servico,
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();
    d.executar_comando(
        "os.aprovar_orcamento.v1",
        &carga(&AprovarOrcamentoOs {
            ordem_servico: os.ordem_servico,
            identificacao_aprovador: "Cliente Teste - CPF 529.982.247-25".to_string(),
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();
    d.executar_comando(
        "os.iniciar_execucao.v1",
        &carga(&IniciarExecucao {
            ordem_servico: os.ordem_servico,
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    d.executar_comando(
        "os.aplicar_peca.v1",
        &carga(&AplicarPeca {
            ordem_servico: os.ordem_servico,
            item_peca,
            local,
            lote: None,
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();
    // A peça saiu do estoque: 5 - 1 = 4.
    assert_eq!(saldo_disponivel(&arm, produto), Quantidade::unidades(4));

    // Cliente cancela em plena execução (ex.: desistiu do conserto) — a peça já aplicada
    // precisa voltar ao estoque (docs/modulos/os.md §11 regra 5).
    let cancelada: OrdemServicoCancelada = postcard::from_bytes(
        &d.executar_comando(
            "os.cancelar_ordem_servico.v1",
            &carga(&CancelarOrdemServico {
                ordem_servico: os.ordem_servico,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(cancelada.pecas_estornadas, 1);
    assert_eq!(cancelada.pecas_pendentes_de_estorno_manual, 0);

    // O estoque voltou ao que era.
    assert_eq!(saldo_disponivel(&arm, produto), Quantidade::unidades(5));

    let detalhe: DetalheOrdem = postcard::from_bytes::<Option<DetalheOrdem>>(
        &d.executar_consulta(
            "os.buscar_detalhe_ordem.v1",
            &carga(&BuscarDetalheOrdem {
                ordem_servico: os.ordem_servico,
            }),
            &s,
            &amb,
            arm.leitor(),
        )
        .unwrap(),
    )
    .unwrap()
    .unwrap();
    assert!(detalhe.itens_peca[0].aplicada);
    assert!(detalhe.itens_peca[0].estornada);

    // Cancelar de novo não estorna outra vez (já é terminal — o comando é recusado antes de
    // chegar a olhar peça nenhuma).
    let erro = d
        .executar_comando(
            "os.cancelar_ordem_servico.v1",
            &carga(&CancelarOrdemServico {
                ordem_servico: os.ordem_servico,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::ESTADO_INVALIDO);
    assert_eq!(saldo_disponivel(&arm, produto), Quantidade::unidades(5));
}

#[test]
fn aplicar_peca_com_lote_rastreia_ate_o_codigo_do_post_it() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes, &ModuloEstoque, &ModuloOs]).unwrap();
    let s = sessao_completa(empresa);
    let amb = ambiente(empresa);

    let (cliente, produto, local) =
        cliente_produto_local_com_saldo(&d, &arm, &s, &amb, Quantidade::unidades(1), Preco::reais(60));

    // Registra o aparelho usado e a peça retirada dele, com o código do post-it.
    let aparelho: AparelhoOrigemRegistrado = postcard::from_bytes(
        &d.executar_comando(
            "estoque.registrar_aparelho_origem.v1",
            &carga(&RegistrarAparelhoOrigem {
                descricao: "iPhone 11 - tela quebrada".to_string(),
                identificador: None,
                custo_aquisicao: Dinheiro::reais(250),
                adquirido_em: cardeal_kernel::Data::de_dias(20_000),
                fornecedor: None,
                observacoes: None,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    let entrada_lote: EntradaComLoteRegistrada = postcard::from_bytes(
        &d.executar_comando(
            "estoque.registrar_entrada_com_lote.v1",
            &carga(&RegistrarEntradaComLote {
                produto,
                local,
                quantidade: Quantidade::unidades(1),
                custo_unitario: Preco::reais(120),
                codigo_lote: "TELA-77".to_string(),
                origem: OrigemLote::AparelhoUsado,
                fornecedor: None,
                aparelho_origem: Some(aparelho.aparelho_origem),
                fabricacao: None,
                validade: None,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    let os: OrdemServicoAberta = postcard::from_bytes(
        &d.executar_comando(
            "os.abrir_ordem_servico.v1",
            &carga(&AbrirOrdemServico {
                cliente,
                equipamento: "iPhone 11".to_string(),
                defeito_relatado: "Tela trincada".to_string(),
                tecnico_responsavel: Id::novo(),
                garantia_dias: 90,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    let item_peca: Id = postcard::from_bytes(
        &d.executar_comando(
            "os.montar_orcamento.v1",
            &carga(&MontarOrcamentoOs {
                ordem_servico: os.ordem_servico,
                item: ItemOrcamentoNovo::Peca {
                    produto,
                    quantidade: Quantidade::unidades(1),
                    preco_unitario: Preco::reais(300),
                },
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    d.executar_comando(
        "os.enviar_para_aprovacao.v1",
        &carga(&EnviarParaAprovacao {
            ordem_servico: os.ordem_servico,
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();
    d.executar_comando(
        "os.aprovar_orcamento.v1",
        &carga(&AprovarOrcamentoOs {
            ordem_servico: os.ordem_servico,
            identificacao_aprovador: "Cliente Teste - CPF 529.982.247-25".to_string(),
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();
    d.executar_comando(
        "os.iniciar_execucao.v1",
        &carga(&IniciarExecucao {
            ordem_servico: os.ordem_servico,
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    // O técnico digitou o código do post-it: aplica exatamente aquela peça física.
    let aplicada: PecaFoiAplicada = postcard::from_bytes(
        &d.executar_comando(
            "os.aplicar_peca.v1",
            &carga(&AplicarPeca {
                ordem_servico: os.ordem_servico,
                item_peca,
                local,
                lote: Some(entrada_lote.lote),
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    // O custo cobrado da OS é o custo médio do produto (90 = média entre 60 e 120 nesta
    // fatia — a contabilidade continua por custo médio), não o custo específico do lote; o
    // custo específico da peça (120) só aparece na consulta pelo código.
    assert_eq!(aplicada.custo_unitario, Preco::reais(90));

    // Digitando o código do post-it, o técnico acha a origem, o custo da peça específica, o
    // aparelho de origem com o custo dele, e o histórico: entrou, e saiu nesta OS.
    let detalhe: DetalheLote = postcard::from_bytes::<Option<DetalheLote>>(
        &d.executar_consulta(
            "estoque.detalhe_do_lote_por_codigo.v1",
            &carga(&DetalheDoLotePorCodigo {
                codigo: "TELA-77".to_string(),
            }),
            &s,
            &amb,
            arm.leitor(),
        )
        .unwrap(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(detalhe.custo_unitario, Preco::reais(120));
    assert_eq!(detalhe.quantidade_atual, Quantidade::ZERO);
    assert_eq!(detalhe.aparelho_origem.unwrap().custo_aquisicao, Dinheiro::reais(250));
    assert_eq!(detalhe.movimentos.len(), 2); // entrada + a saída desta OS
    assert!(detalhe
        .movimentos
        .iter()
        .any(|m| m.origem_modulo == "os" && m.origem_id == Some(os.ordem_servico)));
}

#[test]
fn aplicar_peca_com_lote_de_outro_produto_e_recusado() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes, &ModuloEstoque, &ModuloOs]).unwrap();
    let s = sessao_completa(empresa);
    let amb = ambiente(empresa);

    let (cliente, produto_a, local) =
        cliente_produto_local_com_saldo(&d, &arm, &s, &amb, Quantidade::unidades(1), Preco::reais(50));
    // Um segundo produto, com seu próprio lote.
    let (produto_b, local_b) =
        produto_local_com_saldo(&d, &arm, &s, &amb, "B", Quantidade::unidades(1), Preco::reais(50));

    let lote_de_b: EntradaComLoteRegistrada = postcard::from_bytes(
        &d.executar_comando(
            "estoque.registrar_entrada_com_lote.v1",
            &carga(&RegistrarEntradaComLote {
                produto: produto_b,
                local: local_b,
                quantidade: Quantidade::unidades(1),
                custo_unitario: Preco::reais(70),
                codigo_lote: "OUTRO-PRODUTO".to_string(),
                origem: OrigemLote::Compra,
                fornecedor: None,
                aparelho_origem: None,
                fabricacao: None,
                validade: None,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    let os: OrdemServicoAberta = postcard::from_bytes(
        &d.executar_comando(
            "os.abrir_ordem_servico.v1",
            &carga(&AbrirOrdemServico {
                cliente,
                equipamento: "Notebook".to_string(),
                defeito_relatado: "Tela quebrada".to_string(),
                tecnico_responsavel: Id::novo(),
                garantia_dias: 90,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    let item_peca: Id = postcard::from_bytes(
        &d.executar_comando(
            "os.montar_orcamento.v1",
            &carga(&MontarOrcamentoOs {
                ordem_servico: os.ordem_servico,
                item: ItemOrcamentoNovo::Peca {
                    produto: produto_a,
                    quantidade: Quantidade::unidades(1),
                    preco_unitario: Preco::reais(200),
                },
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    d.executar_comando(
        "os.enviar_para_aprovacao.v1",
        &carga(&EnviarParaAprovacao {
            ordem_servico: os.ordem_servico,
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();
    d.executar_comando(
        "os.aprovar_orcamento.v1",
        &carga(&AprovarOrcamentoOs {
            ordem_servico: os.ordem_servico,
            identificacao_aprovador: "Cliente Teste - CPF 529.982.247-25".to_string(),
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();
    d.executar_comando(
        "os.iniciar_execucao.v1",
        &carga(&IniciarExecucao {
            ordem_servico: os.ordem_servico,
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    // O item orçado é do produto A, mas o lote informado é da peça do produto B —
    // `mod_estoque::registrar_saida_de_lote_comum` recusa.
    let erro = d
        .executar_comando(
            "os.aplicar_peca.v1",
            &carga(&AplicarPeca {
                ordem_servico: os.ordem_servico,
                item_peca,
                local,
                lote: Some(lote_de_b.lote),
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::REGRA_VIOLADA);
}

#[test]
fn os_de_garantia_com_peca_gratuita_fatura_sem_titulo_mas_com_lancamento_de_custo() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes, &ModuloEstoque, &ModuloOs]).unwrap();
    let s = sessao_completa(empresa);
    let amb = ambiente(empresa);

    let cliente: PessoaCadastrada = postcard::from_bytes(
        &d.executar_comando(
            "clientes.criar_pessoa.v1",
            &carga(&CriarPessoa {
                tipo: TipoPessoa::Fisica,
                nome: "Maria Souza".to_string(),
                nome_fantasia: None,
                papel_inicial: Papel::Cliente,
                documento_tipo: Some(TipoDocumento::Cpf),
                documento_numero: Some("11144477735".to_string()),
                data_nascimento: None,
                endereco: None,
                contato: None,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    let grupo: GrupoProdutoCriado = postcard::from_bytes(
        &d.executar_comando(
            "estoque.criar_grupo_produto.v1",
            &carga(&CriarGrupoProduto {
                codigo: "PECAS".to_string(),
                nome: "Peças".to_string(),
                pai: None,
            }),
            &s,
            &amb,
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
            &amb,
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
                nome: "Bateria original 6 células".to_string(),
                ncm: "85076000".to_string(),
                unidade_padrao: unidade.unidade,
                codigo_barras: None,
                detalhes_tecnicos: None,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    let local: LocalCriado = postcard::from_bytes(
        &d.executar_comando(
            "estoque.criar_local.v1",
            &carga(&CriarLocal {
                nome: "Depósito".to_string(),
                tipo: TipoLocal::Deposito,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    d.executar_comando(
        "estoque.registrar_entrada.v1",
        &carga(&mod_estoque::RegistrarEntrada {
            produto: produto.produto,
            local: local.local,
            quantidade: Quantidade::unidades(5),
            custo_unitario: Preco::reais(90),
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    let os: OrdemServicoAberta = postcard::from_bytes(
        &d.executar_comando(
            "os.abrir_ordem_servico.v1",
            &carga(&AbrirOrdemServico {
                cliente: cliente.pessoa,
                equipamento: "Notebook Dell XPS 13".to_string(),
                defeito_relatado: "Não liga".to_string(),
                tecnico_responsavel: Id::novo(),
                garantia_dias: 90,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    // Peça trocada sob garantia: preço cobrado do cliente é zero, mas o custo real (90,00)
    // é debitado do estoque normalmente.
    let item_peca: Id = postcard::from_bytes(
        &d.executar_comando(
            "os.montar_orcamento.v1",
            &carga(&MontarOrcamentoOs {
                ordem_servico: os.ordem_servico,
                item: ItemOrcamentoNovo::Peca {
                    produto: produto.produto,
                    quantidade: Quantidade::unidades(1),
                    preco_unitario: Preco::ZERO,
                },
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    d.executar_comando(
        "os.enviar_para_aprovacao.v1",
        &carga(&EnviarParaAprovacao {
            ordem_servico: os.ordem_servico,
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();
    d.executar_comando(
        "os.aprovar_orcamento.v1",
        &carga(&AprovarOrcamentoOs {
            ordem_servico: os.ordem_servico,
            identificacao_aprovador: "Maria Souza - garantia".to_string(),
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();
    d.executar_comando(
        "os.iniciar_execucao.v1",
        &carga(&IniciarExecucao {
            ordem_servico: os.ordem_servico,
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();
    d.executar_comando(
        "os.aplicar_peca.v1",
        &carga(&AplicarPeca {
            ordem_servico: os.ordem_servico,
            item_peca,
            local: local.local,
            lote: None,
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();
    d.executar_comando(
        "os.concluir_execucao.v1",
        &carga(&ConcluirExecucao {
            ordem_servico: os.ordem_servico,
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    let faturada: OrdemServicoFaturada = postcard::from_bytes(
        &d.executar_comando(
            "os.faturar_ordem_servico.v1",
            &carga(&FaturarOrdemServico {
                ordem_servico: os.ordem_servico,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    assert_eq!(faturada.valor_total, Dinheiro::ZERO);
    assert!(faturada.titulo.is_none()); // nada a cobrar do cliente
    assert!(faturada.lancamento.is_some()); // mas o custo da peça foi lançado
    assert_eq!(conta_titulos(&arm, empresa), 0);
    assert_eq!(conta_lancamentos(&arm, empresa), 1);
}

#[test]
fn remover_item_orcamento_corrige_erro_de_digitacao_e_corrigir_laudo_sobrescreve() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes, &ModuloEstoque, &ModuloOs]).unwrap();
    let s = sessao_completa(empresa);
    let amb = ambiente(empresa);

    let cliente: PessoaCadastrada = postcard::from_bytes(
        &d.executar_comando(
            "clientes.criar_pessoa.v1",
            &carga(&CriarPessoa {
                tipo: TipoPessoa::Fisica,
                nome: "Carlos Lima".to_string(),
                nome_fantasia: None,
                papel_inicial: Papel::Cliente,
                documento_tipo: Some(TipoDocumento::Cpf),
                documento_numero: Some("52998224725".to_string()),
                data_nascimento: None,
                endereco: None,
                contato: None,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    let os: OrdemServicoAberta = postcard::from_bytes(
        &d.executar_comando(
            "os.abrir_ordem_servico.v1",
            &carga(&AbrirOrdemServico {
                cliente: cliente.pessoa,
                equipamento: "Celular".to_string(),
                defeito_relatado: "Não liga".to_string(),
                tecnico_responsavel: Id::novo(),
                garantia_dias: 90,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    // Laudo digitado errado, depois corrigido — ainda `EmDiagnostico`.
    d.executar_comando(
        "os.registrar_laudo.v1",
        &carga(&RegistrarLaudo {
            ordem_servico: os.ordem_servico,
            descricao_problema: "Não liag".to_string(),
            diagnostico: None,
            tecnico: Id::novo(),
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();
    d.executar_comando(
        "os.registrar_laudo.v1",
        &carga(&RegistrarLaudo {
            ordem_servico: os.ordem_servico,
            descricao_problema: "Não liga".to_string(),
            diagnostico: Some("Bateria esgotada".to_string()),
            tecnico: Id::novo(),
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    // Mão de obra digitada com o valor errado.
    let item_errado: Id = postcard::from_bytes(
        &d.executar_comando(
            "os.montar_orcamento.v1",
            &carga(&MontarOrcamentoOs {
                ordem_servico: os.ordem_servico,
                item: ItemOrcamentoNovo::MaoDeObra {
                    descricao: "Troca de bateria".to_string(),
                    valor: Dinheiro::reais(999),
                    tecnico: Id::novo(),
                    horas: None,
                },
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    d.executar_comando(
        "os.remover_item_orcamento.v1",
        &carga(&RemoverItemOrcamento {
            ordem_servico: os.ordem_servico,
            item: item_errado,
            tipo: TipoItemOrcamento::MaoDeObra,
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    // O item certo, com o valor certo.
    d.executar_comando(
        "os.montar_orcamento.v1",
        &carga(&MontarOrcamentoOs {
            ordem_servico: os.ordem_servico,
            item: ItemOrcamentoNovo::MaoDeObra {
                descricao: "Troca de bateria".to_string(),
                valor: Dinheiro::reais(80),
                tecnico: Id::novo(),
                horas: None,
            },
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    let saida = d
        .executar_consulta(
            "os.buscar_detalhe_ordem.v1",
            &carga(&BuscarDetalheOrdem {
                ordem_servico: os.ordem_servico,
            }),
            &s,
            &amb,
            arm.leitor(),
        )
        .unwrap();
    let detalhe: DetalheOrdem = postcard::from_bytes::<Option<DetalheOrdem>>(&saida)
        .unwrap()
        .unwrap();
    assert_eq!(detalhe.ordem.valor_total, Dinheiro::reais(80));
    assert_eq!(detalhe.ordem.itens_orcamento, 1);
    assert_eq!(
        detalhe.laudo.as_ref().unwrap().descricao_problema,
        "Não liga"
    );
    assert_eq!(
        detalhe.laudo.as_ref().unwrap().diagnostico.as_deref(),
        Some("Bateria esgotada")
    );

    // Segue o fluxo normal a partir daqui, sem sobra do item removido.
    d.executar_comando(
        "os.enviar_para_aprovacao.v1",
        &carga(&EnviarParaAprovacao {
            ordem_servico: os.ordem_servico,
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();
}

#[test]
fn ordens_aguardando_aprovacao_e_historico_do_equipamento() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes, &ModuloEstoque, &ModuloOs]).unwrap();
    let s = sessao_completa(empresa);
    let amb = ambiente(empresa);

    let cliente: PessoaCadastrada = postcard::from_bytes(
        &d.executar_comando(
            "clientes.criar_pessoa.v1",
            &carga(&CriarPessoa {
                tipo: TipoPessoa::Fisica,
                nome: "Carlos Lima".to_string(),
                nome_fantasia: None,
                papel_inicial: Papel::Cliente,
                documento_tipo: Some(TipoDocumento::Cpf),
                documento_numero: Some("52998224725".to_string()),
                data_nascimento: None,
                endereco: None,
                contato: None,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    // Primeira OS deste cliente/equipamento, chega até aguardar aprovação.
    let os1: OrdemServicoAberta = postcard::from_bytes(
        &d.executar_comando(
            "os.abrir_ordem_servico.v1",
            &carga(&AbrirOrdemServico {
                cliente: cliente.pessoa,
                equipamento: "Notebook Dell XPS 13".to_string(),
                defeito_relatado: "Não liga".to_string(),
                tecnico_responsavel: Id::novo(),
                garantia_dias: 90,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    d.executar_comando(
        "os.montar_orcamento.v1",
        &carga(&MontarOrcamentoOs {
            ordem_servico: os1.ordem_servico,
            item: ItemOrcamentoNovo::MaoDeObra {
                descricao: "Diagnóstico".to_string(),
                valor: Dinheiro::reais(50),
                tecnico: Id::novo(),
                horas: None,
            },
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();
    d.executar_comando(
        "os.enviar_para_aprovacao.v1",
        &carga(&EnviarParaAprovacao {
            ordem_servico: os1.ordem_servico,
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    // Segunda OS do MESMO cliente, equipamento parecido (erro de digitação) — reincidência,
    // continua só `Aberta`.
    let os2: OrdemServicoAberta = postcard::from_bytes(
        &d.executar_comando(
            "os.abrir_ordem_servico.v1",
            &carga(&AbrirOrdemServico {
                cliente: cliente.pessoa,
                equipamento: "Notbook Dell XPS13".to_string(),
                defeito_relatado: "Não liga".to_string(),
                tecnico_responsavel: Id::novo(),
                garantia_dias: 90,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    // Uma terceira OS de outro equipamento não deve aparecer no histórico.
    d.executar_comando(
        "os.abrir_ordem_servico.v1",
        &carga(&AbrirOrdemServico {
            cliente: cliente.pessoa,
            equipamento: "Impressora HP".to_string(),
            defeito_relatado: "Não liga".to_string(),
            tecnico_responsavel: Id::novo(),
            garantia_dias: 90,
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    // `OrdensAguardandoAprovacao` só lista a primeira (a segunda ainda está `Aberta`).
    let saida = d
        .executar_consulta(
            "os.ordens_aguardando_aprovacao.v1",
            &carga(&OrdensAguardandoAprovacao),
            &s,
            &amb,
            arm.leitor(),
        )
        .unwrap();
    let fila: Vec<mod_os::OrdemServico> = postcard::from_bytes(&saida).unwrap();
    assert_eq!(fila.len(), 1);
    assert_eq!(fila[0].id, os1.ordem_servico);

    // `HistoricoDoEquipamento` a partir da OS2, comparando contra o texto dela, encontra a
    // OS1 (equipamento parecido) mas não a da impressora.
    let saida = d
        .executar_consulta(
            "os.historico_do_equipamento.v1",
            &carga(&HistoricoDoEquipamento {
                cliente: cliente.pessoa,
                equipamento: "Notbook Dell XPS13".to_string(),
                excluir: Some(os2.ordem_servico),
            }),
            &s,
            &amb,
            arm.leitor(),
        )
        .unwrap();
    let historico: Vec<mod_os::OrdemServico> = postcard::from_bytes(&saida).unwrap();
    assert_eq!(historico.len(), 1);
    assert_eq!(historico[0].id, os1.ordem_servico);
}

#[test]
fn abrir_os_so_com_cliente_e_defeito_relatado_funciona_sem_equipamento() {
    // Pedido explícito do usuário: abrir OS só com nome do cliente + defeito relatado tem
    // que funcionar — equipamento fica vazio, completável depois.
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes, &ModuloEstoque, &ModuloOs]).unwrap();
    let s = sessao_completa(empresa);
    let amb = ambiente(empresa);

    let cliente: PessoaCadastrada = postcard::from_bytes(
        &d.executar_comando(
            "clientes.criar_pessoa.v1",
            &carga(&CriarPessoa {
                tipo: TipoPessoa::Fisica,
                nome: "Ana Paula".to_string(),
                nome_fantasia: None,
                papel_inicial: Papel::Cliente,
                documento_tipo: None,
                documento_numero: None,
                data_nascimento: None,
                endereco: None,
                contato: None,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    let os: OrdemServicoAberta = postcard::from_bytes(
        &d.executar_comando(
            "os.abrir_ordem_servico.v1",
            &carga(&AbrirOrdemServico {
                cliente: cliente.pessoa,
                equipamento: String::new(),
                defeito_relatado: "Não carrega a bateria".to_string(),
                tecnico_responsavel: Id::novo(),
                garantia_dias: 90,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    let saida = d
        .executar_consulta(
            "os.buscar_detalhe_ordem.v1",
            &carga(&BuscarDetalheOrdem {
                ordem_servico: os.ordem_servico,
            }),
            &s,
            &amb,
            arm.leitor(),
        )
        .unwrap();
    let detalhe: DetalheOrdem = postcard::from_bytes::<Option<DetalheOrdem>>(&saida)
        .unwrap()
        .unwrap();
    assert_eq!(detalhe.ordem.equipamento, "");
    assert_eq!(detalhe.ordem.defeito_relatado, "Não carrega a bateria");
}

#[test]
fn abrir_os_sem_defeito_relatado_e_recusado() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes, &ModuloEstoque, &ModuloOs]).unwrap();
    let s = sessao_completa(empresa);
    let amb = ambiente(empresa);

    let cliente: PessoaCadastrada = postcard::from_bytes(
        &d.executar_comando(
            "clientes.criar_pessoa.v1",
            &carga(&CriarPessoa {
                tipo: TipoPessoa::Fisica,
                nome: "Ana Paula".to_string(),
                nome_fantasia: None,
                papel_inicial: Papel::Cliente,
                documento_tipo: None,
                documento_numero: None,
                data_nascimento: None,
                endereco: None,
                contato: None,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    let erro = d
        .executar_comando(
            "os.abrir_ordem_servico.v1",
            &carga(&AbrirOrdemServico {
                cliente: cliente.pessoa,
                equipamento: "Celular".to_string(),
                defeito_relatado: "   ".to_string(),
                tecnico_responsavel: Id::novo(),
                garantia_dias: 90,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::ENTRADA_INVALIDA);
}

#[test]
fn editar_dados_da_ordem_completa_equipamento_e_complementa_defeito() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes, &ModuloEstoque, &ModuloOs]).unwrap();
    let s = sessao_completa(empresa);
    let amb = ambiente(empresa);

    let cliente: PessoaCadastrada = postcard::from_bytes(
        &d.executar_comando(
            "clientes.criar_pessoa.v1",
            &carga(&CriarPessoa {
                tipo: TipoPessoa::Fisica,
                nome: "Carlos Lima".to_string(),
                nome_fantasia: None,
                papel_inicial: Papel::Cliente,
                documento_tipo: None,
                documento_numero: None,
                data_nascimento: None,
                endereco: None,
                contato: None,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    let os: OrdemServicoAberta = postcard::from_bytes(
        &d.executar_comando(
            "os.abrir_ordem_servico.v1",
            &carga(&AbrirOrdemServico {
                cliente: cliente.pessoa,
                equipamento: String::new(),
                defeito_relatado: "Não liga".to_string(),
                tecnico_responsavel: Id::novo(),
                garantia_dias: 90,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    // Completa o equipamento e complementa o defeito relatado.
    d.executar_comando(
        "os.editar_dados_da_ordem.v1",
        &carga(&EditarDadosDaOrdem {
            ordem_servico: os.ordem_servico,
            equipamento: Some("Notebook Dell XPS 13".to_string()),
            complemento_defeito_relatado: Some("Também não carrega a bateria".to_string()),
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    let saida = d
        .executar_consulta(
            "os.buscar_detalhe_ordem.v1",
            &carga(&BuscarDetalheOrdem {
                ordem_servico: os.ordem_servico,
            }),
            &s,
            &amb,
            arm.leitor(),
        )
        .unwrap();
    let detalhe: DetalheOrdem = postcard::from_bytes::<Option<DetalheOrdem>>(&saida)
        .unwrap()
        .unwrap();
    assert_eq!(detalhe.ordem.equipamento, "Notebook Dell XPS 13");
    assert_eq!(
        detalhe.ordem.defeito_relatado,
        "Não liga | Também não carrega a bateria"
    );

    // Sem nenhum dos dois campos, o comando recusa (nada para atualizar).
    let erro = d
        .executar_comando(
            "os.editar_dados_da_ordem.v1",
            &carga(&EditarDadosDaOrdem {
                ordem_servico: os.ordem_servico,
                equipamento: None,
                complemento_defeito_relatado: None,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::ENTRADA_INVALIDA);

    // Cancela a OS e confirma que editar dados depois disso é recusado (finalizada).
    d.executar_comando(
        "os.cancelar_ordem_servico.v1",
        &carga(&CancelarOrdemServico {
            ordem_servico: os.ordem_servico,
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();
    let erro = d
        .executar_comando(
            "os.editar_dados_da_ordem.v1",
            &carga(&EditarDadosDaOrdem {
                ordem_servico: os.ordem_servico,
                equipamento: Some("Notebook".to_string()),
                complemento_defeito_relatado: None,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::ESTADO_INVALIDO);
}

#[test]
fn pecas_aguardando_estoque_cruza_orcamento_pendente_com_saldo_real() {
    // Auditoria de integração (2026-09-11): uma peça orçada mas sem saldo suficiente no
    // estoque hoje só era descoberta abrindo a OS e o produto em telas separadas.
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes, &ModuloEstoque, &ModuloOs]).unwrap();
    let s = sessao_completa(empresa);
    let amb = ambiente(empresa);

    let cliente: PessoaCadastrada = postcard::from_bytes(
        &d.executar_comando(
            "clientes.criar_pessoa.v1",
            &carga(&CriarPessoa {
                tipo: TipoPessoa::Fisica,
                nome: "Beatriz Nunes".to_string(),
                nome_fantasia: None,
                papel_inicial: Papel::Cliente,
                documento_tipo: None,
                documento_numero: None,
                data_nascimento: None,
                endereco: None,
                contato: None,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    let grupo: GrupoProdutoCriado = postcard::from_bytes(
        &d.executar_comando(
            "estoque.criar_grupo_produto.v1",
            &carga(&CriarGrupoProduto {
                codigo: "TELA".to_string(),
                nome: "Telas".to_string(),
                pai: None,
            }),
            &s,
            &amb,
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
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    // Produto cadastrado, mas SEM nenhuma entrada de estoque — saldo disponível é zero.
    let produto: ProdutoCriado = postcard::from_bytes(
        &d.executar_comando(
            "estoque.criar_produto.v1",
            &carga(&CriarProduto {
                grupo_produto: grupo.grupo_produto,
                nome: "Tela iPhone 11".to_string(),
                ncm: "85177000".to_string(),
                unidade_padrao: unidade.unidade,
                codigo_barras: None,
                detalhes_tecnicos: None,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    let os: OrdemServicoAberta = postcard::from_bytes(
        &d.executar_comando(
            "os.abrir_ordem_servico.v1",
            &carga(&AbrirOrdemServico {
                cliente: cliente.pessoa,
                equipamento: "iPhone 11".to_string(),
                defeito_relatado: "Tela quebrada".to_string(),
                tecnico_responsavel: Id::novo(),
                garantia_dias: 90,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    d.executar_comando(
        "os.montar_orcamento.v1",
        &carga(&MontarOrcamentoOs {
            ordem_servico: os.ordem_servico,
            item: ItemOrcamentoNovo::Peca {
                produto: produto.produto,
                quantidade: Quantidade::unidades(1),
                preco_unitario: Preco::reais(350),
            },
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    let saida = d
        .executar_consulta(
            "os.pecas_aguardando_estoque.v1",
            &carga(&PecasAguardandoEstoque),
            &s,
            &amb,
            arm.leitor(),
        )
        .unwrap();
    let pendentes: Vec<ItemAguardandoEstoque> = postcard::from_bytes(&saida).unwrap();
    assert_eq!(pendentes.len(), 1);
    assert_eq!(pendentes[0].ordem_servico, os.ordem_servico);
    assert_eq!(pendentes[0].produto, produto.produto);
    assert_eq!(pendentes[0].quantidade_necessaria, Quantidade::unidades(1));
    assert_eq!(pendentes[0].saldo_disponivel, Quantidade::ZERO);

    // Repõe o estoque com quantidade suficiente — a peça sai do radar.
    let local: LocalCriado = postcard::from_bytes(
        &d.executar_comando(
            "estoque.criar_local.v1",
            &carga(&CriarLocal {
                nome: "Depósito".to_string(),
                tipo: TipoLocal::Deposito,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    d.executar_comando(
        "estoque.registrar_entrada.v1",
        &carga(&mod_estoque::RegistrarEntrada {
            produto: produto.produto,
            local: local.local,
            quantidade: Quantidade::unidades(5),
            custo_unitario: Preco::reais(200),
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    let saida = d
        .executar_consulta(
            "os.pecas_aguardando_estoque.v1",
            &carga(&PecasAguardandoEstoque),
            &s,
            &amb,
            arm.leitor(),
        )
        .unwrap();
    let pendentes: Vec<ItemAguardandoEstoque> = postcard::from_bytes(&saida).unwrap();
    assert!(pendentes.is_empty());
}

#[test]
fn abrir_os_com_cliente_inexistente_e_recusado() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes, &ModuloEstoque, &ModuloOs]).unwrap();
    let s = sessao_completa(empresa);
    let amb = ambiente(empresa);

    let erro = d
        .executar_comando(
            "os.abrir_ordem_servico.v1",
            &carga(&AbrirOrdemServico {
                cliente: Id::novo(),
                equipamento: "Celular".to_string(),
                defeito_relatado: "Não liga".to_string(),
                tecnico_responsavel: Id::novo(),
                garantia_dias: 90,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::NAO_ENCONTRADO);
}

#[test]
fn montar_orcamento_aceita_peca_nova_com_os_em_execucao() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes, &ModuloEstoque, &ModuloOs]).unwrap();
    let s = sessao_completa(empresa);
    let amb = ambiente(empresa);

    let cliente: PessoaCadastrada = postcard::from_bytes(
        &d.executar_comando(
            "clientes.criar_pessoa.v1",
            &carga(&CriarPessoa {
                tipo: TipoPessoa::Fisica,
                nome: "Carlos Souza".to_string(),
                nome_fantasia: None,
                papel_inicial: Papel::Cliente,
                documento_tipo: None,
                documento_numero: None,
                data_nascimento: None,
                endereco: None,
                contato: None,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    let os: OrdemServicoAberta = postcard::from_bytes(
        &d.executar_comando(
            "os.abrir_ordem_servico.v1",
            &carga(&AbrirOrdemServico {
                cliente: cliente.pessoa,
                equipamento: "Notebook".to_string(),
                defeito_relatado: "Não carrega".to_string(),
                tecnico_responsavel: Id::novo(),
                garantia_dias: 90,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    d.executar_comando(
        "os.montar_orcamento.v1",
        &carga(&MontarOrcamentoOs {
            ordem_servico: os.ordem_servico,
            item: ItemOrcamentoNovo::MaoDeObra {
                descricao: "Diagnóstico".to_string(),
                valor: Dinheiro::reais(50),
                tecnico: Id::novo(),
                horas: None,
            },
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    d.executar_comando(
        "os.enviar_para_aprovacao.v1",
        &carga(&EnviarParaAprovacao {
            ordem_servico: os.ordem_servico,
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    d.executar_comando(
        "os.aprovar_orcamento.v1",
        &carga(&AprovarOrcamentoOs {
            ordem_servico: os.ordem_servico,
            identificacao_aprovador: "Carlos Souza - CPF 529.982.247-25".to_string(),
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    d.executar_comando(
        "os.iniciar_execucao.v1",
        &carga(&IniciarExecucao {
            ordem_servico: os.ordem_servico,
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    // Uma peça que só se revelou necessária depois de abrir o equipamento — orçada com a OS
    // já `EmExecucao`, sem precisar reabrir o diagnóstico.
    let grupo: GrupoProdutoCriado = postcard::from_bytes(
        &d.executar_comando(
            "estoque.criar_grupo_produto.v1",
            &carga(&CriarGrupoProduto {
                codigo: "PECAS".to_string(),
                nome: "Peças".to_string(),
                pai: None,
            }),
            &s,
            &amb,
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
            &amb,
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
                nome: "Carregador original".to_string(),
                ncm: "85044090".to_string(),
                unidade_padrao: unidade.unidade,
                codigo_barras: None,
                detalhes_tecnicos: None,
            }),
            &s,
            &amb,
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();

    d.executar_comando(
        "os.montar_orcamento.v1",
        &carga(&MontarOrcamentoOs {
            ordem_servico: os.ordem_servico,
            item: ItemOrcamentoNovo::Peca {
                produto: produto.produto,
                quantidade: Quantidade::unidades(1),
                preco_unitario: Preco::reais(120),
            },
        }),
        &s,
        &amb,
        arm.escritor(),
    )
    .unwrap();

    let saida = d
        .executar_consulta(
            "os.buscar_detalhe_ordem.v1",
            &carga(&BuscarDetalheOrdem {
                ordem_servico: os.ordem_servico,
            }),
            &s,
            &amb,
            arm.leitor(),
        )
        .unwrap();
    let detalhe: Option<DetalheOrdem> = postcard::from_bytes(&saida).unwrap();
    let detalhe = detalhe.unwrap();
    assert_eq!(detalhe.itens_peca.len(), 1);
    assert_eq!(detalhe.itens_peca[0].produto, produto.produto);
}
