//! A formação de preço (markup divisor, margem herdada, perfil fiscal) pelo `Despachante` real.

#![allow(clippy::result_large_err)] // `ErroArmazenamento` carrega detalhes de propósito

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel as PapelAuth, Sessao};
use cardeal_kernel::{CodigoErro, Dinheiro, Erro, Id, Instante, Percentual, Preco, Quantidade};
use cardeal_ledger::semear_plano_padrao;
use cardeal_modkit::{Ambiente, Despachante, Modulo, PedidoAtivacao, RegistroModulos};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use mod_estoque::fiscal::CamposFiscais;
use mod_estoque::historico_compras::ResumoDeCompras;
use mod_estoque::precificacao::{OrigemDaCarga, RegimeImposto};
use mod_estoque::{
    CriarGrupoProduto, CriarLocal, CriarProduto, CriarUnidade, CustoDeReferencia,
    DefinirCamposFiscaisDoGrupo, DefinirCamposFiscaisDoProduto, DefinirMargemDoGrupo,
    DefinirMargemDoProduto, DefinirParametrosPreco, DefinirPerfilFiscalDoGrupo, GrupoProdutoCriado,
    HistoricoDeCompras, LocalCriado, ModuloEstoque, OrigemMargem, ParametrosDePreco,
    ParametrosPreco, PrecoSugeridoDoProduto, PrecoSugeridoNoGrupo, PrecosSugeridosDoGrupo,
    ProdutoCriado, RegistrarEntrada, SugestaoDePreco, TipoLocal, UnidadeCriada, MANIFESTO,
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

const PERMISSOES: &[&str] = &[
    "estoque.preco.simples.ver",
    "estoque.produto.criar",
    "estoque.produto.editar",
    "estoque.produto.ver",
    "estoque.tributario.editar",
    "estoque.local.criar",
    "estoque.movimento.entrada",
];

struct Cenario {
    _dir: TempDir,
    arm: Armazenamento,
    empresa: Id,
    d: Despachante,
    s: Sessao,
}

impl Cenario {
    fn novo() -> Self {
        let (dir, arm, empresa) = base();
        Self {
            _dir: dir,
            arm,
            empresa,
            d: Despachante::construir(&[&ModuloEstoque]).unwrap(),
            s: sessao(empresa, PERMISSOES),
        }
    }

    fn cmd<C: serde::Serialize>(&self, nome: &str, c: &C) -> Result<Vec<u8>, Erro> {
        self.d.executar_comando(
            nome,
            &carga(c),
            &self.s,
            &ambiente(self.empresa),
            self.arm.escritor(),
        )
    }

    fn consulta<C: serde::Serialize, R: serde::de::DeserializeOwned>(
        &self,
        nome: &str,
        c: &C,
    ) -> R {
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

    fn grupo(&self, codigo: &str, pai: Option<Id>) -> Id {
        let b = self
            .cmd(
                "estoque.criar_grupo_produto.v1",
                &CriarGrupoProduto {
                    codigo: codigo.to_owned(),
                    nome: codigo.to_owned(),
                    pai,
                },
            )
            .unwrap();
        postcard::from_bytes::<GrupoProdutoCriado>(&b)
            .unwrap()
            .grupo_produto
    }

    /// Cria um produto no grupo e dá entrada de 10 unidades ao custo.
    fn produto_com_custo(&self, nome: &str, grupo: Id, custo: Preco) -> Id {
        let un: UnidadeCriada = postcard::from_bytes(
            &self
                .cmd(
                    "estoque.criar_unidade.v1",
                    &CriarUnidade {
                        sigla: "UN".into(),
                        nome: "Un".into(),
                        fracionavel: false,
                    },
                )
                .or_else(|_| {
                    // já existe: busca pela consulta de unidades
                    let u: Vec<mod_estoque::ItemUnidade> =
                        self.consulta("estoque.unidades.v1", &mod_estoque::Unidades);
                    Ok::<_, Erro>(carga(&UnidadeCriada { unidade: u[0].id }))
                })
                .unwrap(),
        )
        .unwrap();
        let p: ProdutoCriado = postcard::from_bytes(
            &self
                .cmd(
                    "estoque.criar_produto.v1",
                    &CriarProduto {
                        grupo_produto: grupo,
                        nome: nome.to_owned(),
                        ncm: "22021000".into(),
                        unidade_padrao: un.unidade,
                        codigo_barras: None,
                        detalhes_tecnicos: None,
                    },
                )
                .unwrap(),
        )
        .unwrap();
        let local: Vec<mod_estoque::ItemLocal> =
            self.consulta("estoque.locais.v1", &mod_estoque::Locais);
        let local = if let Some(l) = local.first() {
            l.id
        } else {
            postcard::from_bytes::<LocalCriado>(
                &self
                    .cmd(
                        "estoque.criar_local.v1",
                        &CriarLocal {
                            nome: "Depósito".into(),
                            tipo: TipoLocal::Deposito,
                        },
                    )
                    .unwrap(),
            )
            .unwrap()
            .local
        };
        self.cmd(
            "estoque.registrar_entrada.v1",
            &RegistrarEntrada {
                produto: p.produto,
                local,
                quantidade: Quantidade::unidades(10),
                custo_unitario: custo,
            },
        )
        .unwrap();
        p.produto
    }

    fn preco(&self, produto: Id) -> SugestaoDePreco {
        self.consulta(
            "estoque.preco_sugerido_do_produto.v1",
            &PrecoSugeridoDoProduto {
                produto,
                custo: None,
            },
        )
    }

    fn parametros_manuais(&self, opex: i64) {
        let mut p: ParametrosPreco =
            self.consulta("estoque.parametros_de_preco.v1", &ParametrosDePreco);
        p.custo_operacional = Percentual::pontos(opex);
        p.custo_operacional_derivado = false;
        self.cmd(
            "estoque.definir_parametros_preco.v1",
            &DefinirParametrosPreco { parametros: p },
        )
        .unwrap();
    }
}

#[test]
fn sem_margem_definida_o_sistema_diz_o_que_falta() {
    let c = Cenario::novo();
    let g = c.grupo("BEB", None);
    let p = c.produto_com_custo("Coca 2L", g, Preco::reais(50));
    let s = c.preco(p);
    assert_eq!(s.origem_margem, OrigemMargem::Nenhuma);
    assert!(s.sugerido.is_none());
    assert!(s.problema.unwrap().contains("margem"));
}

#[test]
fn margem_herdada_do_grupo_e_perfil_monofasico_formam_o_preco() {
    let c = Cenario::novo();
    c.parametros_manuais(25);
    let bebidas = c.grupo("BEB", None);
    let refri = c.grupo("REF", Some(bebidas));
    let p = c.produto_com_custo("Coca 2L", refri, Preco::reais(50));

    c.cmd(
        "estoque.definir_margem_do_grupo.v1",
        &DefinirMargemDoGrupo {
            grupo: bebidas,
            margem: Some(Percentual::pontos(20)),
        },
    )
    .unwrap();
    c.cmd(
        "estoque.definir_perfil_fiscal_do_grupo.v1",
        &DefinirPerfilFiscalDoGrupo {
            grupo: bebidas,
            icms_st: None,
            monofasico: Some(true),
        },
    )
    .unwrap();

    let s = c.preco(p);
    assert_eq!(s.origem_margem, OrigemMargem::Grupo("BEB".into()));
    assert!(
        s.perfil_fiscal.monofasico && !s.perfil_fiscal.icms_st,
        "herdou do pai"
    );
    // Simples Anexo I, sem faturamento: 4% × (1 − 15,5%) = 3,38%.
    assert_eq!(s.imposto, Percentual::unidades(3_380_000));
    // 25 + 20 + 3,38 = 48,38 → divisor 51,62% → 50 / 0,5162 = 96,86
    assert_eq!(s.sugerido.unwrap().preco, Dinheiro::centavos(9686));
    // O mínimo (sem margem): 50 / (1 − 0,2838) = 69,81
    assert_eq!(s.minimo.unwrap().preco, Dinheiro::centavos(6981));
}

#[test]
fn margem_do_produto_vale_mais_que_a_do_grupo_e_limpar_volta_a_herdar() {
    let c = Cenario::novo();
    c.parametros_manuais(25);
    let g = c.grupo("BEB", None);
    let p = c.produto_com_custo("Coca 2L", g, Preco::reais(50));
    c.cmd(
        "estoque.definir_margem_do_grupo.v1",
        &DefinirMargemDoGrupo {
            grupo: g,
            margem: Some(Percentual::pontos(20)),
        },
    )
    .unwrap();
    c.cmd(
        "estoque.definir_margem_do_produto.v1",
        &DefinirMargemDoProduto {
            produto: p,
            margem: Some(Percentual::pontos(10)),
        },
    )
    .unwrap();
    let s = c.preco(p);
    assert_eq!(s.origem_margem, OrigemMargem::Produto);
    // 25 + 10 + 4 = 39 → 50 / 0,61 = 81,97
    assert_eq!(s.sugerido.unwrap().preco, Dinheiro::centavos(8197));

    c.cmd(
        "estoque.definir_margem_do_produto.v1",
        &DefinirMargemDoProduto {
            produto: p,
            margem: None,
        },
    )
    .unwrap();
    assert_eq!(c.preco(p).origem_margem, OrigemMargem::Grupo("BEB".into()));
}

#[test]
fn custo_informado_recalcula_o_markup_sem_mexer_no_estoque() {
    let c = Cenario::novo();
    c.parametros_manuais(25);
    let g = c.grupo("BEB", None);
    let p = c.produto_com_custo("Coca 2L", g, Preco::reais(50));
    c.cmd(
        "estoque.definir_margem_do_grupo.v1",
        &DefinirMargemDoGrupo {
            grupo: g,
            margem: Some(Percentual::pontos(15)),
        },
    )
    .unwrap();
    let s: SugestaoDePreco = c.consulta(
        "estoque.preco_sugerido_do_produto.v1",
        &PrecoSugeridoDoProduto {
            produto: p,
            custo: Some(Dinheiro::reais(60)),
        },
    );
    assert_eq!(s.custo, Some(Dinheiro::reais(60)));
    // 25 + 15 + 4 = 44 → 60 / 0,56 = 107,14
    assert_eq!(s.sugerido.unwrap().preco, Dinheiro::centavos(10714));
}

#[test]
fn soma_de_percentuais_que_nao_fecha_vira_mensagem_e_nao_erro() {
    let c = Cenario::novo();
    c.parametros_manuais(70);
    let g = c.grupo("BEB", None);
    let p = c.produto_com_custo("Coca 2L", g, Preco::reais(50));
    c.cmd(
        "estoque.definir_margem_do_grupo.v1",
        &DefinirMargemDoGrupo {
            grupo: g,
            margem: Some(Percentual::pontos(30)),
        },
    )
    .unwrap();
    let s = c.preco(p);
    assert!(s.sugerido.is_none());
    assert!(s.problema.unwrap().contains("somam"));
}

#[test]
fn precos_do_grupo_cobrem_a_subarvore() {
    let c = Cenario::novo();
    c.parametros_manuais(25);
    let a = c.grupo("A", None);
    let b = c.grupo("B", Some(a));
    let outro = c.grupo("OUTRO", None);
    c.cmd(
        "estoque.definir_margem_do_grupo.v1",
        &DefinirMargemDoGrupo {
            grupo: a,
            margem: Some(Percentual::pontos(20)),
        },
    )
    .unwrap();
    c.produto_com_custo("No pai", a, Preco::reais(10));
    c.produto_com_custo("No filho", b, Preco::reais(20));
    c.produto_com_custo("Fora", outro, Preco::reais(30));
    let v: Vec<SugestaoDePreco> = c.consulta(
        "estoque.precos_sugeridos_do_grupo.v1",
        &PrecosSugeridosDoGrupo { grupo: Some(a) },
    );
    let nomes: Vec<&str> = v.iter().map(|s| s.nome.as_str()).collect();
    assert_eq!(nomes, ["No filho", "No pai"]);
    assert!(v.iter().all(|s| s.sugerido.is_some()));
    // Sem grupo: o catálogo inteiro, incluindo o produto sem margem (com o problema explicado).
    let todos: Vec<SugestaoDePreco> = c.consulta(
        "estoque.precos_sugeridos_do_grupo.v1",
        &PrecosSugeridosDoGrupo { grupo: None },
    );
    assert_eq!(todos.len(), 3);
    assert!(todos
        .iter()
        .find(|s| s.nome == "Fora")
        .unwrap()
        .problema
        .is_some());
}

#[test]
fn parametros_invalidos_e_falta_de_permissao_sao_recusados() {
    let c = Cenario::novo();
    let g = c.grupo("BEB", None);
    // Margem de 100% não deixa preço possível.
    assert!(c
        .cmd(
            "estoque.definir_margem_do_grupo.v1",
            &DefinirMargemDoGrupo {
                grupo: g,
                margem: Some(Percentual::CEM)
            },
        )
        .is_err());
    // Comissão acima de 100%.
    let mut p: ParametrosPreco = c.consulta("estoque.parametros_de_preco.v1", &ParametrosDePreco);
    p.comissao = Percentual::pontos(150);
    let e = c
        .cmd(
            "estoque.definir_parametros_preco.v1",
            &DefinirParametrosPreco { parametros: p },
        )
        .unwrap_err();
    assert_eq!(e.codigo, CodigoErro::FORA_DA_FAIXA);

    // Sem estoque.tributario.editar: nem parâmetros nem perfil fiscal.
    let sem = sessao(
        c.empresa,
        &["estoque.produto.editar", "estoque.produto.ver"],
    );
    let tenta = |nome: &str, b: Vec<u8>| {
        c.d.executar_comando(nome, &b, &sem, &ambiente(c.empresa), c.arm.escritor())
    };
    assert_eq!(
        tenta(
            "estoque.definir_parametros_preco.v1",
            carga(&DefinirParametrosPreco {
                parametros: ParametrosPreco::padrao_para("MEI")
            })
        )
        .unwrap_err()
        .codigo,
        CodigoErro::SEM_PERMISSAO
    );
    assert_eq!(
        tenta(
            "estoque.definir_perfil_fiscal_do_grupo.v1",
            carga(&DefinirPerfilFiscalDoGrupo {
                grupo: g,
                icms_st: Some(true),
                monofasico: None
            })
        )
        .unwrap_err()
        .codigo,
        CodigoErro::SEM_PERMISSAO
    );
}

#[test]
fn sem_parametros_gravados_vale_o_regime_cadastrado() {
    let c = Cenario::novo();
    // A empresa de teste é SimplesNacional → Anexo I, custo operacional derivado do Razão.
    let p: ParametrosPreco = c.consulta("estoque.parametros_de_preco.v1", &ParametrosDePreco);
    assert_eq!(p, ParametrosPreco::padrao_para("SimplesNacional"));
}

impl Cenario {
    fn regime_cadastral(&self, regime: &'static str) {
        let empresa = self.empresa;
        self.arm
            .escritor()
            .executar(
                ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo()),
                move |uow| {
                    uow.conexao()
                        .execute(
                            "UPDATE nucleo_empresa SET regime = ?2 WHERE id = ?1",
                            rusqlite::params![empresa.em_bytes().as_slice(), regime],
                        )
                        .map(|_| ())
                        .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
                },
            )
            .unwrap();
    }

    fn regime_manual(&self, imposto: i64) {
        let mut p = ParametrosPreco::padrao_para("LucroPresumido");
        p.regime = RegimeImposto::Manual(Percentual::pontos(imposto));
        p.custo_operacional = Percentual::pontos(25);
        p.custo_operacional_derivado = false;
        self.cmd(
            "estoque.definir_parametros_preco.v1",
            &DefinirParametrosPreco { parametros: p },
        )
        .unwrap();
    }

    fn margem_do_grupo(&self, grupo: Id, pontos: i64) {
        self.cmd(
            "estoque.definir_margem_do_grupo.v1",
            &DefinirMargemDoGrupo {
                grupo,
                margem: Some(Percentual::pontos(pontos)),
            },
        )
        .unwrap();
    }
}

fn icms_pis_cofins(icms: Option<i64>, pis: Option<i64>, cofins: Option<i64>) -> CamposFiscais {
    CamposFiscais {
        aliquota_icms: icms.map(Percentual::pontos),
        // 0,65% = 650_000 unidades internas.
        aliquota_pis: pis.map(Percentual::unidades),
        aliquota_cofins: cofins.map(Percentual::pontos),
        ..CamposFiscais::default()
    }
}

#[test]
fn presumido_usa_os_campos_fiscais_herdados_do_grupo_e_o_produto_sobrescreve() {
    let c = Cenario::novo();
    c.regime_cadastral("LucroPresumido");
    c.regime_manual(10);
    let g = c.grupo("ELE", None);
    c.margem_do_grupo(g, 20);
    let p = c.produto_com_custo("Cabo", g, Preco::reais(50));

    // Sem campos fiscais: o percentual manual (10%). 25 + 20 + 10 = 55 → 50 / 0,45 = 111,11.
    let s = c.preco(p);
    assert_eq!(s.imposto, Percentual::pontos(10));
    assert_eq!(s.imposto_origem, OrigemDaCarga::Regime);
    assert_eq!(s.sugerido.unwrap().preco, Dinheiro::centavos(11111));

    // O grupo define ICMS 18% + PIS 0,65% + COFINS 3%: o produto herda. Pelo motor (Presumido):
    // ICMS 18 + PIS 0,65 × 0,82 + COFINS 3 × 0,82 (sem o ICMS na base) + IRPJ 1,2 + CSLL 1,08
    // = 23,273%. 25 + 20 + 23,273 = 68,273 → 50 / 0,31727 = 157,59.
    c.cmd(
        "estoque.definir_campos_fiscais_do_grupo.v1",
        &DefinirCamposFiscaisDoGrupo {
            grupo: g,
            campos: icms_pis_cofins(Some(18), Some(650_000), Some(3)),
        },
    )
    .unwrap();
    let s = c.preco(p);
    assert_eq!(s.imposto, Percentual::unidades(23_273_000));
    assert_eq!(s.imposto_origem, OrigemDaCarga::PerfilFiscalDoProduto);
    assert_eq!(s.sugerido.unwrap().preco, Dinheiro::centavos(15759));

    // O produto sobrescreve só o ICMS (12%); PIS/COFINS continuam do grupo:
    // 12 + 0,65 × 0,88 + 3 × 0,88 + 2,28 = 17,492%. 25 + 20 + 17,492 → 50 / 0,37508 = 133,30.
    c.cmd(
        "estoque.definir_campos_fiscais_do_produto.v1",
        &DefinirCamposFiscaisDoProduto {
            produto: p,
            campos: icms_pis_cofins(Some(12), None, None),
        },
    )
    .unwrap();
    let s = c.preco(p);
    assert_eq!(s.imposto, Percentual::unidades(17_492_000));
    assert_eq!(s.sugerido.unwrap().preco, Dinheiro::centavos(13330));
}

#[test]
fn simples_ignora_os_campos_fiscais_no_imposto() {
    let c = Cenario::novo();
    c.parametros_manuais(25);
    let g = c.grupo("ELE", None);
    c.margem_do_grupo(g, 20);
    let p = c.produto_com_custo("Cabo", g, Preco::reais(50));
    let antes = c.preco(p);
    c.cmd(
        "estoque.definir_campos_fiscais_do_grupo.v1",
        &DefinirCamposFiscaisDoGrupo {
            grupo: g,
            campos: icms_pis_cofins(Some(18), Some(650_000), Some(3)),
        },
    )
    .unwrap();
    let depois = c.preco(p);
    assert_eq!(depois.imposto, antes.imposto);
    assert_eq!(depois.imposto_origem, OrigemDaCarga::Regime);
    assert_eq!(
        depois.sugerido.unwrap().preco,
        antes.sugerido.unwrap().preco
    );
}

// ── custo de referência e histórico de compras ─────────────────────────────────────────────

impl Cenario {
    fn produto_sem_entrada(&self, nome: &str, grupo: Id) -> Id {
        let un: Vec<mod_estoque::ItemUnidade> =
            self.consulta("estoque.unidades.v1", &mod_estoque::Unidades);
        let b = self
            .cmd(
                "estoque.criar_produto.v1",
                &CriarProduto {
                    grupo_produto: grupo,
                    nome: nome.to_owned(),
                    ncm: "22021000".into(),
                    unidade_padrao: un[0].id,
                    codigo_barras: None,
                    detalhes_tecnicos: None,
                },
            )
            .unwrap();
        postcard::from_bytes::<ProdutoCriado>(&b).unwrap().produto
    }

    fn nova_entrada(&self, produto: Id, custo: Preco) {
        let local: Vec<mod_estoque::ItemLocal> =
            self.consulta("estoque.locais.v1", &mod_estoque::Locais);
        self.cmd(
            "estoque.registrar_entrada.v1",
            &RegistrarEntrada {
                produto,
                local: local[0].id,
                quantidade: Quantidade::unidades(10),
                custo_unitario: custo,
            },
        )
        .unwrap();
    }

    fn referencia(&self, r: CustoDeReferencia) {
        let mut p: ParametrosPreco =
            self.consulta("estoque.parametros_de_preco.v1", &ParametrosDePreco);
        p.custo_referencia = r;
        self.cmd(
            "estoque.definir_parametros_preco.v1",
            &DefinirParametrosPreco { parametros: p },
        )
        .unwrap();
    }
}

#[test]
fn o_preco_sugerido_muda_conforme_o_custo_de_referencia() {
    let c = Cenario::novo();
    c.parametros_manuais(0);
    let g = c.grupo("BEB", None);
    c.cmd(
        "estoque.definir_margem_do_grupo.v1",
        &DefinirMargemDoGrupo {
            grupo: g,
            margem: Some(Percentual::pontos(20)),
        },
    )
    .unwrap();
    // 10 un a R$ 50 e depois 10 un a R$ 70: custo médio R$ 60, última compra R$ 70.
    let p = c.produto_com_custo("Coca 2L", g, Preco::reais(50));
    c.nova_entrada(p, Preco::reais(70));

    let padrao: ParametrosPreco = c.consulta("estoque.parametros_de_preco.v1", &ParametrosDePreco);
    assert_eq!(padrao.custo_referencia, CustoDeReferencia::CustoMedio);
    let medio = c.preco(p);
    assert_eq!(medio.custo, Some(Dinheiro::reais(60)));
    assert_eq!(medio.custo_origem, CustoDeReferencia::CustoMedio);

    c.referencia(CustoDeReferencia::UltimaCompra);
    let ultima = c.preco(p);
    assert_eq!(ultima.custo, Some(Dinheiro::reais(70)));
    assert_eq!(ultima.custo_origem, CustoDeReferencia::UltimaCompra);
    assert!(ultima.sugerido.unwrap().preco > medio.sugerido.unwrap().preco);

    // Maior dos dois: a última (70) vence o médio (60)...
    c.referencia(CustoDeReferencia::MaiorDosDois);
    let maior = c.preco(p);
    assert_eq!(maior.custo, Some(Dinheiro::reais(70)));
    assert_eq!(maior.custo_origem, CustoDeReferencia::UltimaCompra);
    // ...e quando a compra mais recente é mais barata, vence o médio (65 > 60).
    c.nova_entrada(p, Preco::reais(40));
    let maior = c.preco(p);
    assert_eq!(maior.custo_origem, CustoDeReferencia::CustoMedio);
    assert!(maior.custo.unwrap() > Dinheiro::reais(40));
}

#[test]
fn historico_de_compras_vem_do_mais_recente_com_variacao_e_media() {
    let c = Cenario::novo();
    let g = c.grupo("BEB", None);
    let p = c.produto_com_custo("Coca 2L", g, Preco::reais(50));
    c.nova_entrada(p, Preco::reais(55));
    c.nova_entrada(p, Preco::reais(44));

    let h: ResumoDeCompras = c.consulta(
        "estoque.historico_de_compras.v1",
        &HistoricoDeCompras { produto: p },
    );
    let custos: Vec<Dinheiro> = h.compras.iter().map(|x| x.custo_unitario).collect();
    assert_eq!(
        custos,
        [
            Dinheiro::reais(44),
            Dinheiro::reais(55),
            Dinheiro::reais(50)
        ]
    );
    assert_eq!(h.compras[0].variacao, Some(Percentual::pontos(-20)));
    assert_eq!(h.compras[1].variacao, Some(Percentual::pontos(10)));
    assert_eq!(h.compras[2].variacao, None);
    assert_eq!(h.ultimo_custo, Some(Dinheiro::reais(44)));
    assert_eq!(h.media_ultimas_3, Some(Dinheiro::centavos(4_967)));
    assert_eq!(h.compras[0].origem_modulo, "estoque");

    // Produto sem compra: histórico vazio, sem erro.
    let sem = c.produto_sem_entrada("Sem compra", g);
    let h: ResumoDeCompras = c.consulta(
        "estoque.historico_de_compras.v1",
        &HistoricoDeCompras { produto: sem },
    );
    assert!(h.compras.is_empty() && h.ultimo_custo.is_none());
}

impl Cenario {
    fn comissao_padrao(&self, pontos: i64) {
        let mut p: ParametrosPreco =
            self.consulta("estoque.parametros_de_preco.v1", &ParametrosDePreco);
        p.comissao = Percentual::pontos(pontos);
        self.cmd(
            "estoque.definir_parametros_preco.v1",
            &DefinirParametrosPreco { parametros: p },
        )
        .unwrap();
    }

    fn comissao_do_grupo(&self, grupo: Id, comissao: Option<Percentual>) -> Result<Vec<u8>, Erro> {
        self.cmd(
            "estoque.definir_comissao_do_grupo.v1",
            &mod_estoque::DefinirComissaoDoGrupo { grupo, comissao },
        )
    }

    fn comissao_do_produto(&self, produto: Id, comissao: Option<Percentual>) {
        self.cmd(
            "estoque.definir_comissao_do_produto.v1",
            &mod_estoque::DefinirComissaoDoProduto { produto, comissao },
        )
        .unwrap();
    }
}

#[test]
fn comissao_por_estrutura_herda_do_grupo_e_o_produto_sobrescreve() {
    use mod_estoque::OrigemComissao;
    let c = Cenario::novo();
    c.parametros_manuais(25);
    c.comissao_padrao(2);
    let bebidas = c.grupo("BEB", None);
    let refri = c.grupo("REF", Some(bebidas));
    let p = c.produto_com_custo("Coca 2L", refri, Preco::reais(50));
    c.margem_do_grupo(bebidas, 20);

    // Ninguém definiu: vale a padrão. 25 + 20 + 4 + 2 = 51 → 50 / 0,49 = 102,04
    let s = c.preco(p);
    assert_eq!(
        (s.comissao, s.origem_comissao.clone()),
        (Percentual::pontos(2), OrigemComissao::Padrao)
    );
    assert_eq!(s.componentes.unwrap().comissao, Percentual::pontos(2));
    assert_eq!(s.sugerido.unwrap().preco, Dinheiro::centavos(10204));

    // A do grupo de cima vale para o subgrupo: 25 + 20 + 4 + 5 = 54 → 50 / 0,46 = 108,70
    c.comissao_do_grupo(bebidas, Some(Percentual::pontos(5)))
        .unwrap();
    let s = c.preco(p);
    assert_eq!(s.origem_comissao, OrigemComissao::Grupo("BEB".into()));
    assert_eq!(s.sugerido.unwrap().preco, Dinheiro::centavos(10870));
    let arvore: Vec<mod_estoque::NoArvoreGrupo> = c.consulta(
        "estoque.arvore_grupos_produto.v1",
        &mod_estoque::ArvoreGruposProduto,
    );
    let beb = arvore.iter().find(|n| n.id == bebidas).unwrap();
    let ref_ = arvore.iter().find(|n| n.id == refri).unwrap();
    assert_eq!(
        (beb.comissao, ref_.comissao),
        (Some(Percentual::pontos(5)), None)
    );

    // A do produto vale mais: 25 + 20 + 4 + 1 = 50 → 100,00. Limpar volta ao grupo.
    c.comissao_do_produto(p, Some(Percentual::pontos(1)));
    let s = c.preco(p);
    assert_eq!(s.origem_comissao, OrigemComissao::Produto);
    assert_eq!(s.sugerido.unwrap().preco, Dinheiro::centavos(10000));
    c.comissao_do_produto(p, None);
    assert_eq!(
        c.preco(p).origem_comissao,
        OrigemComissao::Grupo("BEB".into())
    );

    // Fora da faixa é recusado.
    let e = c
        .comissao_do_grupo(bebidas, Some(Percentual::CEM))
        .unwrap_err();
    assert_eq!(e.codigo, CodigoErro::FORA_DA_FAIXA);
}

#[test]
fn comissao_pela_alteracao_em_lote_e_desfazer_volta_a_herdar() {
    use mod_estoque::alteracao_em_lote::{Alteracao, Mudanca};
    use mod_estoque::{AlteracaoAplicada, OrigemComissao};
    let c = Cenario::novo();
    c.parametros_manuais(25);
    let g = c.grupo("BEB", None);
    c.margem_do_grupo(g, 20);
    let a = c.produto_com_custo("Coca 2L", g, Preco::reais(50));
    let b = c.produto_com_custo("Guaraná 2L", g, Preco::reais(50));

    let r: AlteracaoAplicada = postcard::from_bytes(
        &c.cmd(
            "estoque.aplicar_alteracao_em_lote.v1",
            &mod_estoque::AplicarAlteracaoEmLote {
                descricao: "Comissão dos refrigerantes".into(),
                produtos: vec![a, b],
                alteracao: Alteracao {
                    comissao: Mudanca::Definir(Some(Percentual::pontos(3))),
                    ..Alteracao::default()
                },
            },
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(r.alterados, 2);
    for p in [a, b] {
        let s = c.preco(p);
        assert_eq!(
            (s.comissao, s.origem_comissao),
            (Percentual::pontos(3), OrigemComissao::Produto)
        );
    }

    c.cmd(
        "estoque.desfazer_alteracao_em_lote.v1",
        &mod_estoque::DesfazerAlteracaoEmLote { lote: r.lote },
    )
    .unwrap();
    for p in [a, b] {
        assert_eq!(c.preco(p).origem_comissao, OrigemComissao::Padrao);
    }
}

/// Uma venda de `reais` no Razão, no dia `data` (D Caixa / C Receita de vendas).
fn venda_no_razao(arm: &Armazenamento, empresa: Id, data: cardeal_kernel::Data, reais: i64) {
    use cardeal_ledger::{ConstrutorLancamento, Origem, Razao, RepositorioRazao};
    arm.escritor()
        .executar(
            ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo()),
            move |uow| {
                let erro = |e: String| ErroArmazenamento::Sqlite(e);
                let conta = |uow: &mut cardeal_storage::UnidadeDeTrabalho<'_>, codigo: &str| {
                    uow.conexao()
                        .query_row(
                            "SELECT id FROM razao_conta WHERE empresa = ?1 AND codigo = ?2",
                            rusqlite::params![empresa.em_bytes().as_slice(), codigo],
                            |r| r.get::<_, Vec<u8>>(0),
                        )
                        .map(|b| Id::de_bytes(b.as_slice().try_into().unwrap()))
                        .map_err(|e| erro(e.to_string()))
                };
                let (caixa, receita) = (conta(uow, "1.1.01")?, conta(uow, "4.1")?);
                let l = ConstrutorLancamento::novo(empresa, data, "Venda")
                    .origem(Origem::modulo("teste").tipo("manual"))
                    .criado_por(Id::novo(), Id::novo())
                    .agora(Instante::EPOCA)
                    .liquidacao(data)
                    .debitar(caixa, Dinheiro::reais(reais))
                    .creditar(receita, Dinheiro::reais(reais))
                    .construir()
                    .map_err(|e| erro(e.to_string()))?;
                Razao::registrar(&mut RepositorioRazao::novo(uow), l)
                    .map_err(|e| erro(e.to_string()))?;
                Ok(())
            },
        )
        .unwrap();
}

#[test]
fn o_resumo_do_simples_e_o_preco_usam_o_mesmo_rbt12_da_lei() {
    use mod_estoque::simples::{
        AvisoDoSimples, DefinirHistoricoDoSimples, HistoricoDoSimples, OrigemDoRbt12,
        ResumoDoSimples, ResumoDoSimplesDoMes,
    };
    let c = Cenario::novo();
    let hoje = cardeal_kernel::Data::hoje(cardeal_kernel::Fuso::BRASILIA);
    let mes = hoje.competencia();
    // Três meses de R$ 30 mil antes deste, e uma venda neste mês (que não entra no RBT12).
    for meses_atras in 1..=3 {
        venda_no_razao(
            &c.arm,
            c.empresa,
            mes.primeiro_dia().mais_meses(-meses_atras),
            30_000,
        );
    }
    venda_no_razao(&c.arm, c.empresa, hoje, 50_000);

    let r: ResumoDoSimples = c.consulta(
        "estoque.resumo_do_simples.v1",
        &ResumoDoSimplesDoMes { mes },
    );
    assert_eq!(r.mes.receita, Dinheiro::reais(50_000));
    assert_eq!(r.mes.rbt12, Dinheiro::reais(90_000));
    assert_eq!(r.mes.origem, OrigemDoRbt12::HistoricoCurto { meses: 3 });
    assert_eq!(r.avisos, vec![AvisoDoSimples::HistoricoCurto { meses: 3 }]);
    // Anexo I, 1ª faixa: 4%, sem parcela a deduzir — o DAS do mês é R$ 2.000.
    assert_eq!(r.mes.aliquota, Some(Percentual::pontos(4)));
    assert_eq!(r.mes.das, Some(Dinheiro::reais(2_000)));

    // A empresa é antiga: a receita de antes do Cardeal entra no RBT12 (R$ 25 mil × 9 meses).
    let anteriores = (4..=12)
        .map(|n| {
            let mut m = mes;
            for _ in 0..n {
                m = m.anterior();
            }
            (m, Dinheiro::reais(25_000))
        })
        .collect();
    c.cmd(
        "estoque.definir_historico_do_simples.v1",
        &DefinirHistoricoDoSimples {
            historico: HistoricoDoSimples {
                inicio_atividade: None,
                receita_anterior: anteriores,
            },
        },
    )
    .unwrap();
    let r: ResumoDoSimples = c.consulta(
        "estoque.resumo_do_simples.v1",
        &ResumoDoSimplesDoMes { mes },
    );
    assert_eq!(r.mes.rbt12, Dinheiro::reais(315_000));
    assert_eq!(r.mes.origem, OrigemDoRbt12::DozeMeses);
    assert!(r.avisos.is_empty());
    assert_eq!(r.faixa, Some(2));

    // O preço sugerido usa o mesmo RBT12 e a mesma alíquota efetiva.
    let g = c.grupo("REV", None);
    let p = c.produto_com_custo("Refrigerante", g, Preco::centavos(400));
    let sugestao = c.preco(p);
    assert_eq!(sugestao.rbt12, r.mes.rbt12);
    assert_eq!(Some(sugestao.imposto), r.mes.aliquota);
}

#[test]
fn preco_no_grupo_usa_a_margem_e_a_comissao_herdadas() {
    // A peça comprada sob demanda ainda não existe no cadastro: só a categoria e o custo.
    let c = Cenario::novo();
    c.regime_manual(8);
    let pecas = c.grupo("PECAS", None);
    let celular = c.grupo("CEL", Some(pecas));
    c.margem_do_grupo(pecas, 15);
    // Um produto do mesmo grupo, com o mesmo custo, é a referência: o preço tem de bater.
    let p = c.produto_com_custo("Tela A52", celular, Preco::reais(15));

    let s: SugestaoDePreco = c.consulta(
        "estoque.preco_sugerido_no_grupo.v1",
        &PrecoSugeridoNoGrupo {
            grupo: celular,
            custo: Dinheiro::reais(15),
        },
    );
    assert!(s.produto.e_nulo(), "não há produto");
    assert_eq!(s.grupo, celular);
    assert_eq!(s.origem_margem, OrigemMargem::Grupo("PECAS".into()));
    // opex 25 + margem 15 + imposto 8 = 48 → 15 / 0,52 = 28,85
    assert_eq!(s.sugerido.unwrap().preco, Dinheiro::centavos(2885));
    assert_eq!(
        s.sugerido.unwrap().preco,
        c.preco(p).sugerido.unwrap().preco
    );
}

#[test]
fn preco_no_grupo_sem_margem_diz_o_que_falta() {
    let c = Cenario::novo();
    let g = c.grupo("PECAS", None);
    let s: SugestaoDePreco = c.consulta(
        "estoque.preco_sugerido_no_grupo.v1",
        &PrecoSugeridoNoGrupo {
            grupo: g,
            custo: Dinheiro::reais(15),
        },
    );
    assert!(s.sugerido.is_none());
    assert!(s.problema.unwrap().contains("margem"));
}

impl Cenario {
    fn fiscal_do_grupo(&self, grupo: Id, campos: CamposFiscais) {
        self.cmd(
            "estoque.definir_campos_fiscais_do_grupo.v1",
            &DefinirCamposFiscaisDoGrupo { grupo, campos },
        )
        .unwrap();
    }

    fn marca_st(&self, grupo: Id, st: bool) {
        self.cmd(
            "estoque.definir_perfil_fiscal_do_grupo.v1",
            &DefinirPerfilFiscalDoGrupo {
                grupo,
                icms_st: Some(st),
                monofasico: None,
            },
        )
        .unwrap();
    }
}

#[test]
fn marca_antiga_sem_cst_continua_valendo() {
    let c = Cenario::novo();
    let g = c.grupo("BEB", None);
    let p = c.produto_com_custo("Cerveja", g, Preco::reais(5));
    c.marca_st(g, true);
    assert!(c.preco(p).perfil_fiscal.icms_st);
}

#[test]
fn marca_e_cst_discordando_vale_o_cst_e_vai_para_conferir() {
    let c = Cenario::novo();
    let g = c.grupo("BEB", None);
    let p = c.produto_com_custo("Cerveja", g, Preco::reais(5));
    c.marca_st(g, true);
    c.fiscal_do_grupo(
        g,
        CamposFiscais {
            cst_icms: Some("102".into()),
            ..CamposFiscais::default()
        },
    );
    // A nota emitida usa o CSOSN 102 (sem ST): o preço também.
    assert!(!c.preco(p).perfil_fiscal.icms_st);
    let conflitos: Vec<mod_estoque::TributacaoAConferir> = c.consulta(
        "estoque.tributacao_a_conferir.v1",
        &mod_estoque::ListarTributacaoAConferir,
    );
    assert_eq!(conflitos.len(), 1);
    assert!(
        conflitos[0].motivo.contains("ST"),
        "{}",
        conflitos[0].motivo
    );
    // E o CST que diz ST liga a marca mesmo sem ela.
    c.fiscal_do_grupo(
        g,
        CamposFiscais {
            cst_icms: Some("500".into()),
            ..CamposFiscais::default()
        },
    );
    assert!(c.preco(p).perfil_fiscal.icms_st);
}

#[test]
fn fcp_e_ipi_herdam_do_grupo() {
    let grupo = CamposFiscais {
        aliquota_fcp: Some(Percentual::pontos(2)),
        aliquota_ipi: Some(Percentual::pontos(10)),
        ..CamposFiscais::default()
    };
    let efetivo = mod_estoque::fiscal::perfil_do_produto(&CamposFiscais::default(), &[&grupo]);
    assert_eq!(efetivo.aliquota_fcp, Some(Percentual::pontos(2)));
    let t = efetivo.perfil_tributario(Some("33049990"));
    assert_eq!(t.aliquota_ipi, Some(Percentual::pontos(10)));
    assert_eq!(t.ncm.as_deref(), Some("33049990"));
}

#[test]
fn perfil_gravado_antes_do_fcp_continua_legivel() {
    let antigo = mod_estoque::fiscal::CamposFiscaisAte2026Nov {
        cst_icms: Some("500".into()),
        aliquota_icms: Some(Percentual::pontos(17)),
        ..Default::default()
    };
    let blob = postcard::to_stdvec(&antigo).unwrap();
    let lido = CamposFiscais::de_blob(Some(&blob));
    assert_eq!(lido.cst_icms.as_deref(), Some("500"));
    assert_eq!(lido.aliquota_icms, Some(Percentual::pontos(17)));
    assert_eq!(lido.aliquota_fcp, None);
}

// ── preço pela carga real (motor cardeal-tributos) ─────────────────────────────────────────

fn lucro_ok(s: &SugestaoDePreco, margem: i64) -> bool {
    let r = s.resolvido.as_ref().expect("preço resolvido");
    let alvo = r.preco.aplicar(
        Percentual::pontos(margem),
        cardeal_kernel::Arredondamento::MeioAcima,
    );
    (r.lucro.em_centavos() - alvo.em_centavos()).abs() <= 1
}

fn csosn(c: &str) -> CamposFiscais {
    CamposFiscais {
        cst_icms: Some(c.into()),
        ..CamposFiscais::default()
    }
}

#[test]
fn criterio_1_simples_st_e_comum_mesmo_lucro() {
    let c = Cenario::novo();
    c.parametros_manuais(10);
    let comum = c.grupo("COMUM", None);
    let st = c.grupo("ST", None);
    for g in [comum, st] {
        c.margem_do_grupo(g, 15);
    }
    c.fiscal_do_grupo(comum, csosn("102"));
    c.fiscal_do_grupo(st, csosn("500"));
    let a = c.preco(c.produto_com_custo("Biscoito", comum, Preco::reais(15)));
    let b = c.preco(c.produto_com_custo("Cerveja", st, Preco::reais(15)));
    assert!(a.problema.is_none() && b.problema.is_none());
    assert!(
        a.sugerido.unwrap().preco > b.sugerido.unwrap().preco,
        "quem paga mais imposto sai mais caro"
    );
    assert!(lucro_ok(&a, 15) && lucro_ok(&b, 15));
    // O preço do markup e o resolvido são o mesmo número.
    assert_eq!(
        a.sugerido.unwrap().preco,
        a.resolvido.as_ref().unwrap().preco
    );
}

#[test]
fn criterio_2_presumido_icms_17() {
    let c = Cenario::novo();
    c.regime_cadastral("LucroPresumido");
    c.regime_manual(10);
    let g = c.grupo("ELE", None);
    c.margem_do_grupo(g, 15);
    c.fiscal_do_grupo(
        g,
        CamposFiscais {
            aliquota_icms: Some(Percentual::pontos(17)),
            ..csosn("00")
        },
    );
    let s = c.preco(c.produto_com_custo("Cabo", g, Preco::reais(15)));
    assert!(s.problema.is_none(), "{:?}", s.problema);
    assert!(lucro_ok(&s, 15));
    let rotulos: Vec<&str> = s
        .resolvido
        .as_ref()
        .unwrap()
        .parcelas
        .iter()
        .map(|p| p.rotulo.as_str())
        .collect();
    for imposto in ["ICMS", "PIS", "COFINS", "IRPJ", "CSLL"] {
        assert!(rotulos.contains(&imposto), "{imposto} em {rotulos:?}");
    }
}

#[test]
fn regime_vem_do_cadastro_da_empresa() {
    let c = Cenario::novo();
    let empresa = c.empresa;
    c.arm
        .escritor()
        .executar(
            ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo()),
            move |uow| {
                uow.conexao()
                    .execute(
                        "UPDATE nucleo_empresa SET regime = 'LucroReal' WHERE id = ?1",
                        [empresa.em_bytes().as_slice()],
                    )
                    .map(|_| ())
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            },
        )
        .unwrap();
    c.regime_manual(10);
    let g = c.grupo("ELE", None);
    c.margem_do_grupo(g, 15);
    c.fiscal_do_grupo(
        g,
        CamposFiscais {
            aliquota_icms: Some(Percentual::pontos(17)),
            ..csosn("00")
        },
    );
    let s = c.preco(c.produto_com_custo("Cabo", g, Preco::reais(15)));
    // Lucro Real: o IR é sobre o lucro, e o que sobra depois dele é a margem pedida.
    assert!(lucro_ok(&s, 15));
    let ir = s
        .resolvido
        .as_ref()
        .unwrap()
        .parcelas
        .iter()
        .find(|p| p.rotulo == "IRPJ");
    assert!(ir.unwrap().explicacao.contains("Lucro Real"));
}

#[test]
fn simples_com_cst_do_normal_diz_o_que_corrigir() {
    let c = Cenario::novo();
    c.parametros_manuais(10);
    let g = c.grupo("ELE", None);
    c.margem_do_grupo(g, 15);
    c.fiscal_do_grupo(g, csosn("00"));
    let s = c.preco(c.produto_com_custo("Cabo", g, Preco::reais(15)));
    assert!(s.sugerido.is_none());
    assert!(s.problema.unwrap().contains("CSOSN"));
}

// ── tributação a conferir: confirmar e desfazer ─────────────────────────────────────────────

fn perfil_proprio(c: &Cenario, produto: Id) -> CamposFiscais {
    let b: Option<Vec<u8>> = c
        .arm
        .leitor()
        .consultar(move |k| {
            k.query_row(
                "SELECT perfil_fiscal FROM estoque_produto WHERE id = ?1",
                [produto.em_bytes().as_slice()],
                |r| r.get(0),
            )
            .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
        })
        .unwrap();
    CamposFiscais::de_blob(b.as_deref())
}

fn aprender_st(c: &Cenario, produto: Id) {
    let org = c.empresa;
    c.arm
        .escritor()
        .executar(
            ContextoEscrita::novo(org, Id::novo(), Id::novo(), Id::novo()),
            move |uow| {
                mod_estoque::aprender_tributacao_do_produto(
                    uow.conexao(),
                    org,
                    produto,
                    &CamposFiscais {
                        cst_icms: Some("500".into()),
                        ..CamposFiscais::default()
                    },
                    None,
                    "aprendido da nota de compra: ST",
                    "nota 1/1",
                    Instante::agora(),
                )
                .map(|_| ())
                .map_err(ErroArmazenamento::Dominio)
            },
        )
        .unwrap();
}

fn lista(c: &Cenario) -> Vec<mod_estoque::TributacaoAConferir> {
    c.consulta(
        "estoque.tributacao_a_conferir.v1",
        &mod_estoque::ListarTributacaoAConferir,
    )
}

#[test]
fn desfazer_volta_o_perfil_de_antes() {
    let c = Cenario::novo();
    let g = c.grupo("BEB", None);
    let p = c.produto_com_custo("Cerveja", g, Preco::reais(5));
    aprender_st(&c, p);
    assert_eq!(perfil_proprio(&c, p).cst_icms.as_deref(), Some("500"));
    let item = lista(&c)[0].id.expect("registro");
    c.cmd(
        "estoque.desfazer_tributacao_a_conferir.v1",
        &mod_estoque::DesfazerTributacaoAConferir { id: item },
    )
    .unwrap();
    assert_eq!(perfil_proprio(&c, p), CamposFiscais::default());
    assert!(lista(&c).is_empty());
}

#[test]
fn confirmar_tira_da_lista_e_mantem_o_perfil() {
    let c = Cenario::novo();
    let g = c.grupo("BEB", None);
    let p = c.produto_com_custo("Cerveja", g, Preco::reais(5));
    aprender_st(&c, p);
    let item = lista(&c)[0].id.expect("registro");
    c.cmd(
        "estoque.confirmar_tributacao_a_conferir.v1",
        &mod_estoque::ConfirmarTributacaoAConferir { id: item },
    )
    .unwrap();
    assert!(lista(&c).is_empty());
    assert_eq!(perfil_proprio(&c, p).cst_icms.as_deref(), Some("500"));
}

#[test]
fn simples_com_percentual_manual_continua_no_manual() {
    // O cadastro diz Simples (o que a nota usa): o percentual informado vale, nunca IRPJ/CSLL.
    let c = Cenario::novo();
    c.regime_manual(10);
    let g = c.grupo("ELE", None);
    c.margem_do_grupo(g, 15);
    c.fiscal_do_grupo(
        g,
        CamposFiscais {
            aliquota_icms: Some(Percentual::pontos(17)),
            ..csosn("102")
        },
    );
    let s = c.preco(c.produto_com_custo("Cabo", g, Preco::reais(15)));
    assert!(s.problema.is_none(), "{:?}", s.problema);
    assert_eq!(s.imposto, Percentual::pontos(10));
}
