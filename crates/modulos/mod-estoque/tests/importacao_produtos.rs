//! Importação de produtos por planilha CSV atravessando o `Despachante` real contra SQLite:
//! grupos, unidades, produtos, margem e estoque inicial na mesma transação.

#![allow(clippy::result_large_err)] // `ErroArmazenamento` carrega detalhes de propósito

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel as PapelAuth, Sessao};
use cardeal_kernel::{CodigoErro, Erro, Id, Instante, Percentual, Preco, Quantidade};
use cardeal_ledger::semear_plano_padrao;
use cardeal_modkit::{Ambiente, Despachante, Modulo, PedidoAtivacao, RegistroModulos};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use mod_estoque::importacao_produtos::PreviaImportacaoProdutos;
use mod_estoque::{
    ArvoreGruposProduto, CriarLocal, ImportarProdutos, ItemProdutoComSaldo, ItemUnidade,
    LocalCriado, ModuloEstoque, NoArvoreGrupo, OrigemMargem, PrecoSugeridoDoProduto,
    PreviaDaImportacaoDeProdutos, Produto, ProdutoPorId, ProdutosComSaldo, ProdutosImportados,
    SugestaoDePreco, TipoLocal, Unidades, MANIFESTO,
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

const TODAS: &[&str] = &[
    "estoque.produto.criar",
    "estoque.produto.editar",
    "estoque.produto.ver",
    "estoque.movimento.entrada",
    "estoque.local.criar",
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
            s: sessao(empresa, TODAS),
        }
    }

    fn com_local(self, nome: &str) -> Self {
        let b = self
            .d
            .executar_comando(
                "estoque.criar_local.v1",
                &carga(&CriarLocal {
                    nome: nome.to_owned(),
                    tipo: TipoLocal::Loja,
                }),
                &self.s,
                &ambiente(self.empresa),
                self.arm.escritor(),
            )
            .unwrap();
        let _: LocalCriado = postcard::from_bytes(&b).unwrap();
        self
    }

    fn importar(&self, texto: &str) -> Result<ProdutosImportados, Erro> {
        self.d
            .executar_comando(
                "estoque.importar_produtos.v1",
                &carga(&ImportarProdutos {
                    texto: texto.to_owned(),
                }),
                &self.s,
                &ambiente(self.empresa),
                self.arm.escritor(),
            )
            .map(|b| postcard::from_bytes(&b).unwrap())
    }

    fn consultar<T: serde::de::DeserializeOwned>(
        &self,
        nome: &str,
        pedido: &impl serde::Serialize,
    ) -> T {
        let b = self
            .d
            .executar_consulta(
                nome,
                &carga(pedido),
                &self.s,
                &ambiente(self.empresa),
                self.arm.leitor(),
            )
            .unwrap();
        postcard::from_bytes(&b).unwrap()
    }

    fn previa(&self, texto: &str) -> PreviaImportacaoProdutos {
        self.consultar(
            "estoque.previa_importacao_produtos.v1",
            &PreviaDaImportacaoDeProdutos {
                texto: texto.to_owned(),
            },
        )
    }

    fn produtos(&self) -> Vec<ItemProdutoComSaldo> {
        self.consultar("estoque.produtos_com_saldo.v1", &ProdutosComSaldo)
    }

    fn produto(&self, nome: &str) -> (ItemProdutoComSaldo, Produto) {
        let item = self
            .produtos()
            .into_iter()
            .find(|p| p.nome == nome)
            .unwrap_or_else(|| panic!("{nome} não foi criado"));
        let completo: Option<Produto> = self.consultar(
            "estoque.produto_por_id.v1",
            &ProdutoPorId {
                produto: item.produto,
            },
        );
        (item, completo.unwrap())
    }

    fn arvore(&self) -> Vec<NoArvoreGrupo> {
        self.consultar("estoque.arvore_grupos_produto.v1", &ArvoreGruposProduto)
    }

    fn unidades(&self) -> Vec<ItemUnidade> {
        self.consultar("estoque.unidades.v1", &Unidades)
    }
}

const CSV: &str = "Nome;Grupo;Subgrupo;Unidade;EAN;NCM;Margem;Ponto de pedido;Estoque;Custo;Cor\n\
Leite em pó 400g;Mercearia;Leite;UN;7891000315507;0402.21.10;30%;5;12;18,90;branco\n\
Arroz 5kg;Mercearia;Grãos;PCT;;;;;;;\n\
Queijo prato;Frios;;KG;;;;;2,5;39,90;\n";

#[test]
fn importa_tudo_numa_transacao_e_a_segunda_vez_nao_cria_nada() {
    let c = Cenario::novo().com_local("Loja");

    let p = c.previa(CSV);
    assert!(p.erros.is_empty(), "{:?}", p.erros);
    assert_eq!(
        (p.novos, p.ja_existiam, p.grupos_novos, p.com_estoque),
        (3, 0, 4, 2)
    );
    assert_eq!(p.unidades_novas, ["UN", "PCT", "KG"]);
    assert_eq!(p.colunas_ignoradas, ["Cor"]);

    let r = c.importar(CSV).unwrap();
    assert_eq!(
        (
            r.criados,
            r.ja_existiam,
            r.grupos_criados,
            r.unidades_criadas,
            r.com_estoque
        ),
        (3, 0, 4, 3, 2)
    );

    // Árvore: Mercearia › Leite, Mercearia › Grãos, Frios.
    let arvore = c.arvore();
    assert_eq!(arvore.len(), 4);
    let mercearia = arvore.iter().find(|n| n.nome == "Mercearia").unwrap();
    let leite = arvore.iter().find(|n| n.nome == "Leite").unwrap();
    assert_eq!(leite.pai, Some(mercearia.id));

    // Produto completo: grupo, unidade, NCM, código de barras, ponto de pedido.
    let (item, leite_em_po) = c.produto("Leite em pó 400g");
    assert_eq!(leite_em_po.grupo_produto, leite.id);
    assert_eq!(leite_em_po.ncm, "04022110");
    assert_eq!(leite_em_po.codigo_barras.as_deref(), Some("7891000315507"));
    assert_eq!(
        leite_em_po.ponto_pedido,
        Some(Quantidade::de_str("5").unwrap())
    );
    let un = c.unidades().into_iter().find(|u| u.sigla == "UN").unwrap();
    assert_eq!(leite_em_po.unidade_padrao, un.id);

    // Estoque inicial com custo: entrou como entrada no único local.
    assert_eq!(item.disponivel, Quantidade::de_str("12").unwrap());
    assert_eq!(item.custo_medio, Preco::de_str("18,90").unwrap());
    let (queijo, _) = c.produto("Queijo prato");
    assert_eq!(queijo.disponivel, Quantidade::de_str("2,5").unwrap());
    let (arroz, _) = c.produto("Arroz 5kg");
    assert!(arroz.disponivel.e_zero());

    // A margem própria do produto vale para o preço sugerido.
    let sugestao: SugestaoDePreco = c.consultar(
        "estoque.preco_sugerido_do_produto.v1",
        &PrecoSugeridoDoProduto {
            produto: item.produto,
            custo: None,
        },
    );
    assert_eq!(sugestao.margem, Some(Percentual::pontos(30)));
    assert_eq!(sugestao.origem_margem, OrigemMargem::Produto);

    // Segunda vez: tudo já existe; nem produto nem estoque em dobro.
    let p = c.previa(CSV);
    assert_eq!((p.novos, p.ja_existiam, p.grupos_novos), (0, 3, 0));
    let r = c.importar(CSV).unwrap();
    assert_eq!((r.criados, r.ja_existiam, r.com_estoque), (0, 3, 0));
    assert_eq!(c.produtos().len(), 3);
    let (item, _) = c.produto("Leite em pó 400g");
    assert_eq!(item.disponivel, Quantidade::de_str("12").unwrap());
}

#[test]
fn qualquer_erro_de_linha_recusa_o_arquivo_inteiro() {
    let c = Cenario::novo().com_local("Loja");
    let ruim = "Nome;Grupo;EAN;Estoque;Custo\nBom;Novo grupo;;3;1\nRuim;;7891000315508;;\n";
    let p = c.previa(ruim);
    assert_eq!(p.erros.len(), 1);
    assert_eq!(p.erros[0].linha, 3);
    let e = c.importar(ruim).unwrap_err();
    assert_eq!(e.codigo, CodigoErro::REGRA_VIOLADA);
    assert!(e.mensagem.contains("linha 3"), "{}", e.mensagem);
    assert!(c.produtos().is_empty(), "não pode gravar parcialmente");
    assert!(c.arvore().is_empty(), "nem os grupos");
}

#[test]
fn produto_que_ja_existe_no_catalogo_fica_como_esta() {
    let c = Cenario::novo();
    c.importar("Nome;Grupo\nParafuso 3mm;Ferragens\n").unwrap();
    let r = c
        .importar("Nome;Grupo;Margem\nparafuso 3MM;Outro grupo;50\nPorca 3mm;Ferragens;\n")
        .unwrap();
    assert_eq!((r.criados, r.ja_existiam, r.grupos_criados), (1, 1, 0));
    let (_, parafuso) = c.produto("Parafuso 3mm");
    let ferragens = c
        .arvore()
        .into_iter()
        .find(|n| n.nome == "Ferragens")
        .unwrap();
    assert_eq!(parafuso.grupo_produto, ferragens.id, "não foi movido");
}

#[test]
fn estoque_sem_local_cadastrado_e_erro_de_linha() {
    let c = Cenario::novo();
    let e = c.importar("Nome;Estoque;Custo\nA;5;2\n").unwrap_err();
    assert_eq!(e.codigo, CodigoErro::REGRA_VIOLADA);
    assert!(e.mensagem.contains("local"), "{}", e.mensagem);
}

#[test]
fn a_importacao_nao_e_atalho_de_permissao() {
    let mut c = Cenario::novo().com_local("Loja");

    c.s = sessao(c.empresa, &["estoque.produto.ver"]);
    let e = c.importar("Nome\nA\n").unwrap_err();
    assert_eq!(e.codigo, CodigoErro::SEM_PERMISSAO);

    // Criar pode; margem exige editar, estoque exige registrar entrada.
    c.s = sessao(c.empresa, &["estoque.produto.criar", "estoque.produto.ver"]);
    let e = c.importar("Nome;Margem\nA;20\n").unwrap_err();
    assert_eq!(e.codigo, CodigoErro::SEM_PERMISSAO);
    let e = c.importar("Nome;Estoque;Custo\nA;1;1\n").unwrap_err();
    assert_eq!(e.codigo, CodigoErro::SEM_PERMISSAO);
    assert!(c.produtos().is_empty());

    let r = c.importar("Nome\nA\n").unwrap();
    assert_eq!(r.criados, 1);
}
