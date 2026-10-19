//! Validade de ponta a ponta pelo `Despachante` real contra SQLite: entrada com lote e
//! validade (código gerado quando em branco), os lotes vencidos e a vencer, e a baixa de perda
//! (lote esgotado, lançamento em perdas com o motivo, recusas).

#![allow(clippy::result_large_err)] // `ErroArmazenamento` carrega detalhes de propósito

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel as PapelAuth, Sessao};
use cardeal_kernel::{CodigoErro, Data, Dinheiro, Erro, Fuso, Id, Instante, Preco, Quantidade};
use cardeal_ledger::semear_plano_padrao;
use cardeal_modkit::{Ambiente, Despachante, Modulo, PedidoAtivacao, RegistroModulos};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use mod_estoque::{
    CriarGrupoProduto, CriarLocal, CriarProduto, CriarUnidade, EntradaComLoteRegistrada,
    EntradaRegistrada, GrupoProdutoCriado, LocalCriado, LoteComValidade, LotesComValidade,
    ModuloEstoque, MotivoPerda, OrigemLote, PerdaRegistrada, ProdutoCriado, RegistrarEntrada,
    RegistrarEntradaComLote, RegistrarPerda, TipoLocal, UnidadeCriada, MANIFESTO,
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
    let pedido = PedidoAtivacao::nova()
        .com_modulo("estoque")
        .com_submodulo("estoque", "lote")
        .com_submodulo("estoque", "validade");
    Ambiente::novo(empresa, rm.resolver(&pedido).unwrap())
}

fn sessao(empresa: Id) -> Sessao {
    let mut papel = PapelAuth::novo(empresa, "Testador");
    for p in [
        "estoque.produto.criar",
        "estoque.produto.ver",
        "estoque.local.criar",
        "estoque.movimento.entrada",
        "estoque.movimento.entrada_com_lote",
        "estoque.movimento.perda",
        "estoque.lote.ver",
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

struct Cenario {
    _dir: TempDir,
    arm: Armazenamento,
    empresa: Id,
    d: Despachante,
    s: Sessao,
    local: Id,
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
            local: Id::NULO,
            grupo: Id::NULO,
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
        c.grupo = c
            .cmd::<GrupoProdutoCriado>(
                "estoque.criar_grupo_produto.v1",
                &CriarGrupoProduto {
                    codigo: "LAT".into(),
                    nome: "Laticínios".into(),
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
                &postcard::to_stdvec(c).unwrap(),
                &self.s,
                &ambiente(self.empresa),
                self.arm.escritor(),
            )
            .map(|b| postcard::from_bytes(&b).unwrap())
    }

    fn lotes(&self, dias: Option<i32>) -> Vec<LoteComValidade> {
        let b = self
            .d
            .executar_consulta(
                "estoque.lotes_com_validade.v1",
                &postcard::to_stdvec(&LotesComValidade { dias }).unwrap(),
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

    fn entrada_com_validade(
        &self,
        produto: Id,
        qtd: i64,
        reais: i64,
        validade: Data,
        codigo: &str,
    ) -> EntradaComLoteRegistrada {
        self.cmd(
            "estoque.registrar_entrada_com_lote.v1",
            &RegistrarEntradaComLote {
                produto,
                local: self.local,
                quantidade: Quantidade::unidades(qtd),
                custo_unitario: Preco::reais(reais),
                codigo_lote: codigo.into(),
                origem: OrigemLote::Compra,
                fornecedor: None,
                aparelho_origem: None,
                fabricacao: None,
                validade: Some(validade),
            },
        )
        .unwrap()
    }

    fn perda(
        &self,
        produto: Id,
        lote: Option<Id>,
        qtd: i64,
        motivo: MotivoPerda,
    ) -> Result<PerdaRegistrada, Erro> {
        self.cmd(
            "estoque.registrar_perda.v1",
            &RegistrarPerda {
                produto,
                local: self.local,
                lote,
                quantidade: Quantidade::unidades(qtd),
                motivo,
                observacao: "prateleira do fundo".into(),
            },
        )
    }

    fn historicos_de_perda(&self) -> Vec<String> {
        self.arm
            .leitor()
            .consultar(|conn| {
                let mut stmt = conn
                    .prepare("SELECT historico FROM razao_lancamento WHERE origem_tipo = 'perda'")
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?;
                let linhas = stmt
                    .query_map([], |r| r.get::<_, String>(0))
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?;
                linhas
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            })
            .unwrap()
    }
}

fn hoje() -> Data {
    Data::hoje(Fuso::BRASILIA)
}

#[test]
fn lotes_vencidos_e_a_vencer_com_saldo_e_valor() {
    let c = Cenario::novo();
    let iogurte = c.produto("Iogurte");
    let queijo = c.produto("Queijo");
    let vencido = c.entrada_com_validade(iogurte, 10, 2, hoje().mais_dias(-3), "");
    let semana = c.entrada_com_validade(iogurte, 5, 2, hoje().mais_dias(5), "");
    c.entrada_com_validade(queijo, 4, 20, hoje().mais_dias(45), "Q-001");

    // Código em branco: gerado a partir da validade, sem repetir.
    let todos = c.lotes(None);
    assert_eq!(todos.len(), 3);
    assert_eq!(todos[0].lote, vencido.lote, "o mais urgente primeiro");
    assert!(todos[0].vencido());
    assert_eq!(todos[0].dias, -3);
    assert_eq!(todos[0].saldo, Quantidade::unidades(10));
    assert_eq!(todos[0].valor, Dinheiro::reais(20));
    assert!(todos[0].codigo.starts_with('L') && todos[0].codigo.ends_with("-1"));
    assert_ne!(todos[0].codigo, todos[1].codigo);
    assert_eq!(todos[2].codigo, "Q-001");

    // Até 30 dias: os vencidos e o da semana; o queijo (45 dias) fica de fora.
    let ate_30: Vec<Id> = c.lotes(Some(30)).iter().map(|l| l.lote).collect();
    assert_eq!(ate_30, [vencido.lote, semana.lote]);
}

#[test]
fn perda_do_lote_esgota_lanca_em_perdas_com_o_motivo_e_some_do_alerta() {
    let c = Cenario::novo();
    let iogurte = c.produto("Iogurte");
    let vencido = c.entrada_com_validade(iogurte, 10, 2, hoje().mais_dias(-1), "");
    // Saldo sem lote, a outro custo, no mesmo local.
    c.cmd::<EntradaRegistrada>(
        "estoque.registrar_entrada.v1",
        &RegistrarEntrada {
            produto: iogurte,
            local: c.local,
            quantidade: Quantidade::unidades(10),
            custo_unitario: Preco::reais(4),
        },
    )
    .unwrap();

    // Mais do que o lote tem: recusado.
    assert_eq!(
        c.perda(iogurte, Some(vencido.lote), 11, MotivoPerda::Vencimento)
            .unwrap_err()
            .codigo,
        CodigoErro::ESTOQUE_INSUFICIENTE
    );
    // Perda parcial e depois o resto: o lote esgota. O valor é ao custo médio (10 a R$ 2 e
    // 10 a R$ 4 = R$ 3), como toda saída — não ao custo do lote.
    let r = c
        .perda(iogurte, Some(vencido.lote), 4, MotivoPerda::Vencimento)
        .unwrap();
    assert_eq!(r.valor, Dinheiro::reais(12));
    assert!(!r.lote_esgotado);
    let r = c
        .perda(iogurte, Some(vencido.lote), 6, MotivoPerda::Vencimento)
        .unwrap();
    assert!(r.lote_esgotado);
    assert!(r.lancamento.is_some());
    assert!(c.lotes(None).is_empty(), "lote esgotado sai do alerta");

    let historicos = c.historicos_de_perda();
    assert_eq!(historicos.len(), 2);
    assert!(historicos
        .iter()
        .all(|h| h.contains("vencimento: prateleira do fundo")));

    // Quebra sem lote, ao mesmo custo médio; acima do saldo é recusada.
    let r = c.perda(iogurte, None, 2, MotivoPerda::Quebra).unwrap();
    assert_eq!(r.valor, Dinheiro::reais(6));
    assert_eq!(
        c.perda(iogurte, None, 50, MotivoPerda::Quebra)
            .unwrap_err()
            .codigo,
        CodigoErro::ESTOQUE_INSUFICIENTE
    );
}
