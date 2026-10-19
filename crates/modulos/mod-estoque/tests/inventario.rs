//! O inventário com contagem cega pelo `Despachante` real contra SQLite: foto e trava do
//! local, contagem cega, item achado, encerramento com ajustes no saldo e no razão, cancelamento
//! e recorte por grupo.

#![allow(clippy::result_large_err)] // `ErroArmazenamento` carrega detalhes de propósito

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel as PapelAuth, Sessao};
use cardeal_kernel::{CodigoErro, Dinheiro, Erro, Id, Instante, Preco, Quantidade};
use cardeal_ledger::semear_plano_padrao;
use cardeal_modkit::{Ambiente, Despachante, Modulo, PedidoAtivacao, RegistroModulos};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use mod_estoque::{
    AjustarSaldo, CancelarInventario, ContagemIniciada, ContagemRegistrada, CriarGrupoProduto,
    CriarInventario, CriarLocal, CriarProduto, CriarUnidade, EncerrarInventario, EstadoInventario,
    GrupoProdutoCriado, IniciarContagem, InventarioCriado, InventarioEncerradoResumo, Inventarios,
    ItemProdutoComSaldo, LocalCriado, ModuloEstoque, PosicaoDoInventario, PosicaoInventario,
    ProdutoCriado, ProdutosComSaldo, RegistrarContagem, RegistrarEntrada, RegistrarSaida,
    ResumoInventario, TipoLocal, UnidadeCriada, MANIFESTO,
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
        .resolver(
            &PedidoAtivacao::nova()
                .com_modulo("estoque")
                .com_submodulo("estoque", "inventario"),
        )
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

const TODAS: &[&str] = &[
    "estoque.produto.criar",
    "estoque.produto.ver",
    "estoque.local.criar",
    "estoque.movimento.entrada",
    "estoque.movimento.saida",
    "estoque.movimento.ajustar",
    "estoque.inventario.ver",
    "estoque.inventario.criar",
    "estoque.inventario.contar",
    "estoque.inventario.encerrar",
];

struct Cenario {
    _dir: TempDir,
    arm: Armazenamento,
    empresa: Id,
    d: Despachante,
    s: Sessao,
    local: Id,
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
            s: sessao(empresa, TODAS),
            local: Id::NULO,
            unidade: Id::NULO,
        };
        c.local = c
            .cmd::<LocalCriado>(
                "estoque.criar_local.v1",
                &CriarLocal {
                    nome: "Loja".into(),
                    tipo: TipoLocal::Loja,
                },
            )
            .unwrap()
            .local;
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

    fn grupo(&self, nome: &str) -> Id {
        self.cmd::<GrupoProdutoCriado>(
            "estoque.criar_grupo_produto.v1",
            &CriarGrupoProduto {
                codigo: nome.to_uppercase(),
                nome: nome.to_owned(),
                pai: None,
            },
        )
        .unwrap()
        .grupo_produto
    }

    /// Um produto no grupo, com `qtd` unidades de entrada no local ao custo `reais`.
    fn produto(&self, nome: &str, grupo: Id, qtd: i64, reais: i64) -> Id {
        let p = self
            .cmd::<ProdutoCriado>(
                "estoque.criar_produto.v1",
                &CriarProduto {
                    grupo_produto: grupo,
                    nome: nome.to_owned(),
                    ncm: String::new(),
                    unidade_padrao: self.unidade,
                    codigo_barras: None,
                    detalhes_tecnicos: None,
                },
            )
            .unwrap()
            .produto;
        if qtd > 0 {
            self.entrada(p, qtd, reais).unwrap();
        }
        p
    }

    fn entrada(&self, produto: Id, qtd: i64, reais: i64) -> Result<(), Erro> {
        self.cmd::<mod_estoque::EntradaRegistrada>(
            "estoque.registrar_entrada.v1",
            &RegistrarEntrada {
                produto,
                local: self.local,
                quantidade: Quantidade::unidades(qtd),
                custo_unitario: Preco::reais(reais),
            },
        )
        .map(|_| ())
    }

    fn criar(&self, grupo: Option<Id>) -> Result<Id, Erro> {
        self.cmd::<InventarioCriado>(
            "estoque.criar_inventario.v1",
            &CriarInventario {
                local: self.local,
                grupo,
                descricao: "Balanço".into(),
            },
        )
        .map(|r| r.inventario)
    }

    fn iniciar(&self, inventario: Id) -> Result<u32, Erro> {
        self.cmd::<ContagemIniciada>(
            "estoque.iniciar_contagem.v1",
            &IniciarContagem { inventario },
        )
        .map(|r| r.itens)
    }

    fn contar(&self, inventario: Id, produto: Id, qtd: i64) -> ContagemRegistrada {
        self.cmd(
            "estoque.registrar_contagem.v1",
            &RegistrarContagem {
                inventario,
                produto,
                quantidade: Quantidade::unidades(qtd),
            },
        )
        .unwrap()
    }

    fn encerrar(&self, inventario: Id) -> Result<InventarioEncerradoResumo, Erro> {
        self.cmd(
            "estoque.encerrar_inventario.v1",
            &EncerrarInventario { inventario },
        )
    }

    fn posicao(&self, inventario: Id) -> PosicaoInventario {
        self.consulta(
            "estoque.posicao_do_inventario.v1",
            &PosicaoDoInventario { inventario },
        )
    }

    fn disponivel(&self, produto: Id) -> Quantidade {
        let lista: Vec<ItemProdutoComSaldo> =
            self.consulta("estoque.produtos_com_saldo.v1", &ProdutosComSaldo);
        lista
            .into_iter()
            .find(|p| p.produto == produto)
            .map_or(Quantidade::ZERO, |p| p.disponivel)
    }
}

#[test]
fn contagem_cega_trava_o_local_e_o_encerramento_ajusta_saldo_e_razao() {
    let c = Cenario::novo();
    let g = c.grupo("Mercearia");
    let a = c.produto("Arroz", g, 10, 5);
    let b = c.produto("Feijão", g, 4, 10);
    let o = c.produto("Óleo", g, 2, 3);
    let achado = c.produto("Café", g, 0, 0); // nunca entrou neste local

    let inv = c.criar(None).unwrap();
    // Um local, um inventário aberto por vez.
    assert_eq!(c.criar(None).unwrap_err().codigo, CodigoErro::REGRA_VIOLADA);
    assert_eq!(c.iniciar(inv).unwrap(), 3, "os três com saldo no local");

    // Local travado: entrada, saída e ajuste avulso são recusados.
    assert_eq!(
        c.entrada(a, 1, 5).unwrap_err().codigo,
        CodigoErro::RECURSO_TRAVADO
    );
    let saida = c.cmd::<mod_estoque::SaidaRegistrada>(
        "estoque.registrar_saida.v1",
        &RegistrarSaida {
            produto: a,
            local: c.local,
            quantidade: Quantidade::unidades(1),
        },
    );
    assert_eq!(saida.unwrap_err().codigo, CodigoErro::RECURSO_TRAVADO);
    let ajuste = c.cmd::<mod_estoque::SaldoAjustado>(
        "estoque.ajustar_saldo.v1",
        &AjustarSaldo {
            produto: a,
            local: c.local,
            nova_quantidade: Quantidade::unidades(1),
            motivo: "contagem feita por fora".into(),
        },
    );
    assert_eq!(ajuste.unwrap_err().codigo, CodigoErro::RECURSO_TRAVADO);

    // Cega: a posição não revela o sistema enquanto a contagem está aberta.
    let p = c.posicao(inv);
    assert_eq!(p.resumo.estado, EstadoInventario::Congelado);
    assert!(p
        .itens
        .iter()
        .all(|i| i.sistema.is_none() && i.diferenca.is_none()));

    // Contar: falta 2 de arroz, feijão bate, sobra 1 de óleo, e o café achado entra.
    c.contar(inv, a, 7);
    c.contar(inv, a, 8); // contar de novo corrige
    c.contar(inv, b, 4);
    let r = c.contar(inv, achado, 1);
    assert!(r.incluido);
    assert_eq!((r.contados, r.itens), (3, 4));
    // Ainda falta o óleo: não encerra.
    assert_eq!(
        c.encerrar(inv).unwrap_err().codigo,
        CodigoErro::ESTADO_INVALIDO
    );
    c.contar(inv, o, 3);

    let r = c.encerrar(inv).unwrap();
    assert_eq!(r.ajustes, 3, "arroz, óleo e café");
    assert_eq!(r.faltas, Dinheiro::reais(10)); // 2 × R$ 5
    assert_eq!(r.sobras, Dinheiro::reais(3)); // 1 × R$ 3 (o café não tem custo)

    // Saldos ajustados e o local destravado.
    assert_eq!(c.disponivel(a), Quantidade::unidades(8));
    assert_eq!(c.disponivel(b), Quantidade::unidades(4));
    assert_eq!(c.disponivel(o), Quantidade::unidades(3));
    assert_eq!(c.disponivel(achado), Quantidade::unidades(1));
    c.entrada(a, 1, 5).unwrap();

    // Encerrado: agora a posição revela sistema, diferença e valor.
    let p = c.posicao(inv);
    assert_eq!(p.resumo.estado, EstadoInventario::Encerrado);
    let arroz = p.itens.iter().find(|i| i.produto == a).unwrap();
    assert_eq!(arroz.sistema, Some(Quantidade::unidades(10)));
    assert_eq!(arroz.diferenca, Some(Quantidade::unidades(-2)));
    assert_eq!(arroz.valor_ajuste, Some(Dinheiro::reais(10)));
    let feijao = p.itens.iter().find(|i| i.produto == b).unwrap();
    assert_eq!(feijao.diferenca, Some(Quantidade::ZERO));
    assert_eq!(feijao.valor_ajuste, None);

    // A falta e a sobra foram para o razão (origem "inventario").
    let lancamentos: i64 = c
        .arm
        .leitor()
        .consultar(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM razao_lancamento WHERE origem_tipo = 'inventario'",
                [],
                |r| r.get(0),
            )
            .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
        })
        .unwrap();
    assert_eq!(lancamentos, 2, "café sem custo não lança valor");

    let lista: Vec<ResumoInventario> = c.consulta("estoque.inventarios.v1", &Inventarios);
    assert_eq!(lista.len(), 1);
    assert_eq!((lista[0].itens, lista[0].contados), (4, 4));
}

#[test]
fn cancelar_libera_o_local_sem_ajustar_nada() {
    let c = Cenario::novo();
    let g = c.grupo("Mercearia");
    let a = c.produto("Arroz", g, 10, 5);
    let inv = c.criar(None).unwrap();
    c.iniciar(inv).unwrap();
    c.contar(inv, a, 3);
    c.cmd::<()>(
        "estoque.cancelar_inventario.v1",
        &CancelarInventario { inventario: inv },
    )
    .unwrap();
    assert_eq!(c.disponivel(a), Quantidade::unidades(10), "nada ajustado");
    c.entrada(a, 1, 5).unwrap();
    // Cancelado não volta, e o local aceita um inventário novo.
    assert_eq!(
        c.cmd::<()>(
            "estoque.cancelar_inventario.v1",
            &CancelarInventario { inventario: inv },
        )
        .unwrap_err()
        .codigo,
        CodigoErro::ESTADO_INVALIDO
    );
    assert_eq!(c.posicao(inv).resumo.estado, EstadoInventario::Cancelado);
    c.criar(None).unwrap();
}

#[test]
fn recorte_por_grupo_conta_so_a_secao_e_lista_vazia_e_recusada() {
    let c = Cenario::novo();
    let mercearia = c.grupo("Mercearia");
    let bebidas = c.grupo("Bebidas");
    let vazio = c.grupo("Vazio");
    let arroz = c.produto("Arroz", mercearia, 10, 5);
    c.produto("Refrigerante", bebidas, 6, 4);

    let inv = c.criar(Some(vazio)).unwrap();
    assert_eq!(
        c.iniciar(inv).unwrap_err().codigo,
        CodigoErro::REGRA_VIOLADA
    );
    c.cmd::<()>(
        "estoque.cancelar_inventario.v1",
        &CancelarInventario { inventario: inv },
    )
    .unwrap();

    let inv = c.criar(Some(mercearia)).unwrap();
    assert_eq!(c.iniciar(inv).unwrap(), 1);
    let p = c.posicao(inv);
    assert_eq!(p.itens.len(), 1);
    assert_eq!(p.itens[0].produto, arroz);
}

#[test]
fn sem_permissao_de_encerrar_nao_ajusta() {
    let mut c = Cenario::novo();
    let g = c.grupo("Mercearia");
    let a = c.produto("Arroz", g, 10, 5);
    let inv = c.criar(None).unwrap();
    c.iniciar(inv).unwrap();
    c.contar(inv, a, 9);
    c.s = sessao(
        c.empresa,
        &[
            "estoque.inventario.ver",
            "estoque.inventario.contar",
            "estoque.produto.ver",
        ],
    );
    assert_eq!(
        c.encerrar(inv).unwrap_err().codigo,
        CodigoErro::SEM_PERMISSAO
    );
}
