//! Os caminhos do balcão que precisam ser **uma transação só**: abrir OS cadastrando o
//! cliente na hora, aplicar todas as peças de uma vez, e a busca de OS feita no motor.

#![allow(clippy::result_large_err)] // `ErroArmazenamento` carrega detalhes de propósito

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel as PapelAuth, Sessao};
use cardeal_kernel::{CodigoErro, Id, Instante, Preco, Quantidade};
use cardeal_ledger::semear_plano_padrao;
use cardeal_modkit::{Ambiente, Despachante, Modulo, PedidoAtivacao, RegistroModulos};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use mod_clientes::{
    ContatoInicial, CriarPessoa, ModuloClientes, Papel, PessoaCadastrada, TipoContato, TipoPessoa,
};
use mod_estoque::{
    CriarGrupoProduto, CriarLocal, CriarProduto, CriarUnidade, GrupoProdutoCriado, LocalCriado,
    ModuloEstoque, ProdutoCriado, RegistrarEntrada, TipoLocal, UnidadeCriada,
};
use mod_os::{
    AbrirOrdemComClienteNovo, AbrirOrdemServico, AplicarPecas, AprovarOrcamentoOs,
    BuscarDetalheOrdem, BuscarOrdens, CancelarOrdemServico, DetalheOrdem, EnviarParaAprovacao,
    FiltroEstadoOs, IniciarExecucao, ItemOrcamentoNovo, ModuloOs, MontarOrcamentoOs,
    OrdemComClienteNovoAberta, OrdemServico, OrdemServicoAberta, OrdemServicoCancelada,
};
use serde::de::DeserializeOwned;
use tempfile::TempDir;

struct Balcao {
    _dir: TempDir,
    arm: Armazenamento,
    empresa: Id,
    d: Despachante,
    s: Sessao,
    amb: Ambiente,
}

impl Balcao {
    fn novo(permissoes: &[&str]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let arm =
            Armazenamento::abrir(ConfigArmazenamento::arquivo(dir.path().join("c.db"))).unwrap();
        arm.migrar(&[
            cardeal_ledger::migracoes::conjunto(),
            ModuloClientes.migracoes(),
            ModuloEstoque.migracoes(),
            mod_financeiro::ModuloFinanceiro.migracoes(),
            ModuloOs.migracoes(),
        ])
        .unwrap();
        let empresa = Id::novo();
        arm.escritor()
            .executar(
                ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo()),
                move |uow| {
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
                },
            )
            .unwrap();

        let mut rm = RegistroModulos::novo();
        for m in [
            &mod_clientes::MANIFESTO,
            &mod_estoque::MANIFESTO,
            &mod_financeiro::MANIFESTO,
            &mod_os::MANIFESTO,
        ] {
            rm.registrar(m).unwrap();
        }
        let efetivo = rm
            .resolver(&PedidoAtivacao::nova().com_modulo("os"))
            .unwrap();

        let mut papel = PapelAuth::novo(empresa, "Balcão");
        for p in permissoes {
            papel = papel.com_permissao(*p);
        }
        let s = Sessao::abrir(EmissaoSessao::padrao(
            Id::novo(),
            Id::novo(),
            Escopo::empresa_inteira(empresa),
            AutorizacoesEfetivas::consolidar([&papel]),
            Instante::EPOCA,
        ));
        Self {
            _dir: dir,
            arm,
            empresa,
            d: Despachante::construir(&[
                &ModuloClientes,
                &ModuloEstoque,
                &mod_financeiro::ModuloFinanceiro,
                &ModuloOs,
            ])
            .unwrap(),
            s,
            amb: Ambiente::novo(empresa, efetivo),
        }
    }

    fn cmd<T: DeserializeOwned>(
        &self,
        nome: &str,
        c: &impl serde::Serialize,
    ) -> Result<T, cardeal_kernel::Erro> {
        self.d
            .executar_comando(
                nome,
                &postcard::to_stdvec(c).unwrap(),
                &self.s,
                &self.amb,
                self.arm.escritor(),
            )
            .map(|b| postcard::from_bytes(&b).unwrap())
    }

    fn consulta<T: DeserializeOwned>(&self, nome: &str, q: &impl serde::Serialize) -> T {
        postcard::from_bytes(
            &self
                .d
                .executar_consulta(
                    nome,
                    &postcard::to_stdvec(q).unwrap(),
                    &self.s,
                    &self.amb,
                    self.arm.leitor(),
                )
                .unwrap(),
        )
        .unwrap()
    }

    fn conta_pessoas(&self) -> i64 {
        let empresa = self.empresa;
        self.arm
            .leitor()
            .consultar(move |c| {
                c.query_row(
                    "SELECT COUNT(*) FROM clientes_pessoa WHERE empresa = ?1",
                    [empresa.em_bytes().as_slice()],
                    |r| r.get(0),
                )
                .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            })
            .unwrap()
    }

    fn cliente(&self, nome: &str) -> Id {
        let c: PessoaCadastrada = self.cmd("clientes.criar_pessoa.v1", &pessoa(nome)).unwrap();
        c.pessoa
    }

    fn abrir(&self, cliente: Id, equipamento: &str, defeito: &str) -> OrdemServicoAberta {
        self.cmd(
            "os.abrir_ordem_servico.v1",
            &AbrirOrdemServico {
                cliente,
                equipamento: equipamento.to_owned(),
                defeito_relatado: defeito.to_owned(),
                tecnico_responsavel: Id::novo(),
                garantia_dias: 90,
                ficha: mod_os::FichaEntrada::default(),
            },
        )
        .unwrap()
    }

    fn detalhe(&self, os: Id) -> DetalheOrdem {
        let d: Option<DetalheOrdem> = self.consulta(
            "os.buscar_detalhe_ordem.v1",
            &BuscarDetalheOrdem { ordem_servico: os },
        );
        d.unwrap()
    }

    fn buscar(&self, filtro: FiltroEstadoOs, termo: &str, clientes: Vec<Id>) -> Vec<u64> {
        let v: Vec<OrdemServico> = self.consulta(
            "os.buscar_ordens.v1",
            &BuscarOrdens {
                filtro,
                termo: termo.to_owned(),
                clientes,
                limite: 50,
            },
        );
        v.into_iter().map(|o| o.numero).collect()
    }
}

fn pessoa(nome: &str) -> CriarPessoa {
    CriarPessoa {
        tipo: TipoPessoa::Fisica,
        nome: nome.to_owned(),
        nome_fantasia: None,
        papel_inicial: Papel::Cliente,
        documento_tipo: None,
        documento_numero: None,
        data_nascimento: None,
        endereco: None,
        contato: Some(ContatoInicial {
            tipo: TipoContato::Whatsapp,
            valor: "31999998888".to_owned(),
        }),
    }
}

const TODAS: &[&str] = &[
    "clientes.pessoa.criar",
    "estoque.produto.criar",
    "estoque.local.criar",
    "estoque.movimento.entrada",
    "os.ordem.criar",
    "os.ordem.ver",
    "os.ordem.cancelar",
    "os.orcamento.montar",
    "os.orcamento.enviar",
    "os.orcamento.aprovar",
    "os.execucao.iniciar",
    "os.peca.aplicar",
    "estoque.saldo.ver",
];

#[test]
fn abrir_com_cliente_novo_grava_os_dois_juntos() {
    let b = Balcao::novo(TODAS);
    let r: OrdemComClienteNovoAberta = b
        .cmd(
            "os.abrir_ordem_com_cliente_novo.v1",
            &AbrirOrdemComClienteNovo {
                cliente: pessoa("Maria Souza"),
                contatos_extras: vec![ContatoInicial {
                    tipo: TipoContato::Email,
                    valor: "maria@exemplo.com".to_owned(),
                }],
                equipamento: "iPhone 11".to_owned(),
                defeito_relatado: "Não carrega".to_owned(),
                tecnico_responsavel: Id::novo(),
                garantia_dias: 90,
                ficha: mod_os::FichaEntrada::default(),
            },
        )
        .unwrap();
    assert_eq!(b.conta_pessoas(), 1);
    assert_eq!(b.detalhe(r.ordem.ordem_servico).ordem.cliente, r.cliente);
}

#[test]
fn abrir_com_cliente_novo_que_falha_nao_deixa_cliente_orfao() {
    let b = Balcao::novo(TODAS);
    // Defeito relatado vazio: a abertura da OS é recusada depois do cliente já ter sido
    // gravado na transação — o rollback leva os dois.
    let erro = b
        .cmd::<OrdemComClienteNovoAberta>(
            "os.abrir_ordem_com_cliente_novo.v1",
            &AbrirOrdemComClienteNovo {
                cliente: pessoa("Maria Souza"),
                contatos_extras: Vec::new(),
                equipamento: "iPhone 11".to_owned(),
                defeito_relatado: "   ".to_owned(),
                tecnico_responsavel: Id::novo(),
                garantia_dias: 90,
                ficha: mod_os::FichaEntrada::default(),
            },
        )
        .unwrap_err();
    assert_ne!(erro.codigo, CodigoErro::FALHA_INTERNA);
    assert_eq!(b.conta_pessoas(), 0);
}

#[test]
fn abrir_com_cliente_novo_exige_permissao_de_cadastrar_cliente() {
    let sem_cadastro: Vec<&str> = TODAS
        .iter()
        .copied()
        .filter(|p| *p != "clientes.pessoa.criar")
        .collect();
    let b = Balcao::novo(&sem_cadastro);
    let erro = b
        .cmd::<OrdemComClienteNovoAberta>(
            "os.abrir_ordem_com_cliente_novo.v1",
            &AbrirOrdemComClienteNovo {
                cliente: pessoa("Maria Souza"),
                contatos_extras: Vec::new(),
                equipamento: "iPhone 11".to_owned(),
                defeito_relatado: "Não carrega".to_owned(),
                tecnico_responsavel: Id::novo(),
                garantia_dias: 90,
                ficha: mod_os::FichaEntrada::default(),
            },
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::SEM_PERMISSAO);
    assert_eq!(b.conta_pessoas(), 0);
}

/// Uma OS em execução com uma peça (1 un do produto) orçada; devolve (os, item, local).
fn os_em_execucao_com_peca(b: &Balcao, cliente: Id, produto: Id) -> (Id, Id) {
    let os = b.abrir(cliente, "Notebook", "Tela quebrada").ordem_servico;
    let item: Id = b
        .cmd(
            "os.montar_orcamento.v1",
            &MontarOrcamentoOs {
                ordem_servico: os,
                item: ItemOrcamentoNovo::Peca {
                    produto,
                    quantidade: Quantidade::unidades(1),
                    preco_unitario: Preco::reais(200),
                },
            },
        )
        .unwrap();
    b.cmd::<()>(
        "os.enviar_para_aprovacao.v1",
        &EnviarParaAprovacao { ordem_servico: os },
    )
    .unwrap();
    b.cmd::<()>(
        "os.aprovar_orcamento.v1",
        &AprovarOrcamentoOs {
            ordem_servico: os,
            identificacao_aprovador: "Cliente, CPF 529.982.247-25".to_owned(),
        },
    )
    .unwrap();
    b.cmd::<()>(
        "os.iniciar_execucao.v1",
        &IniciarExecucao { ordem_servico: os },
    )
    .unwrap();
    (os, item)
}

/// Produto "Tela" com 5 unidades a R$ 90 num local "Depósito"; devolve (produto, local).
fn produto_com_estoque(b: &Balcao) -> (Id, Id) {
    let grupo: GrupoProdutoCriado = b
        .cmd(
            "estoque.criar_grupo_produto.v1",
            &CriarGrupoProduto {
                codigo: "PECAS".to_owned(),
                nome: "Peças".to_owned(),
                pai: None,
            },
        )
        .unwrap();
    let unidade: UnidadeCriada = b
        .cmd(
            "estoque.criar_unidade.v1",
            &CriarUnidade {
                sigla: "UN".to_owned(),
                nome: "Unidade".to_owned(),
                fracionavel: false,
            },
        )
        .unwrap();
    let produto: ProdutoCriado = b
        .cmd(
            "estoque.criar_produto.v1",
            &CriarProduto {
                grupo_produto: grupo.grupo_produto,
                nome: "Tela".to_owned(),
                ncm: "85076000".to_owned(),
                unidade_padrao: unidade.unidade,
                codigo_barras: None,
                detalhes_tecnicos: None,
            },
        )
        .unwrap();
    let local: LocalCriado = b
        .cmd(
            "estoque.criar_local.v1",
            &CriarLocal {
                nome: "Depósito".to_owned(),
                tipo: TipoLocal::Deposito,
            },
        )
        .unwrap();
    let _: mod_estoque::EntradaRegistrada = b
        .cmd(
            "estoque.registrar_entrada.v1",
            &RegistrarEntrada {
                produto: produto.produto,
                local: local.local,
                quantidade: Quantidade::unidades(5),
                custo_unitario: Preco::reais(90),
            },
        )
        .unwrap();

    (produto.produto, local.local)
}

#[test]
fn aplicar_pecas_e_tudo_ou_nada() {
    let b = Balcao::novo(TODAS);
    let cliente = b.cliente("João");
    let (produto, local) = produto_com_estoque(&b);

    let (os1, item1) = os_em_execucao_com_peca(&b, cliente, produto);
    let (_os2, item_de_outra_os) = os_em_execucao_com_peca(&b, cliente, produto);

    // O segundo item é de outra OS: o lote inteiro é recusado e o primeiro não sai do estoque.
    b.cmd::<Vec<mod_os::PecaFoiAplicada>>(
        "os.aplicar_pecas.v1",
        &AplicarPecas {
            ordem_servico: os1,
            itens: vec![item1, item_de_outra_os],
            local,
        },
    )
    .unwrap_err();
    assert!(b.detalhe(os1).itens_peca.iter().all(|i| !i.aplicada));

    let feitas: Vec<mod_os::PecaFoiAplicada> = b
        .cmd(
            "os.aplicar_pecas.v1",
            &AplicarPecas {
                ordem_servico: os1,
                itens: vec![item1],
                local,
            },
        )
        .unwrap();
    assert_eq!(feitas.len(), 1);
    assert!(b.detalhe(os1).itens_peca.iter().all(|i| i.aplicada));
}

#[test]
fn buscar_ordens_filtra_no_motor_por_numero_texto_cliente_e_estado() {
    let b = Balcao::novo(TODAS);
    let joao = b.cliente("João");
    let maria = b.cliente("Maria");
    let n1 = b.abrir(joao, "Samsung A52", "Tela trincada").numero;
    let n2 = b.abrir(maria, "Furadeira Bosch", "Não liga").numero;
    let aberta3 = b.abrir(joao, "Notebook Acer", "Teclado com água");
    let _: OrdemServicoCancelada = b
        .cmd(
            "os.cancelar_ordem_servico.v1",
            &CancelarOrdemServico {
                ordem_servico: aberta3.ordem_servico,
            },
        )
        .unwrap();
    let n3 = aberta3.numero;

    // Ativas: a cancelada fica de fora; mais recentes primeiro.
    assert_eq!(b.buscar(FiltroEstadoOs::Ativas, "", vec![]), vec![n2, n1]);
    assert_eq!(
        b.buscar(FiltroEstadoOs::Todas, "", vec![]),
        vec![n3, n2, n1]
    );
    // Texto por palavras, sem acento/caixa, no aparelho e no defeito.
    assert_eq!(
        b.buscar(FiltroEstadoOs::Todas, "tela samsung", vec![]),
        vec![n1]
    );
    assert_eq!(b.buscar(FiltroEstadoOs::Todas, "AGUA", vec![]), vec![n3]);
    // Número (com ou sem #).
    assert_eq!(
        b.buscar(FiltroEstadoOs::Todas, &format!("#{n2}"), vec![]),
        vec![n2]
    );
    // Cliente resolvido pela tela.
    assert_eq!(
        b.buscar(FiltroEstadoOs::Todas, "maria", vec![maria]),
        vec![n2]
    );
    // Estado específico.
    assert_eq!(
        b.buscar(FiltroEstadoOs::Um(mod_os::EstadoOs::Cancelada), "", vec![]),
        vec![n3]
    );
}

#[test]
fn ordens_por_id_devolve_varias_numa_consulta_e_ignora_ids_desconhecidos() {
    let b = Balcao::novo(TODAS);
    let joao = b.cliente("João");
    let a = b.abrir(joao, "Notebook", "Não liga");
    let c = b.abrir(joao, "Celular", "Tela");
    let v: Vec<OrdemServico> = b.consulta(
        "os.ordens_por_id.v1",
        &mod_os::OrdensPorId {
            ordens: vec![a.ordem_servico, Id::novo(), c.ordem_servico],
        },
    );
    let mut numeros: Vec<u64> = v.iter().map(|o| o.numero).collect();
    numeros.sort_unstable();
    assert_eq!(numeros, vec![a.numero, c.numero]);
}

#[test]
fn ficha_de_entrada_e_gravada_na_abertura_e_corrigida_depois() {
    let b = Balcao::novo(&[TODAS, &["os.ordem.editar_dados"]].concat());
    let joao = b.cliente("João");
    let hoje = cardeal_kernel::Data::hoje(cardeal_kernel::Fuso::BRASILIA);
    let aberta: OrdemServicoAberta = b
        .cmd(
            "os.abrir_ordem_servico.v1",
            &AbrirOrdemServico {
                cliente: joao,
                equipamento: "iPhone 12".to_owned(),
                defeito_relatado: "Tela".to_owned(),
                tecnico_responsavel: Id::novo(),
                garantia_dias: 90,
                ficha: mod_os::FichaEntrada {
                    previsao_entrega: Some(hoje.mais_dias(3)),
                    numero_serie: "IMEI 3567".to_owned(),
                    acessorios: "Capinha azul".to_owned(),
                },
            },
        )
        .unwrap();
    let f = b.detalhe(aberta.ordem_servico).ordem.ficha;
    assert_eq!(f.previsao_entrega, Some(hoje.mais_dias(3)));
    assert_eq!(f.numero_serie, "IMEI 3567");

    // A peça atrasou: a previsão muda, o resto fica.
    b.cmd::<()>(
        "os.editar_dados_da_ordem.v1",
        &mod_os::EditarDadosDaOrdem {
            ordem_servico: aberta.ordem_servico,
            equipamento: None,
            complemento_defeito_relatado: None,
            ficha: Some(mod_os::FichaEntrada {
                previsao_entrega: Some(hoje.mais_dias(10)),
                ..f
            }),
        },
    )
    .unwrap();
    let f = b.detalhe(aberta.ordem_servico).ordem.ficha;
    assert_eq!(f.previsao_entrega, Some(hoje.mais_dias(10)));
    assert_eq!(f.acessorios, "Capinha azul");
}

#[test]
fn linha_do_tempo_registra_cada_mudanca_de_estado_uma_vez() {
    let b = Balcao::novo(&[TODAS, &["os.ordem.editar_dados"]].concat());
    let joao = b.cliente("João");
    let os = b.abrir(joao, "Notebook", "Não liga").ordem_servico;
    let _: Id = b
        .cmd(
            "os.montar_orcamento.v1",
            &MontarOrcamentoOs {
                ordem_servico: os,
                item: ItemOrcamentoNovo::MaoDeObra {
                    descricao: "Reparo".to_owned(),
                    valor: cardeal_kernel::Dinheiro::reais(100),
                    tecnico: Id::novo(),
                    horas: None,
                },
            },
        )
        .unwrap();
    // Editar dados não muda o estado: não entra na linha do tempo.
    b.cmd::<()>(
        "os.editar_dados_da_ordem.v1",
        &mod_os::EditarDadosDaOrdem {
            ordem_servico: os,
            equipamento: Some("Notebook Dell".to_owned()),
            complemento_defeito_relatado: None,
            ficha: None,
        },
    )
    .unwrap();
    b.cmd::<()>(
        "os.enviar_para_aprovacao.v1",
        &EnviarParaAprovacao { ordem_servico: os },
    )
    .unwrap();
    let _: OrdemServicoCancelada = b
        .cmd(
            "os.cancelar_ordem_servico.v1",
            &CancelarOrdemServico { ordem_servico: os },
        )
        .unwrap();

    use mod_os::EstadoOs as E;
    let passos: Vec<E> = b.detalhe(os).historico.iter().map(|p| p.estado).collect();
    assert_eq!(
        passos,
        vec![E::Aberta, E::AguardandoAprovacao, E::Cancelada]
    );
}

fn orcar_peca(b: &Balcao, os: Id, produto: Id) -> Id {
    b.cmd(
        "os.montar_orcamento.v1",
        &MontarOrcamentoOs {
            ordem_servico: os,
            item: ItemOrcamentoNovo::Peca {
                produto,
                quantidade: Quantidade::unidades(1),
                preco_unitario: Preco::reais(200),
            },
        },
    )
    .unwrap()
}

#[test]
fn faturar_direto_baixa_a_peca_com_o_custo_real() {
    let b = Balcao::novo(&[TODAS, &["os.faturar"]].concat());
    let cliente = b.cliente("João");
    let (produto, _local) = produto_com_estoque(&b);
    let os = b.abrir(cliente, "Notebook", "Tela").ordem_servico;
    orcar_peca(&b, os, produto);

    // Faturada ainda Aberta, sem passar por execução nem "Aplicar".
    let _: mod_os::OrdemServicoFaturada = b
        .cmd(
            "os.faturar_ordem_servico.v1",
            &mod_os::FaturarOrdemServico {
                ordem_servico: os,
                parcelas: 1,
                primeiro_vencimento: cardeal_kernel::Data::hoje(cardeal_kernel::Fuso::BRASILIA),
                intervalo_dias: 0,
                pago_no_ato: None,
            },
        )
        .unwrap();
    let d = b.detalhe(os);
    assert!(d.itens_peca[0].aplicada, "a peça saiu do estoque");
    assert_eq!(d.itens_peca[0].custo_unitario, Preco::reais(90));
    let saldo: Quantidade = b.consulta(
        "estoque.saldo_disponivel_do_produto.v1",
        &mod_estoque::SaldoDisponivelDoProduto { produto },
    );
    assert_eq!(saldo, Quantidade::unidades(4));
}

#[test]
fn concluir_aplica_sozinho_a_peca_que_faltava() {
    let b = Balcao::novo(&[TODAS, &["os.execucao.concluir"]].concat());
    let cliente = b.cliente("João");
    let (produto, _local) = produto_com_estoque(&b);
    let (os, _item) = os_em_execucao_com_peca(&b, cliente, produto);
    b.cmd::<()>(
        "os.concluir_execucao.v1",
        &mod_os::ConcluirExecucao { ordem_servico: os },
    )
    .unwrap();
    let d = b.detalhe(os);
    assert_eq!(d.ordem.estado, mod_os::EstadoOs::Concluida);
    assert!(d.itens_peca.iter().all(|i| i.aplicada));
}

#[test]
fn peca_sob_encomenda_do_orcamento_ate_a_chegada() {
    let b = Balcao::novo(
        &[
            TODAS,
            &[
                "financeiro.pagar.criar",
                "financeiro.pagar.baixar",
                "financeiro.pagar.ver",
            ],
        ]
        .concat(),
    );
    let cliente = b.cliente("Maria");
    // Um local precisa existir; a peça ainda não existe no catálogo.
    let _: mod_estoque::LocalCriado = b
        .cmd(
            "estoque.criar_local.v1",
            &mod_estoque::CriarLocal {
                nome: "Bancada".to_owned(),
                tipo: TipoLocal::Deposito,
            },
        )
        .unwrap();
    let os = b.abrir(cliente, "Samsung A52", "Tela").ordem_servico;

    // 1. Peça nova direto no orçamento, só pelo nome.
    let item: Id = b
        .cmd(
            "os.montar_orcamento.v1",
            &MontarOrcamentoOs {
                ordem_servico: os,
                item: ItemOrcamentoNovo::PecaNova {
                    nome: "Tela Samsung A52 original".to_owned(),
                    quantidade: Quantidade::unidades(1),
                    preco_unitario: Preco::reais(320),
                },
            },
        )
        .unwrap();

    // 2. Sem estoque: aparece na lista de compras; depois de encomendada, com o fornecedor.
    let lista = || -> Vec<mod_os::ItemAguardandoEstoque> {
        b.consulta(
            "os.pecas_aguardando_estoque.v1",
            &mod_os::PecasAguardandoEstoque,
        )
    };
    assert_eq!(lista().len(), 1);
    assert!(lista()[0].encomenda.is_none());
    b.cmd::<()>(
        "os.encomendar_peca.v1",
        &mod_os::EncomendarPeca {
            ordem_servico: os,
            item_peca: item,
            fornecedor: "Distribuidora Centro".to_owned(),
            custo_previsto: Some(Preco::reais(180)),
            previsao_chegada: None,
        },
    )
    .unwrap();
    let pendente = &lista()[0];
    assert_eq!(pendente.equipamento, "Samsung A52");
    assert_eq!(
        pendente.encomenda.as_ref().unwrap().fornecedor,
        "Distribuidora Centro"
    );

    // 3. Chegou, a prazo: aplicada com o custo real, some da lista, vira conta a pagar.
    let r: mod_os::ChegadaRegistrada = b
        .cmd(
            "os.registrar_chegada_da_peca.v1",
            &mod_os::RegistrarChegadaDaPeca {
                ordem_servico: os,
                item_peca: item,
                custo_unitario: Preco::reais(175),
                fornecedor: String::new(),
                pagamento: mod_os::PagamentoDaPeca::APrazo {
                    vencimento: cardeal_kernel::Data::hoje(cardeal_kernel::Fuso::BRASILIA)
                        .mais_dias(15),
                },
            },
        )
        .unwrap();
    assert_eq!(r.custo_total, cardeal_kernel::Dinheiro::reais(175));
    assert!(!r.pago);
    let d = b.detalhe(os);
    assert!(d.itens_peca[0].aplicada);
    assert_eq!(d.itens_peca[0].custo_unitario, Preco::reais(175));
    assert!(lista().is_empty());
    let a_pagar: Vec<mod_financeiro::ItemTituloEmAberto> = b.consulta(
        "financeiro.titulos_a_pagar_em_aberto.v1",
        &mod_financeiro::TitulosAPagarEmAberto,
    );
    assert_eq!(a_pagar.len(), 1);
    assert_eq!(
        a_pagar[0].descricao.as_deref(),
        Some("Peça da OS #1 — Distribuidora Centro")
    );

    // A peça cadastrada às pressas ficou com NCM pendente, sem travar nada.
    let saldo: Quantidade = b.consulta(
        "estoque.saldo_disponivel_do_produto.v1",
        &mod_estoque::SaldoDisponivelDoProduto {
            produto: d.itens_peca[0].produto,
        },
    );
    assert_eq!(saldo, Quantidade::ZERO, "entrou e saiu na mesma hora");
}

#[test]
fn chegada_paga_na_hora_nao_deixa_conta_em_aberto() {
    let b = Balcao::novo(
        &[
            TODAS,
            &[
                "financeiro.pagar.criar",
                "financeiro.pagar.baixar",
                "financeiro.pagar.ver",
            ],
        ]
        .concat(),
    );
    let cliente = b.cliente("João");
    let (produto, _) = produto_com_estoque(&b);
    let os = b.abrir(cliente, "Notebook", "Tela").ordem_servico;
    let item = orcar_peca(&b, os, produto);
    let r: mod_os::ChegadaRegistrada = b
        .cmd(
            "os.registrar_chegada_da_peca.v1",
            &mod_os::RegistrarChegadaDaPeca {
                ordem_servico: os,
                item_peca: item,
                custo_unitario: Preco::reais(100),
                fornecedor: "Loja da esquina".to_owned(),
                pagamento: mod_os::PagamentoDaPeca::PagoAgora {
                    meio_pagamento: mod_financeiro::MeioPagamento::Dinheiro,
                    conta_origem: None,
                },
            },
        )
        .unwrap();
    assert!(r.pago);
    let a_pagar: Vec<mod_financeiro::ItemTituloEmAberto> = b.consulta(
        "financeiro.titulos_a_pagar_em_aberto.v1",
        &mod_financeiro::TitulosAPagarEmAberto,
    );
    assert!(a_pagar.is_empty());
    // Sem permissão de financeiro, a chegada é recusada inteira.
    let sem = Balcao::novo(TODAS);
    let c2 = sem.cliente("Ana");
    let (p2, _) = produto_com_estoque(&sem);
    let os2 = sem.abrir(c2, "Tablet", "Bateria").ordem_servico;
    let item2 = orcar_peca(&sem, os2, p2);
    let erro = sem
        .cmd::<mod_os::ChegadaRegistrada>(
            "os.registrar_chegada_da_peca.v1",
            &mod_os::RegistrarChegadaDaPeca {
                ordem_servico: os2,
                item_peca: item2,
                custo_unitario: Preco::reais(100),
                fornecedor: String::new(),
                pagamento: mod_os::PagamentoDaPeca::PagoAgora {
                    meio_pagamento: mod_financeiro::MeioPagamento::Dinheiro,
                    conta_origem: None,
                },
            },
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::SEM_PERMISSAO);
    assert!(!sem.detalhe(os2).itens_peca[0].aplicada);
}

#[test]
fn margem_do_mes_soma_receita_e_custo_das_os_faturadas() {
    let b = Balcao::novo(&[TODAS, &["os.faturar"]].concat());
    let cliente = b.cliente("João");
    let (produto, _) = produto_com_estoque(&b);
    let hoje = cardeal_kernel::Data::hoje(cardeal_kernel::Fuso::BRASILIA);
    let faturar = |os: Id| {
        let _: mod_os::OrdemServicoFaturada = b
            .cmd(
                "os.faturar_ordem_servico.v1",
                &mod_os::FaturarOrdemServico {
                    ordem_servico: os,
                    parcelas: 1,
                    primeiro_vencimento: hoje,
                    intervalo_dias: 0,
                    pago_no_ato: None,
                },
            )
            .unwrap();
    };
    // Uma OS com peça (R$ 200 cobrado, R$ 90 de custo) faturada; outra aberta, fora da conta.
    let com_peca = b.abrir(cliente, "Notebook", "Tela").ordem_servico;
    orcar_peca(&b, com_peca, produto);
    faturar(com_peca);
    let aberta = b.abrir(cliente, "Tablet", "Bateria").ordem_servico;
    orcar_peca(&b, aberta, produto);

    let m: mod_os::MargemDasOrdens = b.consulta(
        "os.margem_das_ordens_no_periodo.v1",
        &mod_os::MargemDasOrdensNoPeriodo {
            periodo: cardeal_kernel::Periodo::novo(hoje.inicio_do_mes(), hoje.fim_do_mes()),
        },
    );
    assert_eq!(m.ordens, 1);
    assert_eq!(m.receita, cardeal_kernel::Dinheiro::reais(200));
    assert_eq!(m.custo_pecas, cardeal_kernel::Dinheiro::reais(90));
    assert_eq!(m.margem(), cardeal_kernel::Dinheiro::reais(110));

    // Fora do período (mês passado): nada.
    let m: mod_os::MargemDasOrdens = b.consulta(
        "os.margem_das_ordens_no_periodo.v1",
        &mod_os::MargemDasOrdensNoPeriodo {
            periodo: cardeal_kernel::Periodo::novo(
                hoje.inicio_do_mes().mais_meses(-1),
                hoje.inicio_do_mes().mais_dias(-1),
            ),
        },
    );
    assert_eq!(m.ordens, 0);

    // Por OS: a faturada tem o custo real; a aberta ainda tem a peça pendente, sem custo.
    let m: Option<mod_os::MargemDaOrdemServico> = b.consulta(
        "os.margem_da_ordem.v1",
        &mod_os::MargemDaOrdem {
            ordem_servico: com_peca,
        },
    );
    let m = m.expect("OS faturada");
    assert_eq!(m.margem(), cardeal_kernel::Dinheiro::reais(110));
    assert_eq!(m.percentual(), Some(55));
    assert_eq!(m.pecas_pendentes, 0);
    let m: Option<mod_os::MargemDaOrdemServico> = b.consulta(
        "os.margem_da_ordem.v1",
        &mod_os::MargemDaOrdem {
            ordem_servico: aberta,
        },
    );
    let m = m.expect("OS aberta");
    assert_eq!(
        (m.custo_pecas, m.pecas_pendentes),
        (cardeal_kernel::Dinheiro::ZERO, 1)
    );
}
