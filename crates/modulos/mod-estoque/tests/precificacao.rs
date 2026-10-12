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
    ParametrosPreco, PrecoSugeridoDoProduto, PrecosSugeridosDoGrupo, ProdutoCriado,
    RegistrarEntrada, SugestaoDePreco, TipoLocal, UnidadeCriada, MANIFESTO,
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
    c.regime_manual(10);
    let g = c.grupo("ELE", None);
    c.margem_do_grupo(g, 20);
    let p = c.produto_com_custo("Cabo", g, Preco::reais(50));

    // Sem campos fiscais: o percentual manual (10%). 25 + 20 + 10 = 55 → 50 / 0,45 = 111,11.
    let s = c.preco(p);
    assert_eq!(s.imposto, Percentual::pontos(10));
    assert_eq!(s.imposto_origem, OrigemDaCarga::Regime);
    assert_eq!(s.sugerido.unwrap().preco, Dinheiro::centavos(11111));

    // O grupo define ICMS 18% + PIS 0,65% + COFINS 3% = 21,65%: o produto herda.
    // 25 + 20 + 21,65 = 66,65 → 50 / 0,3335 = 149,93.
    c.cmd(
        "estoque.definir_campos_fiscais_do_grupo.v1",
        &DefinirCamposFiscaisDoGrupo {
            grupo: g,
            campos: icms_pis_cofins(Some(18), Some(650_000), Some(3)),
        },
    )
    .unwrap();
    let s = c.preco(p);
    assert_eq!(s.imposto, Percentual::unidades(21_650_000));
    assert_eq!(s.imposto_origem, OrigemDaCarga::PerfilFiscalDoProduto);
    assert_eq!(s.sugerido.unwrap().preco, Dinheiro::centavos(14993));

    // O produto sobrescreve só o ICMS (12%); PIS/COFINS continuam do grupo: 15,65%.
    // 25 + 20 + 15,65 = 60,65 → 50 / 0,3935 = 127,06.
    c.cmd(
        "estoque.definir_campos_fiscais_do_produto.v1",
        &DefinirCamposFiscaisDoProduto {
            produto: p,
            campos: icms_pis_cofins(Some(12), None, None),
        },
    )
    .unwrap();
    let s = c.preco(p);
    assert_eq!(s.imposto, Percentual::unidades(15_650_000));
    assert_eq!(s.sugerido.unwrap().preco, Dinheiro::centavos(12706));
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
