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
    CriarGrupoProduto, CriarLocal, CriarProduto, CriarUnidade, GrupoProdutoCriado, LocalCriado,
    ModuloEstoque, ProdutoCriado, TipoLocal, UnidadeCriada,
};
use mod_os::{
    AbrirOrdemServico, AplicarPeca, AprovarOrcamentoOs, BuscarDetalheOrdem, CancelarOrdemServico,
    ConcluirExecucao, DetalheOrdem, EnviarParaAprovacao, FaturarOrdemServico, IniciarExecucao,
    ItemOrcamentoNovo, ModuloOs, MontarOrcamentoOs, OrdemServicoAberta, OrdemServicoFaturada,
    OrdensEmAberto, PecaFoiAplicada, RegistrarLaudo, RemoverItemOrcamento, TipoItemOrcamento,
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
        "os.ordem.criar",
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
                documento_tipo: TipoDocumento::Cpf,
                documento_numero: "52998224725".to_string(),
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
                documento_tipo: TipoDocumento::Cpf,
                documento_numero: "11144477735".to_string(),
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
                documento_tipo: TipoDocumento::Cpf,
                documento_numero: "52998224725".to_string(),
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
                documento_tipo: TipoDocumento::Cpf,
                documento_numero: "52998224725".to_string(),
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
                documento_tipo: TipoDocumento::Cpf,
                documento_numero: "11144477735".to_string(),
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
                documento_tipo: TipoDocumento::Cpf,
                documento_numero: "52998224725".to_string(),
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
