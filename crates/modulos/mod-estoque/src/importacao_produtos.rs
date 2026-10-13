//! Importação de produtos por planilha (CSV): domínio puro.
//!
//! A primeira linha com conteúdo é o **cabeçalho**, e as colunas valem pelo nome, em qualquer
//! ordem (sem caixa nem acento: "Código de barras", "EAN" e "GTIN" são a mesma coluna). Só
//! `Nome` é obrigatória; as outras, quando vêm, preenchem o cadastro:
//!
//! - `Grupo;Subgrupo;Categoria;Subcategoria` — o caminho na estrutura mercadológica. O que
//!   ainda não existe é criado, com as mesmas regras da [importação da estrutura]
//!   (`crate::importacao_estrutura`). Sem grupo, o produto vai para "Sem grupo".
//! - `NCM`, `Código de barras` (GTIN validado), `Unidade` (sigla; padrão `UN`, e uma sigla
//!   nova vira unidade nova), `Margem` (a margem líquida desejada do produto), `Comissão` (a
//!   comissão de vendedor própria do produto).
//! - `Ponto de pedido`, `Estoque mínimo`, `Estoque máximo`.
//! - `Fabricante` (ou `Marca`), `Código do fabricante` (ou `MPN`/`Referência`), `Localização`.
//! - `Estoque` + `Custo` (+ `Local`): o **estoque inicial**, que entra como uma entrada de
//!   estoque com esse custo unitário. Sem `Local`, vale o único local da empresa.
//!
//! [`planejar`] compara a planilha com o que já existe e devolve o que criar, quantos já
//! existiam e os erros por linha; nada é gravado aqui. Um produto que **já existe** (mesmo
//! código de barras ou, sem ele, o mesmo nome) é mantido como está: a importação só cria, e
//! por isso importar o mesmo arquivo duas vezes não duplica nem o produto nem o estoque.
//! Quem aplica recusa o arquivo inteiro se houver um único erro.

use std::collections::{HashMap, HashSet};

use cardeal_kernel::texto::chave_busca;
use cardeal_kernel::{Id, Percentual, Preco, Quantidade};
use serde::{Deserialize, Serialize};

use crate::importacao_estrutura::{
    ler_linhas, validar_caminho, ErroDeLinha, NoExistente, NoNovo, PaiPlanejado,
    ResolvedorDeCaminhos,
};
use crate::produto::{normalizar_ncm, validar_gtin, DetalhesTecnicos};

/// Quantos produtos novos a prévia lista (o resto só conta).
pub const AMOSTRA_DA_PREVIA: usize = 200;

/// O grupo de quem vem sem grupo na planilha.
pub const GRUPO_SEM_GRUPO: &str = "Sem grupo";

/// A unidade de quem vem sem unidade na planilha.
pub const UNIDADE_PADRAO: &str = "UN";

/// Um produto que já está cadastrado.
#[derive(Debug, Clone)]
pub struct ProdutoExistente {
    /// O produto.
    pub id: Id,
    /// O nome.
    pub nome: String,
    /// O código de barras, quando tem.
    pub codigo_barras: Option<String>,
}

/// Uma unidade que já está cadastrada.
#[derive(Debug, Clone)]
pub struct UnidadeExistente {
    /// A unidade.
    pub id: Id,
    /// A sigla ("UN", "KG").
    pub sigla: String,
}

/// Um local de estoque da empresa.
#[derive(Debug, Clone)]
pub struct LocalExistente {
    /// O local.
    pub id: Id,
    /// O nome.
    pub nome: String,
}

/// O que já existe e a planilha precisa enxergar.
#[derive(Debug, Clone, Default)]
pub struct Existentes {
    /// A árvore da estrutura mercadológica.
    pub grupos: Vec<NoExistente>,
    /// O catálogo.
    pub produtos: Vec<ProdutoExistente>,
    /// As unidades.
    pub unidades: Vec<UnidadeExistente>,
    /// Os locais ativos da empresa (só pesam no estoque inicial).
    pub locais: Vec<LocalExistente>,
}

/// A unidade de um produto a criar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnidadePlanejada {
    /// Uma unidade que já existe.
    Existente(Id),
    /// Uma unidade do próprio plano (índice em [`PlanoProdutos::unidades`]).
    Nova(usize),
}

/// Uma unidade a criar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnidadeNova {
    /// A sigla, em maiúsculas.
    pub sigla: String,
    /// O nome por extenso (o conhecido para a sigla, ou a própria sigla).
    pub nome: String,
    /// Se aceita fração (peso, volume, comprimento).
    pub fracionavel: bool,
}

/// O estoque inicial de um produto: uma entrada com custo num local.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EstoqueInicial {
    /// O local que recebe.
    pub local: Id,
    /// A quantidade (positiva).
    pub quantidade: Quantidade,
    /// O custo unitário (positivo).
    pub custo_unitario: Preco,
}

/// Um produto a criar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProdutoNovo {
    /// A linha da planilha.
    pub linha: usize,
    /// O nome.
    pub nome: String,
    /// O grupo (existente ou do plano).
    pub grupo: PaiPlanejado,
    /// O caminho do grupo, para exibir ("Bebidas › Refrigerantes").
    pub caminho: String,
    /// A unidade.
    pub unidade: UnidadePlanejada,
    /// A sigla da unidade, para exibir.
    pub sigla_unidade: String,
    /// NCM só com dígitos; vazio = pendente.
    pub ncm: String,
    /// O código de barras validado.
    pub codigo_barras: Option<String>,
    /// A margem líquida desejada própria.
    pub margem: Option<Percentual>,
    /// A comissão de vendedor própria.
    pub comissao: Option<Percentual>,
    /// O ponto de pedido.
    pub ponto_pedido: Option<Quantidade>,
    /// O estoque mínimo.
    pub estoque_minimo: Option<Quantidade>,
    /// O estoque máximo.
    pub estoque_maximo: Option<Quantidade>,
    /// Fabricante, código do fabricante e localização física.
    pub detalhes: DetalhesTecnicos,
    /// O estoque inicial, quando a linha traz quantidade.
    pub estoque: Option<EstoqueInicial>,
}

/// O resultado da análise da planilha.
#[derive(Debug, Clone, Default)]
pub struct PlanoProdutos {
    /// Os grupos a criar, pais antes dos filhos.
    pub grupos: Vec<NoNovo>,
    /// As unidades a criar.
    pub unidades: Vec<UnidadeNova>,
    /// Os produtos a criar, na ordem da planilha.
    pub produtos: Vec<ProdutoNovo>,
    /// Quantos produtos da planilha já existiam (e ficam como estão).
    pub ja_existiam: u32,
    /// Quantas linhas de produto a planilha trouxe (sem o cabeçalho).
    pub linhas: u32,
    /// Linhas com custo mas sem estoque: o custo não tem onde ficar e é ignorado.
    pub custos_sem_estoque: u32,
    /// Os erros por linha; com qualquer um, nada é gravado.
    pub erros: Vec<ErroDeLinha>,
    /// Colunas do cabeçalho que a importação não conhece (ficam de fora).
    pub colunas_ignoradas: Vec<String>,
}

/// Uma linha da amostra da prévia.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinhaDaPrevia {
    /// O nome.
    pub nome: String,
    /// O caminho do grupo.
    pub grupo: String,
    /// A sigla da unidade.
    pub unidade: String,
    /// O estoque inicial, quando há.
    pub estoque: Option<Quantidade>,
    /// O custo do estoque inicial, quando há.
    pub custo: Option<Preco>,
}

/// O que a tela mostra antes de confirmar.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviaImportacaoProdutos {
    /// Quantos produtos serão criados.
    pub novos: u32,
    /// Quantos já existiam.
    pub ja_existiam: u32,
    /// Quantos grupos novos.
    pub grupos_novos: u32,
    /// Quantas unidades novas (siglas).
    pub unidades_novas: Vec<String>,
    /// Quantos produtos novos entram com estoque inicial.
    pub com_estoque: u32,
    /// Linhas com custo mas sem estoque (o custo é ignorado).
    pub custos_sem_estoque: u32,
    /// Quantas linhas de produto.
    pub linhas: u32,
    /// Os erros por linha.
    pub erros: Vec<ErroDeLinha>,
    /// As colunas que ficam de fora.
    pub colunas_ignoradas: Vec<String>,
    /// Os primeiros produtos a criar (até [`AMOSTRA_DA_PREVIA`]).
    pub amostra: Vec<LinhaDaPrevia>,
}

impl PlanoProdutos {
    /// O resumo para a tela.
    #[must_use]
    pub fn previa(&self) -> PreviaImportacaoProdutos {
        PreviaImportacaoProdutos {
            novos: contar(self.produtos.len()),
            ja_existiam: self.ja_existiam,
            grupos_novos: contar(self.grupos.len()),
            unidades_novas: self.unidades.iter().map(|u| u.sigla.clone()).collect(),
            com_estoque: contar(self.produtos.iter().filter(|p| p.estoque.is_some()).count()),
            custos_sem_estoque: self.custos_sem_estoque,
            linhas: self.linhas,
            erros: self.erros.clone(),
            colunas_ignoradas: self.colunas_ignoradas.clone(),
            amostra: self
                .produtos
                .iter()
                .take(AMOSTRA_DA_PREVIA)
                .map(|p| LinhaDaPrevia {
                    nome: p.nome.clone(),
                    grupo: p.caminho.clone(),
                    unidade: p.sigla_unidade.clone(),
                    estoque: p.estoque.map(|e| e.quantidade),
                    custo: p.estoque.map(|e| e.custo_unitario),
                })
                .collect(),
        }
    }

    /// Verdadeiro se algum produto novo traz margem, comissão, ponto de pedido ou estoque
    /// mínimo/máximo — parâmetros que fora da importação exigem a permissão de editar produto.
    #[must_use]
    pub fn define_parametros(&self) -> bool {
        self.produtos.iter().any(|p| {
            p.margem.is_some()
                || p.comissao.is_some()
                || p.ponto_pedido.is_some()
                || p.estoque_minimo.is_some()
                || p.estoque_maximo.is_some()
        })
    }

    /// Verdadeiro se algum produto novo entra com estoque inicial.
    #[must_use]
    pub fn tem_estoque(&self) -> bool {
        self.produtos.iter().any(|p| p.estoque.is_some())
    }
}

fn contar(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// As colunas que a importação entende.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Coluna {
    Nome,
    Grupo,
    Subgrupo,
    Categoria,
    Subcategoria,
    Ncm,
    CodigoBarras,
    Unidade,
    Margem,
    Comissao,
    PontoPedido,
    EstoqueMinimo,
    EstoqueMaximo,
    Fabricante,
    CodigoFabricante,
    Localizacao,
    Estoque,
    Custo,
    Local,
}

/// Os níveis da estrutura, na ordem do caminho.
const NIVEIS: [Coluna; 4] = [
    Coluna::Grupo,
    Coluna::Subgrupo,
    Coluna::Categoria,
    Coluna::Subcategoria,
];

/// Reconhece o título de uma coluna: sem caixa, acento, espaço nem pontuação.
fn coluna_de(titulo: &str) -> Option<Coluna> {
    let chave: String = chave_busca(titulo)
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect();
    Some(match chave.as_str() {
        "nome" | "produto" | "descricao" | "nomedoproduto" => Coluna::Nome,
        "grupo" | "nivel1" => Coluna::Grupo,
        "subgrupo" | "nivel2" => Coluna::Subgrupo,
        "categoria" | "nivel3" => Coluna::Categoria,
        "subcategoria" | "nivel4" => Coluna::Subcategoria,
        "ncm" => Coluna::Ncm,
        "codigodebarras" | "codigobarras" | "codbarras" | "ean" | "gtin" => Coluna::CodigoBarras,
        "unidade" | "un" | "und" | "sigla" => Coluna::Unidade,
        "margem" | "margemdesejada" | "margemliquida" => Coluna::Margem,
        "comissao" | "comissaodevendedor" | "comissaovendedor" => Coluna::Comissao,
        "pontodepedido" | "pontopedido" => Coluna::PontoPedido,
        "estoqueminimo" | "minimo" => Coluna::EstoqueMinimo,
        "estoquemaximo" | "maximo" => Coluna::EstoqueMaximo,
        "fabricante" | "marca" => Coluna::Fabricante,
        "codigodofabricante" | "codigofabricante" | "mpn" | "referencia" => {
            Coluna::CodigoFabricante
        }
        "localizacao" | "localizacaofisica" | "prateleira" => Coluna::Localizacao,
        "estoque" | "estoqueinicial" | "quantidade" | "saldo" => Coluna::Estoque,
        "custo" | "custounitario" | "precodecusto" => Coluna::Custo,
        "local" | "deposito" => Coluna::Local,
        _ => return None,
    })
}

/// O nome por extenso e se aceita fração, para as siglas conhecidas.
fn unidade_conhecida(sigla: &str) -> (&'static str, bool) {
    match sigla {
        "UN" | "UND" => ("Unidade", false),
        "PC" | "PCA" => ("Peça", false),
        "CX" => ("Caixa", false),
        "PCT" => ("Pacote", false),
        "DZ" => ("Dúzia", false),
        "PAR" => ("Par", false),
        "KIT" => ("Kit", false),
        "CJ" => ("Conjunto", false),
        "FD" => ("Fardo", false),
        "SC" => ("Saco", false),
        "RL" => ("Rolo", false),
        "KG" => ("Quilograma", true),
        "G" | "GR" => ("Grama", true),
        "L" | "LT" => ("Litro", true),
        "ML" => ("Mililitro", true),
        "M" | "MT" => ("Metro", true),
        "CM" => ("Centímetro", true),
        "M2" => ("Metro quadrado", true),
        "M3" => ("Metro cúbico", true),
        _ => ("", false),
    }
}

/// O cabeçalho entendido: a posição de cada coluna conhecida.
struct Cabecalho(HashMap<Coluna, usize>);

impl Cabecalho {
    /// A célula da coluna, sem espaços nas pontas; `""` se a coluna não existe ou a linha é
    /// curta.
    fn celula<'a>(&self, cels: &'a [String], c: Coluna) -> &'a str {
        self.0
            .get(&c)
            .and_then(|&i| cels.get(i))
            .map_or("", |s| s.trim())
    }
}

/// Lê o cabeçalho; erro quando falta `Nome` ou uma coluna aparece duas vezes.
fn ler_cabecalho(cels: &[String]) -> Result<(Cabecalho, Vec<String>), String> {
    let mut posicoes = HashMap::new();
    let mut ignoradas = Vec::new();
    for (i, titulo) in cels.iter().enumerate() {
        if titulo.trim().is_empty() {
            continue;
        }
        match coluna_de(titulo) {
            Some(c) => {
                if posicoes.insert(c, i).is_some() {
                    return Err(format!("a coluna \"{}\" aparece duas vezes", titulo.trim()));
                }
            }
            None => ignoradas.push(titulo.trim().to_owned()),
        }
    }
    if !posicoes.contains_key(&Coluna::Nome) {
        return Err(
            "a primeira linha precisa ser o cabeçalho, com pelo menos a coluna Nome".to_owned(),
        );
    }
    Ok((Cabecalho(posicoes), ignoradas))
}

/// Uma quantidade opcional não negativa; `Ok(None)` quando a célula está vazia.
fn quantidade(texto: &str, o_que: &str, problemas: &mut Vec<String>) -> Option<Quantidade> {
    if texto.is_empty() {
        return None;
    }
    match Quantidade::de_str(texto) {
        Ok(q) if q.e_negativa() => {
            problemas.push(format!("{o_que} não pode ser negativo(a)"));
            None
        }
        Ok(q) => Some(q),
        Err(_) => {
            problemas.push(format!("{o_que} \"{texto}\" não é um número"));
            None
        }
    }
}

/// Os campos de uma linha já lidos e validados, antes de resolver grupo e unidade (que têm
/// efeito no plano e só podem acontecer com a linha inteira boa).
struct LinhaLidaOk {
    caminho: Vec<String>,
    sigla: String,
    ncm: String,
    codigo_barras: Option<String>,
    margem: Option<Percentual>,
    comissao: Option<Percentual>,
    ponto_pedido: Option<Quantidade>,
    estoque_minimo: Option<Quantidade>,
    estoque_maximo: Option<Quantidade>,
    detalhes: DetalhesTecnicos,
    estoque: Option<EstoqueInicial>,
    custo_sem_estoque: bool,
}

/// A margem desejada: entre 0% e 100% (exclusive).
fn ler_margem(texto: &str, problemas: &mut Vec<String>) -> Option<Percentual> {
    if texto.is_empty() {
        return None;
    }
    match Percentual::de_str(texto) {
        Ok(p) if p.e_fracao_valida() && p != Percentual::CEM => Some(p),
        Ok(_) => {
            problemas.push("a margem precisa estar entre 0% e 100%".to_owned());
            None
        }
        Err(_) => {
            problemas.push(format!("a margem \"{texto}\" não é um percentual"));
            None
        }
    }
}

/// A comissão de vendedor própria: entre 0% e 100% (exclusive).
fn ler_comissao(texto: &str, problemas: &mut Vec<String>) -> Option<Percentual> {
    if texto.is_empty() {
        return None;
    }
    match Percentual::de_str(texto) {
        Ok(p) if crate::precificacao::comissao_valida(p) => Some(p),
        Ok(_) => {
            problemas.push("a comissão precisa estar entre 0% e 100%".to_owned());
            None
        }
        Err(_) => {
            problemas.push(format!("a comissão \"{texto}\" não é um percentual"));
            None
        }
    }
}

/// O local do estoque inicial: o nomeado (sem caixa nem acento) ou, sem nome, o único da
/// empresa.
fn ler_local(nome: &str, locais: &[LocalExistente], problemas: &mut Vec<String>) -> Option<Id> {
    match (nome, locais) {
        ("", []) => {
            problemas.push(
                "não há local de estoque cadastrado: crie um antes de importar estoque".to_owned(),
            );
            None
        }
        ("", [unico]) => Some(unico.id),
        ("", varios) => {
            problemas.push(format!(
                "a empresa tem {} locais de estoque: informe a coluna Local",
                varios.len()
            ));
            None
        }
        (nome, _) => {
            let chave = chave_busca(nome);
            let achado = locais.iter().find(|l| chave_busca(&l.nome) == chave);
            if achado.is_none() {
                problemas.push(format!("o local \"{nome}\" não existe"));
            }
            achado.map(|l| l.id)
        }
    }
}

/// O estoque inicial da linha e se ela traz custo sem estoque. Só há estoque com quantidade
/// positiva; custo sem estoque não tem onde ficar (o custo mora no saldo) e é ignorado, sem
/// erro: planilha de cliente traz custo de tudo.
fn ler_estoque_inicial(
    cab: &Cabecalho,
    cels: &[String],
    locais: &[LocalExistente],
    problemas: &mut Vec<String>,
) -> (Option<EstoqueInicial>, bool) {
    let qtd = quantidade(cab.celula(cels, Coluna::Estoque), "o estoque", problemas);
    let texto_custo = cab.celula(cels, Coluna::Custo);
    let Some(quantidade) = qtd.filter(|q| q.e_positiva()) else {
        return (None, !texto_custo.is_empty());
    };
    let custo = match texto_custo {
        "" => {
            problemas.push("estoque sem custo: informe o custo unitário".to_owned());
            None
        }
        t => match Preco::de_str(t) {
            Ok(p) if p.unidades_internas() > 0 => Some(p),
            Ok(_) => {
                problemas.push("o custo precisa ser maior que zero".to_owned());
                None
            }
            Err(_) => {
                problemas.push(format!("o custo \"{t}\" não é um valor"));
                None
            }
        },
    };
    let local = ler_local(cab.celula(cels, Coluna::Local), locais, problemas);
    let estoque = custo
        .zip(local)
        .map(|(custo_unitario, local)| EstoqueInicial {
            local,
            quantidade,
            custo_unitario,
        });
    (estoque, false)
}

/// Lê e valida os campos da linha (menos o nome e a duplicidade, já tratados). Junta todos
/// os problemas da linha numa lista, para o usuário corrigir tudo de uma vez.
fn ler_campos(
    cab: &Cabecalho,
    cels: &[String],
    codigo_barras: Option<String>,
    locais: &[LocalExistente],
) -> Result<LinhaLidaOk, Vec<String>> {
    let mut problemas = Vec::new();

    let mut caminho: Vec<String> = NIVEIS
        .iter()
        .map(|&c| cab.celula(cels, c).to_owned())
        .collect();
    while caminho.last().is_some_and(String::is_empty) {
        caminho.pop();
    }
    if let Err(m) = validar_caminho(&caminho) {
        problemas.push(m);
    }

    let sigla = match cab.celula(cels, Coluna::Unidade) {
        "" => UNIDADE_PADRAO.to_owned(),
        s => s.to_uppercase(),
    };
    if sigla.chars().count() > 6 || !sigla.chars().all(char::is_alphanumeric) {
        problemas.push(format!(
            "a unidade \"{sigla}\" precisa ser uma sigla de até 6 letras ou números (UN, KG, CX)"
        ));
    }

    let ncm = normalizar_ncm(cab.celula(cels, Coluna::Ncm)).unwrap_or_else(|e| {
        problemas.push(e.to_string());
        String::new()
    });
    let margem = ler_margem(cab.celula(cels, Coluna::Margem), &mut problemas);
    let comissao = ler_comissao(cab.celula(cels, Coluna::Comissao), &mut problemas);
    let ponto_pedido = quantidade(
        cab.celula(cels, Coluna::PontoPedido),
        "o ponto de pedido",
        &mut problemas,
    );
    let estoque_minimo = quantidade(
        cab.celula(cels, Coluna::EstoqueMinimo),
        "o estoque mínimo",
        &mut problemas,
    );
    let estoque_maximo = quantidade(
        cab.celula(cels, Coluna::EstoqueMaximo),
        "o estoque máximo",
        &mut problemas,
    );
    if let (Some(min), Some(max)) = (estoque_minimo, estoque_maximo) {
        if min > max {
            problemas.push("o estoque mínimo é maior que o máximo".to_owned());
        }
    }

    let texto_opcional = |c: Coluna| Some(cab.celula(cels, c).to_owned()).filter(|s| !s.is_empty());
    let detalhes = DetalhesTecnicos {
        fabricante: texto_opcional(Coluna::Fabricante),
        codigo_fabricante: texto_opcional(Coluna::CodigoFabricante),
        localizacao_fisica: texto_opcional(Coluna::Localizacao),
        ..DetalhesTecnicos::default()
    };
    let (estoque, custo_sem_estoque) = ler_estoque_inicial(cab, cels, locais, &mut problemas);

    if !problemas.is_empty() {
        return Err(problemas);
    }
    Ok(LinhaLidaOk {
        caminho,
        sigla,
        ncm,
        codigo_barras,
        margem,
        comissao,
        ponto_pedido,
        estoque_minimo,
        estoque_maximo,
        detalhes,
        estoque,
        custo_sem_estoque,
    })
}

/// O estado da análise linha a linha: o que já existe, indexado, e o plano em construção.
struct Planejador<'a> {
    cab: Cabecalho,
    locais: &'a [LocalExistente],
    por_codigo: HashMap<&'a str, Id>,
    por_nome: HashMap<String, Id>,
    unidades: HashMap<String, Id>,
    unidades_novas: HashMap<String, usize>,
    resolvedor: ResolvedorDeCaminhos,
    ja_existiam: HashSet<Id>,
    /// Nome e código de barras dos produtos novos → a linha que os trouxe.
    nomes_novos: HashMap<String, usize>,
    codigos_novos: HashMap<String, usize>,
    plano: PlanoProdutos,
}

impl<'a> Planejador<'a> {
    fn novo(cab: Cabecalho, existentes: &'a Existentes) -> Self {
        Self {
            cab,
            locais: &existentes.locais,
            por_codigo: existentes
                .produtos
                .iter()
                .filter_map(|p| p.codigo_barras.as_deref().map(|c| (c, p.id)))
                .collect(),
            por_nome: existentes
                .produtos
                .iter()
                .map(|p| (chave_busca(&p.nome), p.id))
                .collect(),
            unidades: existentes
                .unidades
                .iter()
                .map(|u| (u.sigla.to_uppercase(), u.id))
                .collect(),
            unidades_novas: HashMap::new(),
            resolvedor: ResolvedorDeCaminhos::novo(&existentes.grupos),
            ja_existiam: HashSet::new(),
            nomes_novos: HashMap::new(),
            codigos_novos: HashMap::new(),
            plano: PlanoProdutos::default(),
        }
    }

    /// Analisa uma linha: já existe, erro, ou produto novo no plano.
    fn linha(&mut self, linha: usize, cels: &[String]) {
        if cels.iter().all(|c| c.trim().is_empty()) {
            return; // só separadores
        }
        self.plano.linhas += 1;
        match self.classificar(linha, cels) {
            Ok(Some(lida)) => {
                let nome = self.cab.celula(cels, Coluna::Nome).to_owned();
                self.planejar_novo(linha, nome, lida);
            }
            Ok(None) => {}
            Err(mensagem) => self.plano.erros.push(ErroDeLinha { linha, mensagem }),
        }
    }

    /// `Ok(None)` quando o produto já existe (fica como está); `Ok(Some)` com a linha lida
    /// quando é novo e está boa; `Err` com o que está errado.
    fn classificar(
        &mut self,
        linha: usize,
        cels: &[String],
    ) -> Result<Option<LinhaLidaOk>, String> {
        let nome = self.cab.celula(cels, Coluna::Nome);
        if nome.is_empty() {
            return Err("sem nome".to_owned());
        }
        let codigo_barras = match self.cab.celula(cels, Coluna::CodigoBarras) {
            "" => None,
            t => Some(validar_gtin(t).map_err(|e| format!("código de barras \"{t}\": {e}"))?),
        };

        // Já existe: fica como está (por código de barras; sem ele, ou sem achar, pelo nome).
        let chave_nome = chave_busca(nome);
        let existente = codigo_barras
            .as_deref()
            .and_then(|c| self.por_codigo.get(c))
            .or_else(|| self.por_nome.get(&chave_nome));
        if let Some(&id) = existente {
            self.ja_existiam.insert(id);
            return Ok(None);
        }
        if let Some(anterior) = self.nomes_novos.get(&chave_nome) {
            return Err(format!(
                "produto repetido: o mesmo nome está na linha {anterior}"
            ));
        }
        if let Some(anterior) = codigo_barras
            .as_ref()
            .and_then(|c| self.codigos_novos.get(c))
        {
            return Err(format!(
                "código de barras repetido: o mesmo está na linha {anterior}"
            ));
        }
        let lida = ler_campos(&self.cab, cels, codigo_barras, self.locais)
            .map_err(|problemas| problemas.join("; "))?;
        self.nomes_novos.insert(chave_nome, linha);
        if let Some(c) = &lida.codigo_barras {
            self.codigos_novos.insert(c.clone(), linha);
        }
        Ok(Some(lida))
    }

    /// A unidade de uma sigla: a existente, ou uma nova no plano (uma vez por sigla).
    fn unidade(&mut self, sigla: &str) -> UnidadePlanejada {
        if let Some(&id) = self.unidades.get(sigla) {
            return UnidadePlanejada::Existente(id);
        }
        let unidades = &mut self.plano.unidades;
        let indice = *self
            .unidades_novas
            .entry(sigla.to_owned())
            .or_insert_with(|| {
                let (nome, fracionavel) = unidade_conhecida(sigla);
                unidades.push(UnidadeNova {
                    sigla: sigla.to_owned(),
                    nome: if nome.is_empty() {
                        sigla.to_owned()
                    } else {
                        nome.to_owned()
                    },
                    fracionavel,
                });
                unidades.len() - 1
            });
        UnidadePlanejada::Nova(indice)
    }

    /// Põe no plano um produto de uma linha boa — daqui em diante os efeitos valem direto.
    fn planejar_novo(&mut self, linha: usize, nome: String, lida: LinhaLidaOk) {
        let caminho = if lida.caminho.is_empty() {
            vec![GRUPO_SEM_GRUPO.to_owned()]
        } else {
            lida.caminho
        };
        let grupo = self.resolvedor.resolver(&caminho);
        let unidade = self.unidade(&lida.sigla);
        if lida.custo_sem_estoque {
            self.plano.custos_sem_estoque += 1;
        }
        self.plano.produtos.push(ProdutoNovo {
            linha,
            nome,
            grupo,
            caminho: caminho.join(" › "),
            unidade,
            sigla_unidade: lida.sigla,
            ncm: lida.ncm,
            codigo_barras: lida.codigo_barras,
            margem: lida.margem,
            comissao: lida.comissao,
            ponto_pedido: lida.ponto_pedido,
            estoque_minimo: lida.estoque_minimo,
            estoque_maximo: lida.estoque_maximo,
            detalhes: lida.detalhes,
            estoque: lida.estoque,
        });
    }

    fn concluir(self) -> PlanoProdutos {
        let mut plano = self.plano;
        (plano.grupos, _) = self.resolvedor.concluir();
        plano.ja_existiam = contar(self.ja_existiam.len());
        plano
    }
}

/// Analisa a planilha contra o que já existe.
#[must_use]
pub fn planejar(texto: &str, existentes: &Existentes) -> PlanoProdutos {
    let mut linhas = ler_linhas(texto).into_iter();
    let Some((linha_cab, cels_cab)) = linhas.next() else {
        return PlanoProdutos::default(); // arquivo vazio: nada a fazer
    };
    let (cab, ignoradas) = match ler_cabecalho(&cels_cab) {
        Ok(lido) => lido,
        Err(mensagem) => {
            return PlanoProdutos {
                erros: vec![ErroDeLinha {
                    linha: linha_cab,
                    mensagem,
                }],
                ..PlanoProdutos::default()
            };
        }
    };
    let mut planejador = Planejador::novo(cab, existentes);
    planejador.plano.colunas_ignoradas = ignoradas;
    for (linha, cels) in linhas {
        planejador.linha(linha, &cels);
    }
    planejador.concluir()
}

#[cfg(test)]
mod testes {
    use super::*;

    fn grupo(nome: &str, codigo: &str, pai: Option<Id>) -> NoExistente {
        NoExistente {
            id: Id::novo(),
            codigo: codigo.into(),
            nome: nome.into(),
            pai,
        }
    }

    fn produto(nome: &str, codigo_barras: Option<&str>) -> ProdutoExistente {
        ProdutoExistente {
            id: Id::novo(),
            nome: nome.into(),
            codigo_barras: codigo_barras.map(Into::into),
        }
    }

    fn local(nome: &str) -> LocalExistente {
        LocalExistente {
            id: Id::novo(),
            nome: nome.into(),
        }
    }

    fn q(t: &str) -> Quantidade {
        Quantidade::de_str(t).unwrap()
    }

    /// Um GTIN-13 válido (o dígito verificador confere).
    const EAN: &str = "7891000315507";

    #[test]
    fn so_o_nome_basta_e_cai_em_sem_grupo_com_un() {
        let p = planejar("Nome\nParafuso 3mm\n", &Existentes::default());
        assert!(p.erros.is_empty(), "{:?}", p.erros);
        assert_eq!(p.produtos.len(), 1);
        let novo = &p.produtos[0];
        assert_eq!(novo.caminho, GRUPO_SEM_GRUPO);
        assert_eq!(novo.sigla_unidade, "UN");
        assert_eq!(novo.unidade, UnidadePlanejada::Nova(0));
        assert_eq!(p.unidades[0].nome, "Unidade");
        assert_eq!(p.grupos.len(), 1);
        assert!(novo.ncm.is_empty());
        assert!(novo.estoque.is_none());
    }

    #[test]
    fn colunas_por_nome_em_qualquer_ordem_sem_caixa_nem_acento() {
        let csv = "\u{feff}Código de Barras;DESCRIÇÃO;Subgrupo;Grupo;Unidade;NCM;Margem;Marca\n\
                   7891000315507;Leite em pó 400g;Leite;Mercearia;un;0402.21.10;30%;Ninho\n";
        let p = planejar(csv, &Existentes::default());
        assert!(p.erros.is_empty(), "{:?}", p.erros);
        let novo = &p.produtos[0];
        assert_eq!(novo.nome, "Leite em pó 400g");
        assert_eq!(novo.caminho, "Mercearia › Leite");
        assert_eq!(novo.sigla_unidade, "UN");
        assert_eq!(novo.ncm, "04022110");
        assert_eq!(novo.codigo_barras.as_deref(), Some(EAN));
        assert_eq!(novo.margem, Some(Percentual::pontos(30)));
        assert_eq!(novo.detalhes.fabricante.as_deref(), Some("Ninho"));
        assert_eq!(p.grupos.len(), 2);
    }

    #[test]
    fn sem_cabecalho_com_nome_e_um_erro_so() {
        let p = planejar("Parafuso;10\nPorca;5\n", &Existentes::default());
        assert_eq!(p.erros.len(), 1);
        assert_eq!(p.erros[0].linha, 1);
        assert!(p.erros[0].mensagem.contains("Nome"));
        assert!(p.produtos.is_empty());
    }

    #[test]
    fn coluna_repetida_e_coluna_desconhecida() {
        let p = planejar("Nome;Produto\nA;B\n", &Existentes::default());
        assert_eq!(p.erros.len(), 1);
        assert!(p.erros[0].mensagem.contains("duas vezes"));

        let p = planejar(
            "Nome;Cor;Preço de venda\nA;azul;10\n",
            &Existentes::default(),
        );
        assert!(p.erros.is_empty());
        assert_eq!(p.colunas_ignoradas, ["Cor", "Preço de venda"]);
    }

    #[test]
    fn existente_por_codigo_de_barras_ou_por_nome_fica_como_esta() {
        let existentes = Existentes {
            produtos: vec![
                produto("Leite Ninho 400g", Some(EAN)),
                produto("Parafuso 3mm", None),
            ],
            ..Existentes::default()
        };
        // Código igual com outro nome, nome igual sem caixa nem acento: os dois já existem.
        let csv = format!(
            "Nome;EAN;Estoque;Custo\nLeite (outro nome);{EAN};10;5\nPARAFUSO 3MM;;;\nNovo;;;\n"
        );
        let p = planejar(&csv, &existentes);
        assert!(p.erros.is_empty(), "{:?}", p.erros);
        assert_eq!(p.ja_existiam, 2);
        assert_eq!(p.produtos.len(), 1);
        assert_eq!(p.produtos[0].nome, "Novo");
        assert!(!p.tem_estoque(), "o estoque do existente não é reaplicado");
    }

    #[test]
    fn importar_duas_vezes_nao_cria_nada_na_segunda() {
        let csv =
            format!("Nome;Grupo;Subgrupo;EAN\nLeite;Mercearia;Leite;{EAN}\nArroz 5kg;Mercearia;\n");
        let p1 = planejar(&csv, &Existentes::default());
        assert_eq!(p1.produtos.len(), 2);
        // Aplica o plano em memória, como o comando faz no banco.
        let mut ids = Vec::new();
        let mut existentes = Existentes::default();
        for g in &p1.grupos {
            let id = Id::novo();
            let pai = match g.pai {
                PaiPlanejado::Raiz => None,
                PaiPlanejado::Existente(p) => Some(p),
                PaiPlanejado::Novo(i) => Some(ids[i]),
            };
            ids.push(id);
            existentes.grupos.push(NoExistente {
                id,
                codigo: g.codigo.clone(),
                nome: g.nome.clone(),
                pai,
            });
        }
        for u in &p1.unidades {
            existentes.unidades.push(UnidadeExistente {
                id: Id::novo(),
                sigla: u.sigla.clone(),
            });
        }
        for n in &p1.produtos {
            existentes.produtos.push(ProdutoExistente {
                id: Id::novo(),
                nome: n.nome.clone(),
                codigo_barras: n.codigo_barras.clone(),
            });
        }
        let p2 = planejar(&csv, &existentes);
        assert!(p2.erros.is_empty());
        assert!(p2.produtos.is_empty());
        assert!(p2.grupos.is_empty());
        assert!(p2.unidades.is_empty());
        assert_eq!(p2.ja_existiam, 2);
    }

    #[test]
    fn repetidos_na_planilha_sao_erro() {
        let csv = format!("Nome;EAN\nA;{EAN}\na;\nB;{EAN}\n");
        let p = planejar(&csv, &Existentes::default());
        assert_eq!(p.produtos.len(), 1);
        assert_eq!(p.erros.len(), 2);
        assert_eq!(p.erros[0].linha, 3);
        assert!(p.erros[0].mensagem.contains("linha 2"));
        assert_eq!(p.erros[1].linha, 4);
        assert!(p.erros[1].mensagem.contains("código de barras repetido"));
    }

    #[test]
    fn todos_os_problemas_da_linha_numa_mensagem() {
        let csv =
            "Nome;NCM;Margem;Unidade;Estoque mínimo;Estoque máximo;Grupo;Subgrupo;Categoria\n\
                   X;123;150;caixa grande;10;5;;Sub;Cat\n";
        let p = planejar(csv, &Existentes::default());
        assert_eq!(p.erros.len(), 1);
        let m = &p.erros[0].mensagem;
        for trecho in [
            "8 dígitos",
            "entre 0% e 100%",
            "sigla",
            "mínimo é maior",
            "vazio",
        ] {
            assert!(m.contains(trecho), "faltou {trecho:?} em {m:?}");
        }
        assert!(p.produtos.is_empty());
        assert!(
            p.grupos.is_empty(),
            "linha com erro não deixa grupo planejado"
        );
        assert!(p.unidades.is_empty());
    }

    #[test]
    fn codigo_de_barras_invalido_e_nome_vazio() {
        let csv = "Nome;EAN\n;7891000315507\nB;7891000315508\nC;123\n";
        let p = planejar(csv, &Existentes::default());
        let linhas: Vec<_> = p.erros.iter().map(|e| e.linha).collect();
        assert_eq!(linhas, [2, 3, 4]);
        assert!(p.erros[0].mensagem.contains("sem nome"));
    }

    #[test]
    fn estoque_inicial_no_unico_local_com_custo_brasileiro() {
        let loja = local("Loja");
        let id_loja = loja.id;
        let existentes = Existentes {
            locais: vec![loja],
            ..Existentes::default()
        };
        let p = planejar(
            "Nome;Estoque;Custo\nCabo USB;1.250,5;R$ 12,90\nSem estoque;0;3,00\nSem nada;;\n",
            &existentes,
        );
        assert!(p.erros.is_empty(), "{:?}", p.erros);
        let e = p.produtos[0].estoque.unwrap();
        assert_eq!(e.local, id_loja);
        assert_eq!(e.quantidade, q("1250,5"));
        assert_eq!(e.custo_unitario, Preco::de_str("12,90").unwrap());
        assert!(p.produtos[1].estoque.is_none());
        assert_eq!(p.custos_sem_estoque, 1, "custo com estoque zero é ignorado");
        assert!(p.tem_estoque());
        assert_eq!(p.previa().com_estoque, 1);
    }

    #[test]
    fn estoque_exige_custo_e_local_resolvido() {
        let varios = Existentes {
            locais: vec![local("Loja"), local("Depósito")],
            ..Existentes::default()
        };
        let p = planejar(
            "Nome;Estoque;Custo;Local\nA;5;;Loja\nB;5;2;\nC;5;2;Galpão\nD;5;2;deposito\nE;-1;2;Loja\n",
            &varios,
        );
        let msgs: Vec<_> = p
            .erros
            .iter()
            .map(|e| (e.linha, e.mensagem.as_str()))
            .collect();
        assert_eq!(msgs.len(), 4, "{msgs:?}");
        assert!(msgs[0].1.contains("sem custo"));
        assert!(msgs[1].1.contains("2 locais"));
        assert!(msgs[2].1.contains("Galpão"));
        assert!(msgs[3].1.contains("negativo"));
        // "deposito" casa com "Depósito" sem acento.
        assert_eq!(p.produtos.len(), 1);
        assert_eq!(p.produtos[0].nome, "D");

        let nenhum = planejar("Nome;Estoque;Custo\nA;5;2\n", &Existentes::default());
        assert!(nenhum.erros[0].mensagem.contains("não há local"));
    }

    #[test]
    fn grupos_existentes_e_novos_compartilham_a_arvore() {
        let mercearia = grupo("Mercearia", "MERC", None);
        let id = mercearia.id;
        let existentes = Existentes {
            grupos: vec![mercearia],
            ..Existentes::default()
        };
        let p = planejar(
            "Nome;Grupo;Subgrupo\nA;mercearia;Grãos\nB;Mercearia;graos\nC;Mercearia\n",
            &existentes,
        );
        assert!(p.erros.is_empty());
        assert_eq!(p.grupos.len(), 1, "Grãos só é criado uma vez");
        assert_eq!(p.grupos[0].pai, PaiPlanejado::Existente(id));
        assert_eq!(p.produtos[0].grupo, PaiPlanejado::Novo(0));
        assert_eq!(p.produtos[1].grupo, PaiPlanejado::Novo(0));
        assert_eq!(p.produtos[2].grupo, PaiPlanejado::Existente(id));
    }

    #[test]
    fn unidades_existentes_novas_e_conhecidas() {
        let un = UnidadeExistente {
            id: Id::novo(),
            sigla: "UN".into(),
        };
        let id_un = un.id;
        let existentes = Existentes {
            unidades: vec![un],
            ..Existentes::default()
        };
        let p = planejar("Nome;Unidade\nA;un\nB;kg\nC;KG\nD;BLT\n", &existentes);
        assert!(p.erros.is_empty());
        assert_eq!(p.produtos[0].unidade, UnidadePlanejada::Existente(id_un));
        assert_eq!(p.produtos[1].unidade, UnidadePlanejada::Nova(0));
        assert_eq!(p.produtos[2].unidade, UnidadePlanejada::Nova(0));
        assert_eq!(
            p.unidades,
            [
                UnidadeNova {
                    sigla: "KG".into(),
                    nome: "Quilograma".into(),
                    fracionavel: true
                },
                UnidadeNova {
                    sigla: "BLT".into(),
                    nome: "BLT".into(),
                    fracionavel: false
                },
            ]
        );
    }

    #[test]
    fn comissao_propria_e_faixa() {
        let p = planejar("Nome;Comissão\nA;3,5%\nB;\nC;100\n", &Existentes::default());
        assert_eq!(p.erros.len(), 1);
        assert_eq!(p.erros[0].linha, 4);
        assert!(p.erros[0].mensagem.contains("comissão"));
        assert_eq!(
            p.produtos[0].comissao,
            Some(Percentual::de_str("3,5").unwrap())
        );
        assert_eq!(p.produtos[1].comissao, None);
        assert!(p.define_parametros());
    }

    #[test]
    fn parametros_e_previa() {
        let p = planejar(
            "Nome;Ponto de pedido;Estoque mínimo;Estoque máximo;Localização;MPN\n\
             A;5;2;20;Gaveta 3;LM317\nB;;;;;\n",
            &Existentes::default(),
        );
        assert!(p.erros.is_empty(), "{:?}", p.erros);
        let a = &p.produtos[0];
        assert_eq!(a.ponto_pedido, Some(q("5")));
        assert_eq!(a.estoque_minimo, Some(q("2")));
        assert_eq!(a.estoque_maximo, Some(q("20")));
        assert_eq!(a.detalhes.localizacao_fisica.as_deref(), Some("Gaveta 3"));
        assert_eq!(a.detalhes.codigo_fabricante.as_deref(), Some("LM317"));
        assert!(p.define_parametros());
        let v = p.previa();
        assert_eq!((v.novos, v.grupos_novos, v.linhas), (2, 1, 2));
        assert_eq!(v.unidades_novas, ["UN"]);
        assert_eq!(v.amostra[0].grupo, GRUPO_SEM_GRUPO);
    }

    #[test]
    fn arquivo_vazio_e_linhas_em_branco() {
        assert!(planejar("", &Existentes::default()).erros.is_empty());
        let p = planejar("Nome\n\n;\nA\n\n", &Existentes::default());
        assert!(p.erros.is_empty());
        assert_eq!((p.linhas, p.produtos.len()), (1, 1));
    }
}
