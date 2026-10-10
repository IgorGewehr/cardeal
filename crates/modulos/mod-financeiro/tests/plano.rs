//! O plano de contas: modelos por segmento, contas novas e categorias que lançam em conta própria.

#![allow(clippy::result_large_err)]

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel, Sessao};
use cardeal_kernel::{CodigoErro, Data, Dinheiro, Fuso, Id, Instante, Periodo};
use cardeal_ledger::semear_plano_padrao;
use cardeal_ledger::ModeloDePlano;
use cardeal_modkit::{Ambiente, Despachante, Modulo, PedidoAtivacao, RegistroModulos};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use mod_financeiro::dre::Dre;
use mod_financeiro::{
    AplicarModeloDePlano, CategoriasPorConta, CriarCategoriasSugeridas, CriarContaDoPlano,
    DreDoPeriodo, LancarTituloAPagar, ModeloAplicado, ModuloFinanceiro, NoPlano, PlanoDeContas,
    VincularCategoriaAConta, MANIFESTO,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tempfile::TempDir;

struct Cena {
    _dir: TempDir,
    arm: Armazenamento,
    empresa: Id,
    d: Despachante,
}

fn cena() -> Cena {
    let dir = tempfile::tempdir().unwrap();
    let arm =
        Armazenamento::abrir(ConfigArmazenamento::arquivo(dir.path().join("cardeal.db"))).unwrap();
    arm.migrar(&[
        cardeal_ledger::migracoes::conjunto(),
        ModuloFinanceiro.migracoes(),
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
    Cena {
        _dir: dir,
        arm,
        empresa,
        d: Despachante::construir(&[&ModuloFinanceiro]).unwrap(),
    }
}

impl Cena {
    fn sessao(&self, usuario: Id, permissoes: &[&str]) -> Sessao {
        let mut papel = Papel::novo(self.empresa, "Teste");
        for p in permissoes {
            papel = papel.com_permissao(*p);
        }
        Sessao::abrir(EmissaoSessao::padrao(
            usuario,
            Id::novo(),
            Escopo::empresa_inteira(self.empresa),
            AutorizacoesEfetivas::consolidar([&papel]),
            Instante::EPOCA,
        ))
    }

    fn ambiente(&self) -> Ambiente {
        let mut rm = RegistroModulos::novo();
        rm.registrar(&MANIFESTO).unwrap();
        let efetivo = rm
            .resolver(&PedidoAtivacao::nova().com_modulo("financeiro"))
            .unwrap();
        Ambiente::novo(self.empresa, efetivo)
    }

    fn cmd<T: DeserializeOwned>(&self, s: &Sessao, nome: &str, c: &impl Serialize) -> T {
        let saida = self
            .d
            .executar_comando(
                nome,
                &postcard::to_stdvec(c).unwrap(),
                s,
                &self.ambiente(),
                self.arm.escritor(),
            )
            .unwrap();
        postcard::from_bytes(&saida).unwrap()
    }

    fn consulta<T: DeserializeOwned>(&self, s: &Sessao, nome: &str, c: &impl Serialize) -> T {
        let saida = self
            .d
            .executar_consulta(
                nome,
                &postcard::to_stdvec(c).unwrap(),
                s,
                &self.ambiente(),
                self.arm.leitor(),
            )
            .unwrap();
        postcard::from_bytes(&saida).unwrap()
    }
}

fn hoje() -> Data {
    Data::hoje(Fuso::BRASILIA)
}

const PERMS: &[&str] = &[
    "financeiro.plano.ver",
    "financeiro.plano.editar",
    "financeiro.categoria.criar",
    "financeiro.categoria.ver",
    "financeiro.pagar.criar",
    "financeiro.dre.ver",
];

fn plano(c: &Cena, s: &Sessao) -> Vec<NoPlano> {
    c.consulta(s, "financeiro.plano_de_contas.v1", &PlanoDeContas)
}

fn por_codigo<'a>(p: &'a [NoPlano], codigo: &str) -> &'a NoPlano {
    p.iter()
        .find(|n| n.codigo == codigo)
        .unwrap_or_else(|| panic!("conta {codigo}"))
}

#[test]
fn plano_base_vem_pronto_em_ordem_de_codigo() {
    let c = cena();
    let s = c.sessao(Id::novo(), PERMS);
    let p = plano(&c, &s);
    let codigos: Vec<&str> = p.iter().map(|n| n.codigo.as_str()).collect();
    assert!(codigos.len() > 40, "o plano padrão já vem semeado");
    let pos = |cod: &str| codigos.iter().position(|c| *c == cod).unwrap();
    assert!(pos("1") < pos("1.1") && pos("1.1") < pos("1.1.01") && pos("1.1.01") < pos("1.2"));
    assert!(pos("5.3") < pos("5.10"), "ordem numérica, não alfabética");
    assert!(por_codigo(&p, "5.1").papel.as_deref() == Some("Cmv"));
    assert!(por_codigo(&p, "5").sintetica);
}

#[test]
fn modelo_acrescenta_contas_uma_vez_so_e_liga_as_categorias_de_mesmo_nome() {
    let c = cena();
    let s = c.sessao(Id::novo(), PERMS);
    let _: u32 = c.cmd(
        &s,
        "financeiro.criar_categorias_sugeridas.v1",
        &CriarCategoriasSugeridas,
    );
    let antes: Vec<(Id, Id)> = c.consulta(
        &s,
        "financeiro.categorias_por_conta.v1",
        &CategoriasPorConta,
    );
    // A única de mesmo nome no plano base é "Outras receitas": já lança na conta certa.
    assert_eq!(antes.len(), 1);

    let r: ModeloAplicado = c.cmd(
        &s,
        "financeiro.aplicar_modelo_de_plano.v1",
        &AplicarModeloDePlano {
            modelo: ModeloDePlano::AssistenciaTecnica,
        },
    );
    assert!(r.criadas.contains(&"5.20".to_owned()) && r.criadas.contains(&"4.22".to_owned()));
    assert!(
        r.categorias_vinculadas >= 10,
        "aluguel, energia, pró-labore…: {}",
        r.categorias_vinculadas
    );
    let p = plano(&c, &s);
    assert_eq!(por_codigo(&p, "5.20").nome, "Aluguel");
    assert_eq!(
        por_codigo(&p, "4.22").papel.as_deref(),
        Some("ReceitaServicos")
    );

    // Idempotente: a segunda vez não cria nada.
    let r2: ModeloAplicado = c.cmd(
        &s,
        "financeiro.aplicar_modelo_de_plano.v1",
        &AplicarModeloDePlano {
            modelo: ModeloDePlano::AssistenciaTecnica,
        },
    );
    assert!(r2.criadas.is_empty());
    // Outro modelo soma só o que lhe é próprio.
    let r3: ModeloAplicado = c.cmd(
        &s,
        "financeiro.aplicar_modelo_de_plano.v1",
        &AplicarModeloDePlano {
            modelo: ModeloDePlano::Comercio,
        },
    );
    assert!(!r3.criadas.contains(&"5.20".to_owned()));
}

#[test]
fn despesa_com_categoria_vinculada_cai_na_conta_propria_e_aparece_na_dre() {
    let c = cena();
    let s = c.sessao(Id::novo(), PERMS);
    let _: u32 = c.cmd(
        &s,
        "financeiro.criar_categorias_sugeridas.v1",
        &CriarCategoriasSugeridas,
    );
    let _: ModeloAplicado = c.cmd(
        &s,
        "financeiro.aplicar_modelo_de_plano.v1",
        &AplicarModeloDePlano {
            modelo: ModeloDePlano::Comercio,
        },
    );
    let categorias: Vec<mod_financeiro::CategoriaFinanceira> =
        c.consulta(&s, "financeiro.categorias.v1", &mod_financeiro::Categorias);
    let aluguel = categorias.iter().find(|x| x.nome == "Aluguel").unwrap().id;

    let h = hoje();
    let _: mod_financeiro::TituloAPagarLancado = c.cmd(
        &s,
        "financeiro.lancar_titulo_a_pagar.v1",
        &LancarTituloAPagar {
            fornecedor: None,
            valor_total: Dinheiro::reais(1_500),
            emissao: h,
            parcelas: 1,
            primeiro_vencimento: h,
            intervalo_dias: 0,
            observacao: None,
            categoria: Some(aluguel),
            quitado_agora: None,
        },
    );
    // Sem categoria vinculada: cai na conta genérica (administrativas), como sempre.
    let _: mod_financeiro::TituloAPagarLancado = c.cmd(
        &s,
        "financeiro.lancar_titulo_a_pagar.v1",
        &LancarTituloAPagar {
            fornecedor: None,
            valor_total: Dinheiro::reais(200),
            emissao: h,
            parcelas: 1,
            primeiro_vencimento: h,
            intervalo_dias: 0,
            observacao: None,
            categoria: None,
            quitado_agora: None,
        },
    );
    let dre: Dre = c.consulta(
        &s,
        "financeiro.dre_do_periodo.v1",
        &DreDoPeriodo {
            periodo: Periodo::novo(h.inicio_do_mes(), h.fim_do_mes()),
        },
    );
    let linha = |nome: &str| {
        dre.linhas
            .iter()
            .find(|l| l.rotulo == nome)
            .map(|l| l.valor)
    };
    assert_eq!(
        linha("Aluguel"),
        Some(Dinheiro::reais(-1_500)),
        "o aluguel ganhou linha própria"
    );
    assert_eq!(
        linha("Despesas administrativas"),
        Some(Dinheiro::reais(-200))
    );
    assert_eq!(dre.lucro_liquido, Dinheiro::reais(-1_700));
}

#[test]
fn vinculo_exige_conta_analitica_da_natureza_certa_e_pode_ser_desfeito() {
    let c = cena();
    let s = c.sessao(Id::novo(), PERMS);
    let _: u32 = c.cmd(
        &s,
        "financeiro.criar_categorias_sugeridas.v1",
        &CriarCategoriasSugeridas,
    );
    let categorias: Vec<mod_financeiro::CategoriaFinanceira> =
        c.consulta(&s, "financeiro.categorias.v1", &mod_financeiro::Categorias);
    let aluguel = categorias.iter().find(|x| x.nome == "Aluguel").unwrap().id;
    let p = plano(&c, &s);

    let tenta = |conta: Option<Id>| {
        c.d.executar_comando(
            "financeiro.vincular_categoria_a_conta.v1",
            &postcard::to_stdvec(&VincularCategoriaAConta {
                categoria: aluguel,
                conta,
            })
            .unwrap(),
            &s,
            &c.ambiente(),
            c.arm.escritor(),
        )
    };
    // Conta de receita para categoria de pagar, e conta sintética: recusadas.
    assert_eq!(
        tenta(Some(por_codigo(&p, "4.1").id)).unwrap_err().codigo,
        CodigoErro::REGRA_VIOLADA
    );
    assert_eq!(
        tenta(Some(por_codigo(&p, "5").id)).unwrap_err().codigo,
        CodigoErro::REGRA_VIOLADA
    );
    // Despesa analítica: aceita; e desfaz com None.
    tenta(Some(por_codigo(&p, "5.4").id)).unwrap();
    let pares: Vec<(Id, Id)> = c.consulta(
        &s,
        "financeiro.categorias_por_conta.v1",
        &CategoriasPorConta,
    );
    assert!(pares.contains(&(por_codigo(&p, "5.4").id, aluguel)));
    assert_eq!(pares.len(), 2, "mais 'Outras receitas', ligada pelo nome");
    tenta(None).unwrap();
    let depois: Vec<(Id, Id)> = c.consulta(
        &s,
        "financeiro.categorias_por_conta.v1",
        &CategoriasPorConta,
    );
    assert_eq!(depois.len(), 1);
}

#[test]
fn conta_nova_ganha_codigo_respeita_5_niveis_e_so_agrupadora_recebe_filha() {
    let c = cena();
    let s = c.sessao(Id::novo(), PERMS);
    let p = plano(&c, &s);
    let raiz = por_codigo(&p, "5").id;
    let nova =
        |pai: Id, nome: &str, agrupadora: bool| -> Result<mod_financeiro::ContaDoPlanoCriada, _> {
            c.d.executar_comando(
                "financeiro.criar_conta_do_plano.v1",
                &postcard::to_stdvec(&CriarContaDoPlano {
                    pai,
                    nome: nome.into(),
                    agrupadora,
                })
                .unwrap(),
                &s,
                &c.ambiente(),
                c.arm.escritor(),
            )
            .map(|b| postcard::from_bytes(&b).unwrap())
        };
    let g1 = nova(raiz, "Despesas da loja", true).unwrap();
    assert_eq!(g1.codigo, "5.12", "o próximo livre depois de 5.11");
    let g2 = nova(g1.conta, "Operação", true).unwrap();
    assert_eq!(g2.codigo, "5.12.01");
    let g3 = nova(g2.conta, "Fixas", true).unwrap();
    let folha = nova(g3.conta, "Conta no nível cinco", false).unwrap();
    assert_eq!(folha.codigo.split('.').count(), 5);
    let g4 = nova(g3.conta, "Agrupadora no nível cinco", true).unwrap();
    // Abaixo do 5º nível, ou sob conta analítica, ou sem nome: recusa.
    assert!(nova(g4.conta, "Sexto", false).is_err());
    assert!(nova(folha.conta, "Filha de analítica", false).is_err());
    assert!(nova(por_codigo(&p, "5.1").id, "Filha de analítica", false).is_err());
    assert!(nova(raiz, "  ", false).is_err());
    // A conta nova aparece na árvore, no lugar certo.
    let p = plano(&c, &s);
    let pos = |cod: &str| p.iter().position(|n| n.codigo == cod).unwrap();
    assert!(pos("5.12") > pos("5.11") && pos("5.12.01") == pos("5.12") + 1);
}
