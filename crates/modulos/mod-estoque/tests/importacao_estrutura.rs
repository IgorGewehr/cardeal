//! Importação da estrutura por planilha CSV atravessando o `Despachante` real contra SQLite.

#![allow(clippy::result_large_err)] // `ErroArmazenamento` carrega detalhes de propósito

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel as PapelAuth, Sessao};
use cardeal_kernel::{CodigoErro, Erro, Id, Instante};
use cardeal_ledger::semear_plano_padrao;
use cardeal_modkit::{Ambiente, Despachante, Modulo, PedidoAtivacao, RegistroModulos};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use mod_estoque::importacao_estrutura::PreviaImportacao;
use mod_estoque::{
    ArvoreGruposProduto, EstruturaImportada, ImportarEstrutura, ModuloEstoque, NoArvoreGrupo,
    PreviaDaImportacaoDaEstrutura, MANIFESTO,
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

    fn importar(&self, texto: &str) -> Result<EstruturaImportada, Erro> {
        self.d
            .executar_comando(
                "estoque.importar_estrutura.v1",
                &carga(&ImportarEstrutura {
                    texto: texto.to_owned(),
                }),
                &self.s,
                &ambiente(self.empresa),
                self.arm.escritor(),
            )
            .map(|b| postcard::from_bytes(&b).unwrap())
    }

    fn previa(&self, texto: &str) -> PreviaImportacao {
        let b = self
            .d
            .executar_consulta(
                "estoque.previa_importacao_estrutura.v1",
                &carga(&PreviaDaImportacaoDaEstrutura {
                    texto: texto.to_owned(),
                }),
                &self.s,
                &ambiente(self.empresa),
                self.arm.leitor(),
            )
            .unwrap();
        postcard::from_bytes(&b).unwrap()
    }

    fn arvore(&self) -> Vec<NoArvoreGrupo> {
        let b = self
            .d
            .executar_consulta(
                "estoque.arvore_grupos_produto.v1",
                &carga(&ArvoreGruposProduto),
                &self.s,
                &ambiente(self.empresa),
                self.arm.leitor(),
            )
            .unwrap();
        postcard::from_bytes(&b).unwrap()
    }
}

const CSV: &str = "Grupo;Subgrupo;Categoria;Subcategoria\n\
Bebidas;Refrigerantes;Cola;Lata\n\
Bebidas;Sucos\n\
Peças;Telas\n";

#[test]
fn importa_cria_a_arvore_e_a_segunda_vez_nao_cria_nada() {
    let c = Cenario::novo();
    let previa = c.previa(CSV);
    assert_eq!(
        (previa.novos, previa.ja_existiam, previa.erros.len()),
        (7, 0, 0)
    );

    let r = c.importar(CSV).unwrap();
    assert_eq!((r.criados, r.ja_existiam), (7, 0));
    let arvore = c.arvore();
    assert_eq!(arvore.len(), 7);
    let lata = arvore.iter().find(|n| n.nome == "Lata").unwrap();
    assert_eq!(lata.nivel, 4);
    let cola = arvore.iter().find(|n| n.nome == "Cola").unwrap();
    assert_eq!(lata.pai, Some(cola.id));
    // Os códigos gerados são únicos.
    let mut codigos: Vec<_> = arvore.iter().map(|n| &n.codigo).collect();
    codigos.sort();
    codigos.dedup();
    assert_eq!(codigos.len(), 7);

    let previa = c.previa(CSV);
    assert_eq!((previa.novos, previa.ja_existiam), (0, 7));
    let r = c.importar(CSV).unwrap();
    assert_eq!((r.criados, r.ja_existiam), (0, 7));
    assert_eq!(c.arvore().len(), 7);
}

#[test]
fn acrescenta_sob_o_que_ja_existe_sem_acento_nem_caixa() {
    let c = Cenario::novo();
    c.importar("Peças;Telas\n").unwrap();
    let r = c.importar("PECAS;Baterias\nPecas;telas\n").unwrap();
    assert_eq!((r.criados, r.ja_existiam), (1, 2));
    assert_eq!(c.arvore().len(), 3);
}

#[test]
fn qualquer_erro_de_linha_recusa_o_arquivo_inteiro() {
    let c = Cenario::novo();
    let ruim = "Ok;Sub\nA;;C\n";
    let previa = c.previa(ruim);
    assert_eq!(previa.erros.len(), 1);
    assert_eq!(previa.erros[0].linha, 2);
    let e = c.importar(ruim).unwrap_err();
    assert_eq!(e.codigo, CodigoErro::REGRA_VIOLADA);
    assert!(c.arvore().is_empty(), "não pode gravar parcialmente");
}

#[test]
fn sem_permissao_de_criar_nao_importa() {
    let mut c = Cenario::novo();
    c.s = sessao(c.empresa, &["estoque.produto.ver"]);
    let e = c.importar("A\n").unwrap_err();
    assert_eq!(e.codigo, CodigoErro::SEM_PERMISSAO);
}
