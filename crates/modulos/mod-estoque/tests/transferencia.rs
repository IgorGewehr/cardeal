//! Transferência entre locais pelo `Despachante` real contra SQLite: saída e entrada atômicas,
//! custo médio levado ao destino, recusas (saldo, mesmo local, inventário, lote) e histórico.

#![allow(clippy::result_large_err)] // `ErroArmazenamento` carrega detalhes de propósito

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel as PapelAuth, Sessao};
use cardeal_kernel::{CodigoErro, Dinheiro, Erro, Id, Instante, Preco, Quantidade};
use cardeal_ledger::semear_plano_padrao;
use cardeal_modkit::{Ambiente, Despachante, Modulo, PedidoAtivacao, RegistroModulos};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use mod_estoque::transferencia::ItemTransferencia;
use mod_estoque::{
    CriarGrupoProduto, CriarInventario, CriarLocal, CriarProduto, CriarUnidade,
    EntradaComLoteRegistrada, EntradaRegistrada, GrupoProdutoCriado, IniciarContagem,
    InventarioCriado, LocalCriado, ModuloEstoque, OrigemLote, ProdutoCriado, RegistrarEntrada,
    RegistrarEntradaComLote, ResumoTransferencia, SaldoNoLocal, SaldosDoProdutoPorLocal, TipoLocal,
    TransferenciaFeita, Transferencias, TransferirEntreLocais, UnidadeCriada, MANIFESTO,
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
    let mut pedido = PedidoAtivacao::nova().com_modulo("estoque");
    for sub in ["lote", "inventario", "multi_local", "transferencia"] {
        pedido = pedido.com_submodulo("estoque", sub);
    }
    Ambiente::novo(empresa, rm.resolver(&pedido).unwrap())
}

fn sessao(empresa: Id) -> Sessao {
    let mut papel = PapelAuth::novo(empresa, "Testador");
    for p in [
        "estoque.produto.criar",
        "estoque.produto.ver",
        "estoque.saldo.ver",
        "estoque.local.criar",
        "estoque.movimento.entrada",
        "estoque.movimento.entrada_com_lote",
        "estoque.transferencia.ver",
        "estoque.transferencia.criar",
        "estoque.inventario.criar",
        "estoque.inventario.contar",
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

struct Cenario {
    _dir: TempDir,
    arm: Armazenamento,
    empresa: Id,
    d: Despachante,
    s: Sessao,
    deposito: Id,
    loja: Id,
    grupo: Id,
    unidade: Id,
}

impl Cenario {
    fn novo() -> Self {
        let (dir, arm, empresa) = base();
        let mut c = Self {
            _dir: dir,
            arm,
            empresa,
            d: Despachante::construir(&[&ModuloEstoque]).unwrap(),
            s: sessao(empresa),
            deposito: Id::NULO,
            loja: Id::NULO,
            grupo: Id::NULO,
            unidade: Id::NULO,
        };
        let local = |c: &Self, nome: &str, tipo| {
            c.cmd::<LocalCriado>(
                "estoque.criar_local.v1",
                &CriarLocal {
                    nome: nome.into(),
                    tipo,
                },
            )
            .unwrap()
            .local
        };
        c.deposito = local(&c, "Depósito", TipoLocal::Deposito);
        c.loja = local(&c, "Loja", TipoLocal::Loja);
        c.grupo = c
            .cmd::<GrupoProdutoCriado>(
                "estoque.criar_grupo_produto.v1",
                &CriarGrupoProduto {
                    codigo: "MERC".into(),
                    nome: "Mercearia".into(),
                    pai: None,
                },
            )
            .unwrap()
            .grupo_produto;
        c.unidade = c
            .cmd::<UnidadeCriada>(
                "estoque.criar_unidade.v1",
                &CriarUnidade {
                    sigla: "UN".into(),
                    nome: "Unidade".into(),
                    fracionavel: false,
                },
            )
            .unwrap()
            .unidade;
        c
    }

    fn cmd<T: serde::de::DeserializeOwned>(
        &self,
        nome: &str,
        c: &impl serde::Serialize,
    ) -> Result<T, Erro> {
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

    fn consulta<T: serde::de::DeserializeOwned>(&self, nome: &str, c: &impl serde::Serialize) -> T {
        let b = self
            .d
            .executar_consulta(
                nome,
                &carga(c),
                &self.s,
                &ambiente(self.empresa),
                self.arm.leitor(),
            )
            .unwrap();
        postcard::from_bytes(&b).unwrap()
    }

    fn produto(&self, nome: &str) -> Id {
        self.cmd::<ProdutoCriado>(
            "estoque.criar_produto.v1",
            &CriarProduto {
                grupo_produto: self.grupo,
                nome: nome.into(),
                ncm: String::new(),
                unidade_padrao: self.unidade,
                codigo_barras: None,
                detalhes_tecnicos: None,
            },
        )
        .unwrap()
        .produto
    }

    fn entrada(&self, produto: Id, local: Id, qtd: i64, reais: i64) {
        self.cmd::<EntradaRegistrada>(
            "estoque.registrar_entrada.v1",
            &RegistrarEntrada {
                produto,
                local,
                quantidade: Quantidade::unidades(qtd),
                custo_unitario: Preco::reais(reais),
            },
        )
        .unwrap();
    }

    fn transferir(
        &self,
        origem: Id,
        destino: Id,
        itens: &[(Id, i64)],
    ) -> Result<TransferenciaFeita, Erro> {
        self.cmd(
            "estoque.transferir_entre_locais.v1",
            &TransferirEntreLocais {
                origem,
                destino,
                itens: itens
                    .iter()
                    .map(|&(produto, q)| ItemTransferencia {
                        produto,
                        quantidade: Quantidade::unidades(q),
                    })
                    .collect(),
                observacao: "reposição da loja".into(),
            },
        )
    }

    /// (disponível no depósito, disponível na loja).
    fn saldos(&self, produto: Id) -> (Quantidade, Quantidade) {
        let s: Vec<SaldoNoLocal> = self.consulta(
            "estoque.saldos_do_produto_por_local.v1",
            &SaldosDoProdutoPorLocal { produto },
        );
        let de = |l: Id| s.iter().find(|x| x.local == l).unwrap().disponivel;
        (de(self.deposito), de(self.loja))
    }

    fn custo_na_loja(&self, produto: Id) -> Preco {
        self.arm
            .leitor()
            .consultar(|conn| {
                conn.query_row(
                    "SELECT custo_medio FROM estoque_saldo_local WHERE produto = ?1 AND local = ?2",
                    [
                        produto.em_bytes().as_slice(),
                        self.loja.em_bytes().as_slice(),
                    ],
                    |r| r.get::<_, i64>(0),
                )
                .map(Preco::interna)
                .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            })
            .unwrap()
    }
}

fn q(n: i64) -> Quantidade {
    Quantidade::unidades(n)
}

#[test]
fn transfere_varios_itens_levando_o_custo_e_fica_no_historico() {
    let c = Cenario::novo();
    let arroz = c.produto("Arroz");
    let feijao = c.produto("Feijão");
    c.entrada(arroz, c.deposito, 10, 20);
    c.entrada(arroz, c.loja, 2, 10);
    c.entrada(feijao, c.deposito, 5, 8);

    let r = c
        .transferir(c.deposito, c.loja, &[(arroz, 2), (feijao, 5)])
        .unwrap();
    assert_eq!(r.itens, 2);
    assert_eq!(r.valor, Dinheiro::reais(80)); // 2 × 20 + 5 × 8

    assert_eq!(c.saldos(arroz), (q(8), q(4)));
    assert_eq!(c.saldos(feijao), (q(0), q(5)));
    // Loja tinha 2 a R$ 10 e recebeu 2 a R$ 20: custo médio R$ 15.
    assert_eq!(c.custo_na_loja(arroz), Preco::reais(15));
    assert_eq!(c.custo_na_loja(feijao), Preco::reais(8));

    let h: Vec<ResumoTransferencia> = c.consulta("estoque.transferencias.v1", &Transferencias);
    assert_eq!(h.len(), 1);
    assert_eq!(
        (h[0].origem.as_str(), h[0].destino.as_str()),
        ("Depósito", "Loja")
    );
    assert_eq!(h[0].observacao.as_deref(), Some("reposição da loja"));
    assert_eq!(h[0].valor(), Dinheiro::reais(80));

    // Mesma conta de estoque no CNPJ: nenhum lançamento no razão.
    let lancamentos: i64 = c
        .arm
        .leitor()
        .consultar(|conn| {
            conn.query_row("SELECT COUNT(*) FROM razao_lancamento", [], |r| r.get(0))
                .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
        })
        .unwrap();
    assert_eq!(lancamentos, 0);
}

#[test]
fn sem_saldo_na_origem_nada_e_transferido() {
    let c = Cenario::novo();
    let arroz = c.produto("Arroz");
    let feijao = c.produto("Feijão");
    c.entrada(arroz, c.deposito, 10, 20);
    c.entrada(feijao, c.deposito, 1, 8);
    let e = c
        .transferir(c.deposito, c.loja, &[(arroz, 3), (feijao, 2)])
        .unwrap_err();
    assert_eq!(e.codigo, CodigoErro::ESTOQUE_INSUFICIENTE);
    assert!(e.mensagem.contains("Feijão"), "{}", e.mensagem);
    // Tudo ou nada: nem o arroz (que tinha saldo) se mexeu.
    assert_eq!(c.saldos(arroz), (q(10), q(0)));
    let h: Vec<ResumoTransferencia> = c.consulta("estoque.transferencias.v1", &Transferencias);
    assert!(h.is_empty());
}

#[test]
fn mesmo_local_inventario_e_lote_sao_recusados() {
    let c = Cenario::novo();
    let arroz = c.produto("Arroz");
    c.entrada(arroz, c.deposito, 10, 20);
    assert_eq!(
        c.transferir(c.deposito, c.deposito, &[(arroz, 1)])
            .unwrap_err()
            .codigo,
        CodigoErro::REGRA_VIOLADA
    );

    // Peça com lote ativo na origem.
    let tela = c.produto("Tela A52");
    c.cmd::<EntradaComLoteRegistrada>(
        "estoque.registrar_entrada_com_lote.v1",
        &RegistrarEntradaComLote {
            produto: tela,
            local: c.deposito,
            quantidade: q(1),
            custo_unitario: Preco::reais(150),
            codigo_lote: "P-001".into(),
            origem: OrigemLote::Compra,
            fornecedor: None,
            aparelho_origem: None,
            fabricacao: None,
            validade: None,
        },
    )
    .unwrap();
    let e = c.transferir(c.deposito, c.loja, &[(tela, 1)]).unwrap_err();
    assert_eq!(e.codigo, CodigoErro::REGRA_VIOLADA);
    assert!(e.mensagem.contains("lote"), "{}", e.mensagem);

    // Destino em inventário: travado.
    let inv = c
        .cmd::<InventarioCriado>(
            "estoque.criar_inventario.v1",
            &CriarInventario {
                local: c.loja,
                grupo: None,
                descricao: String::new(),
            },
        )
        .unwrap();
    c.entrada(arroz, c.loja, 1, 20);
    c.cmd::<mod_estoque::ContagemIniciada>(
        "estoque.iniciar_contagem.v1",
        &IniciarContagem {
            inventario: inv.inventario,
        },
    )
    .unwrap();
    assert_eq!(
        c.transferir(c.deposito, c.loja, &[(arroz, 1)])
            .unwrap_err()
            .codigo,
        CodigoErro::RECURSO_TRAVADO
    );
}
