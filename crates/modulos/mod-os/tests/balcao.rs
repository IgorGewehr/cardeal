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
            d: Despachante::construir(&[&ModuloClientes, &ModuloEstoque, &ModuloOs]).unwrap(),
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

#[test]
fn aplicar_pecas_e_tudo_ou_nada() {
    let b = Balcao::novo(TODAS);
    let cliente = b.cliente("João");
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

    let (os1, item1) = os_em_execucao_com_peca(&b, cliente, produto.produto);
    let (_os2, item_de_outra_os) = os_em_execucao_com_peca(&b, cliente, produto.produto);

    // O segundo item é de outra OS: o lote inteiro é recusado e o primeiro não sai do estoque.
    b.cmd::<Vec<mod_os::PecaFoiAplicada>>(
        "os.aplicar_pecas.v1",
        &AplicarPecas {
            ordem_servico: os1,
            itens: vec![item1, item_de_outra_os],
            local: local.local,
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
                local: local.local,
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
