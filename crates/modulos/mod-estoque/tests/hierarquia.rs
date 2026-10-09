//! A estrutura mercadológica (até 4 níveis) atravessando o `Despachante` real contra SQLite.

#![allow(clippy::result_large_err)] // `ErroArmazenamento` carrega detalhes de propósito

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel as PapelAuth, Sessao};
use cardeal_kernel::{CodigoErro, Erro, Id, Instante};
use cardeal_ledger::semear_plano_padrao;
use cardeal_modkit::{Ambiente, Despachante, Modulo, PedidoAtivacao, RegistroModulos};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use mod_estoque::{
    ArvoreGruposProduto, CriarGrupoProduto, CriarProduto, CriarUnidade, GrupoProdutoCriado,
    ModuloEstoque, MoverGrupoProduto, MoverProdutosDeGrupo, NoArvoreGrupo, Produto, ProdutoCriado,
    ProdutoPorId, ProdutosMovidos, RenomearGrupoProduto, UnidadeCriada, MANIFESTO,
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

    fn comando<C: serde::Serialize>(&self, nome: &str, c: &C) -> Result<Vec<u8>, Erro> {
        self.d.executar_comando(
            nome,
            &carga(c),
            &self.s,
            &ambiente(self.empresa),
            self.arm.escritor(),
        )
    }

    fn grupo(&self, codigo: &str, pai: Option<Id>) -> Result<Id, Erro> {
        let saida = self.comando(
            "estoque.criar_grupo_produto.v1",
            &CriarGrupoProduto {
                codigo: codigo.to_owned(),
                nome: codigo.to_owned(),
                pai,
            },
        )?;
        Ok(postcard::from_bytes::<GrupoProdutoCriado>(&saida)
            .unwrap()
            .grupo_produto)
    }

    fn arvore(&self) -> Vec<NoArvoreGrupo> {
        let saida = self
            .d
            .executar_consulta(
                "estoque.arvore_grupos_produto.v1",
                &carga(&ArvoreGruposProduto),
                &self.s,
                &ambiente(self.empresa),
                self.arm.leitor(),
            )
            .unwrap();
        postcard::from_bytes(&saida).unwrap()
    }

    fn produto(&self, nome: &str, grupo: Id) -> Id {
        let un: UnidadeCriada = postcard::from_bytes(
            &self
                .comando(
                    "estoque.criar_unidade.v1",
                    &CriarUnidade {
                        sigla: format!("U{}", nome.len()),
                        nome: "Unidade".to_owned(),
                        fracionavel: false,
                    },
                )
                .unwrap_or_else(|_| {
                    // Unidade já existe: cria outra com sigla única.
                    self.comando(
                        "estoque.criar_unidade.v1",
                        &CriarUnidade {
                            sigla: nome.chars().take(3).collect(),
                            nome: "Outra".to_owned(),
                            fracionavel: false,
                        },
                    )
                    .unwrap()
                }),
        )
        .unwrap();
        let p: ProdutoCriado = postcard::from_bytes(
            &self
                .comando(
                    "estoque.criar_produto.v1",
                    &CriarProduto {
                        grupo_produto: grupo,
                        nome: nome.to_owned(),
                        ncm: "22021000".to_owned(),
                        unidade_padrao: un.unidade,
                        codigo_barras: None,
                        detalhes_tecnicos: None,
                    },
                )
                .unwrap(),
        )
        .unwrap();
        p.produto
    }
}

fn recusado(r: Result<Vec<u8>, Erro>) -> String {
    match r {
        Err(e) => {
            assert_eq!(e.codigo, CodigoErro::REGRA_VIOLADA);
            e.mensagem
        }
        outro => panic!("esperava recusa de regra, veio {outro:?}"),
    }
}

#[test]
fn arvore_de_quatro_niveis_vem_em_pre_ordem_com_niveis_e_contagens() {
    let c = Cenario::novo();
    let bebidas = c.grupo("BEB", None).unwrap();
    let refri = c.grupo("REF", Some(bebidas)).unwrap();
    let pet = c.grupo("PET", Some(refri)).unwrap();
    let dois_l = c.grupo("2L", Some(pet)).unwrap();
    let alimentos = c.grupo("ALI", None).unwrap();

    c.produto("Coca 2L", dois_l);
    c.produto("Suco avulso", bebidas);

    let a = c.arvore();
    let codigos: Vec<&str> = a.iter().map(|n| n.codigo.as_str()).collect();
    // Pré-ordem, irmãos por nome: ALI, BEB › REF › PET › 2L.
    assert_eq!(codigos, ["ALI", "BEB", "REF", "PET", "2L"]);
    let por_codigo = |c: &str| a.iter().find(|n| n.codigo == c).unwrap();
    assert_eq!(por_codigo("BEB").nivel, 1);
    assert_eq!(por_codigo("2L").nivel, 4);
    assert_eq!(por_codigo("BEB").produtos_diretos, 1);
    assert_eq!(por_codigo("BEB").produtos_na_subarvore, 2);
    assert_eq!(por_codigo("PET").produtos_na_subarvore, 1);
    assert_eq!(por_codigo("ALI").produtos_na_subarvore, 0);
    assert_eq!(por_codigo("ALI").id, alimentos);
}

#[test]
fn quinto_nivel_e_recusado_com_mensagem_clara() {
    let c = Cenario::novo();
    let n1 = c.grupo("N1", None).unwrap();
    let n2 = c.grupo("N2", Some(n1)).unwrap();
    let n3 = c.grupo("N3", Some(n2)).unwrap();
    let n4 = c.grupo("N4", Some(n3)).unwrap();
    let msg = recusado(c.grupo("N5", Some(n4)).map(|_| Vec::new()));
    assert!(msg.contains("no máximo 4 níveis"), "{msg}");
    assert_eq!(c.arvore().len(), 4, "o grupo recusado não foi gravado");
}

#[test]
fn mover_grupo_respeita_profundidade_e_ciclo() {
    let c = Cenario::novo();
    let a = c.grupo("A", None).unwrap();
    let b = c.grupo("B", Some(a)).unwrap();
    let cc = c.grupo("C", Some(b)).unwrap();
    let x = c.grupo("X", None).unwrap();
    let y = c.grupo("Y", Some(x)).unwrap();

    // A (altura 3) sob Y (nível 2) → nível 5: recusa.
    recusado(c.comando(
        "estoque.mover_grupo_produto.v1",
        &MoverGrupoProduto {
            grupo: a,
            novo_pai: Some(y),
        },
    ));
    // A sob o próprio descendente: ciclo.
    let msg = recusado(c.comando(
        "estoque.mover_grupo_produto.v1",
        &MoverGrupoProduto {
            grupo: a,
            novo_pai: Some(cc),
        },
    ));
    assert!(msg.contains("dentro de si mesmo"), "{msg}");
    // B (altura 2) sob Y (nível 2) → nível 4: cabe, e leva o C junto.
    c.comando(
        "estoque.mover_grupo_produto.v1",
        &MoverGrupoProduto {
            grupo: b,
            novo_pai: Some(y),
        },
    )
    .unwrap();
    let arv = c.arvore();
    let nivel = |id: Id| arv.iter().find(|n| n.id == id).unwrap().nivel;
    assert_eq!((nivel(b), nivel(cc)), (3, 4));
    // Promover à raiz.
    c.comando(
        "estoque.mover_grupo_produto.v1",
        &MoverGrupoProduto {
            grupo: cc,
            novo_pai: None,
        },
    )
    .unwrap();
    let arv = c.arvore();
    assert_eq!(arv.iter().find(|n| n.id == cc).unwrap().nivel, 1);
}

#[test]
fn renomear_troca_nome_e_codigo_e_recusa_vazio() {
    let c = Cenario::novo();
    let g = c.grupo("OLD", None).unwrap();
    c.comando(
        "estoque.renomear_grupo_produto.v1",
        &RenomearGrupoProduto {
            grupo: g,
            codigo: "NOVO".to_owned(),
            nome: "Nome novo".to_owned(),
        },
    )
    .unwrap();
    let n = &c.arvore()[0];
    assert_eq!((n.codigo.as_str(), n.nome.as_str()), ("NOVO", "Nome novo"));
    assert!(c
        .comando(
            "estoque.renomear_grupo_produto.v1",
            &RenomearGrupoProduto {
                grupo: g,
                codigo: " ".to_owned(),
                nome: "x".to_owned(),
            },
        )
        .is_err());
}

#[test]
fn mover_produtos_de_grupo_em_lote_e_tudo_ou_nada() {
    let c = Cenario::novo();
    let a = c.grupo("A", None).unwrap();
    let b = c.grupo("B", None).unwrap();
    let p1 = c.produto("Produto um", a);
    let p2 = c.produto("Produto dois", a);

    let saida = c
        .comando(
            "estoque.mover_produtos_de_grupo.v1",
            &MoverProdutosDeGrupo {
                produtos: vec![p1, p2],
                grupo: b,
            },
        )
        .unwrap();
    assert_eq!(
        postcard::from_bytes::<ProdutosMovidos>(&saida)
            .unwrap()
            .movidos,
        2
    );
    let arv = c.arvore();
    assert_eq!(arv.iter().find(|n| n.id == b).unwrap().produtos_diretos, 2);

    // Um produto inexistente no meio desfaz o lote inteiro.
    assert!(c
        .comando(
            "estoque.mover_produtos_de_grupo.v1",
            &MoverProdutosDeGrupo {
                produtos: vec![p1, Id::novo()],
                grupo: a,
            },
        )
        .is_err());
    let saida =
        c.d.executar_consulta(
            "estoque.produto_por_id.v1",
            &carga(&ProdutoPorId { produto: p1 }),
            &c.s,
            &ambiente(c.empresa),
            c.arm.leitor(),
        )
        .unwrap();
    let p: Option<Produto> = postcard::from_bytes(&saida).unwrap();
    assert_eq!(p.unwrap().grupo_produto, b);
}
