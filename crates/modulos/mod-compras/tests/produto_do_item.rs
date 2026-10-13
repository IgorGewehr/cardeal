//! Produto novo a partir do item de nota que não casou (lunno §5.6): o código de barras do XML
//! fica guardado no item, o nome sugerido é padronizado pelo dicionário, a correção do usuário
//! ensina o dicionário, e a próxima nota do mesmo item casa sozinha.

#![allow(clippy::result_large_err)] // `ErroArmazenamento` carrega detalhes de propósito

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Sessao};
use cardeal_kernel::{CodigoErro, Erro, Id, Instante};
use cardeal_ledger::semear_plano_padrao;
use cardeal_modkit::{Ambiente, Despachante, Modulo, PedidoAtivacao, RegistroModulos};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use mod_compras::{
    CriarProdutoDoItem, EstadoCasamento, ImportarNotaDeArquivoXml, ItemNotaEntrada, ItensDaNota,
    ModuloCompras, ProdutoDoItemCriado, RelatorioImportacao,
};
use mod_estoque::padronizacao::TermoDicionario;
use mod_estoque::{DicionarioDeNomes, ModuloEstoque, Produto, ProdutoPorId, SugerirNomeDoProduto};
use tempfile::TempDir;

const CNPJ_EMPRESA: &str = "11222333000181";
const CNPJ_FORNECEDOR: &str = "45543915000181";
const GTIN: &str = "7891000315507";

fn base() -> (TempDir, Armazenamento, Id) {
    let dir = tempfile::tempdir().unwrap();
    let arm =
        Armazenamento::abrir(ConfigArmazenamento::arquivo(dir.path().join("cardeal.db"))).unwrap();
    arm.migrar(&[
        cardeal_ledger::migracoes::conjunto(),
        mod_clientes::ModuloClientes.migracoes(),
        ModuloEstoque.migracoes(),
        mod_financeiro::ModuloFinanceiro.migracoes(),
        ModuloCompras.migracoes(),
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
                     VALUES (?1,'Teste LTDA','Teste',?2,'SimplesNacional','{}','comercio',0)",
                    rusqlite::params![empresa.em_bytes().as_slice(), CNPJ_EMPRESA],
                )
                .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?;
            semear_plano_padrao(uow, empresa).map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
        })
        .unwrap();
    (dir, arm, empresa)
}

fn xml(chave: &str, descricao: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<nfeProc xmlns="http://www.portalfiscal.inf.br/nfe">
  <NFe>
    <infNFe Id="NFe{chave}" versao="4.00">
      <ide><nNF>12</nNF><serie>1</serie><dhEmi>2026-10-01T09:00:00-03:00</dhEmi></ide>
      <emit><CNPJ>{CNPJ_FORNECEDOR}</CNPJ><xNome>Atacado Beta LTDA</xNome></emit>
      <det nItem="1">
        <prod>
          <cProd>FER-10</cProd>
          <cEAN>{GTIN}</cEAN>
          <xProd>{descricao}</xProd>
          <NCM>21021090</NCM>
          <qCom>24.0000</qCom>
          <vUnCom>1.500000</vUnCom>
          <vProd>36.00</vProd>
        </prod>
      </det>
      <total><ICMSTot><vProd>36.00</vProd><vFrete>0.00</vFrete><vSeg>0.00</vSeg>
        <vOutro>0.00</vOutro><vNF>36.00</vNF></ICMSTot></total>
    </infNFe>
  </NFe>
</nfeProc>"#
    )
}

fn sessao(empresa: Id, permissoes: &[&str]) -> Sessao {
    let mut papel = cardeal_auth::Papel::novo(empresa, "Testador");
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

const TODAS: &[&str] = &[
    "compras.entrada.importar",
    "compras.entrada.conferir",
    "compras.entrada.ver",
    "estoque.produto.criar",
    "estoque.produto.ver",
];

struct Cenario {
    _dir: TempDir,
    arm: Armazenamento,
    d: Despachante,
    s: Sessao,
    amb: Ambiente,
    empresa: Id,
}

impl Cenario {
    fn novo() -> Self {
        let (dir, arm, empresa) = base();
        let d = Despachante::construir(&[
            &mod_clientes::ModuloClientes,
            &ModuloEstoque,
            &mod_financeiro::ModuloFinanceiro,
            &ModuloCompras,
        ])
        .unwrap();
        let mut rm = RegistroModulos::novo();
        rm.registrar(&mod_clientes::MANIFESTO).unwrap();
        rm.registrar(&mod_estoque::MANIFESTO).unwrap();
        rm.registrar(&mod_financeiro::MANIFESTO).unwrap();
        rm.registrar(&mod_compras::MANIFESTO).unwrap();
        let efetivo = rm
            .resolver(
                &PedidoAtivacao::nova()
                    .com_modulo("estoque")
                    .com_modulo("compras"),
            )
            .unwrap();
        Self {
            _dir: dir,
            arm,
            d,
            s: sessao(empresa, TODAS),
            amb: Ambiente::novo(empresa, efetivo),
            empresa,
        }
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
                &self.amb,
                self.arm.escritor(),
            )
            .map(|b| postcard::from_bytes(&b).unwrap())
    }

    fn consulta<T: serde::de::DeserializeOwned>(&self, nome: &str, c: &impl serde::Serialize) -> T {
        let b = self
            .d
            .executar_consulta(
                nome,
                &postcard::to_stdvec(c).unwrap(),
                &self.s,
                &self.amb,
                self.arm.leitor(),
            )
            .unwrap();
        postcard::from_bytes(&b).unwrap()
    }

    fn importar(&self, chave: &str, descricao: &str) -> RelatorioImportacao {
        self.cmd(
            "compras.importar_nota_de_arquivo_xml.v1",
            &ImportarNotaDeArquivoXml {
                xml: xml(chave, descricao),
            },
        )
        .unwrap()
    }

    fn itens(&self, nota: Id) -> Vec<ItemNotaEntrada> {
        self.consulta("compras.itens_da_nota.v1", &ItensDaNota { nota })
    }

    fn sugestao(&self, descricao: &str) -> String {
        self.consulta(
            "estoque.sugerir_nome_do_produto.v1",
            &SugerirNomeDoProduto {
                descricao: descricao.into(),
            },
        )
    }
}

#[test]
fn produto_do_item_nasce_com_ncm_e_gtin_ensina_o_dicionario_e_a_proxima_nota_casa() {
    let c = Cenario::novo();
    let nf = "FERM BIOLOGICO SECO FLEISCHMANN 10G";
    let r = c.importar(&"1".repeat(44), nf);
    assert_eq!(r.itens_nao_casados, 1);
    let item = c.itens(r.nota_entrada).remove(0);
    assert_eq!(
        item.codigo_barras.as_deref(),
        Some(GTIN),
        "o cEAN fica no item"
    );

    assert_eq!(c.sugestao(nf), "Ferm Biologico Seco Fleischmann 10gr");
    let criado: ProdutoDoItemCriado = c
        .cmd(
            "compras.criar_produto_do_item.v1",
            &CriarProdutoDoItem {
                item_nota: item.id,
                nome: "Fermento Biológico Seco Fleischmann 10gr".into(),
                grupo: None,
            },
        )
        .unwrap();
    assert_eq!(criado.termos_aprendidos, 2);

    let produto: Option<Produto> = c.consulta(
        "estoque.produto_por_id.v1",
        &ProdutoPorId {
            produto: criado.produto,
        },
    );
    let produto = produto.unwrap();
    assert_eq!(produto.nome, "Fermento Biológico Seco Fleischmann 10gr");
    assert_eq!(produto.ncm, "21021090");
    assert_eq!(produto.codigo_barras.as_deref(), Some(GTIN));
    let item = c.itens(r.nota_entrada).remove(0);
    assert_eq!(item.estado_casamento, EstadoCasamento::Casado);
    assert_eq!(item.produto_casado, Some(criado.produto));

    // O dicionário aprendeu e a sugestão seguinte já sai com ele.
    let termos: Vec<TermoDicionario> =
        c.consulta("estoque.dicionario_de_nomes.v1", &DicionarioDeNomes);
    let pares: Vec<(&str, &str)> = termos
        .iter()
        .map(|t| (t.de.as_str(), t.para.as_str()))
        .collect();
    assert_eq!(pares, [("biologico", "Biológico"), ("ferm", "Fermento")]);
    assert_eq!(
        c.sugestao("FERM BIOLOGICO INSTANT FLEISCHMANN 125G"),
        "Fermento Biológico Instant Fleischmann 125gr"
    );

    // A próxima nota do mesmo item casa sozinha (código de barras).
    let r2 = c.importar(&"2".repeat(44), nf);
    assert_eq!(r2.itens_nao_casados, 0);
    assert_eq!(
        c.itens(r2.nota_entrada)[0].produto_casado,
        Some(criado.produto)
    );
}

#[test]
fn criar_produto_da_nota_exige_poder_cadastrar_produto() {
    let mut c = Cenario::novo();
    let r = c.importar(&"3".repeat(44), "ARROZ TIPO1 5KG");
    let item = c.itens(r.nota_entrada).remove(0);
    c.s = sessao(
        c.empresa,
        &[
            "compras.entrada.importar",
            "compras.entrada.conferir",
            "compras.entrada.ver",
        ],
    );
    let e = c
        .cmd::<ProdutoDoItemCriado>(
            "compras.criar_produto_do_item.v1",
            &CriarProdutoDoItem {
                item_nota: item.id,
                nome: "Arroz Tipo1 5Kg".into(),
                grupo: None,
            },
        )
        .unwrap_err();
    assert_eq!(e.codigo, CodigoErro::SEM_PERMISSAO);
}
