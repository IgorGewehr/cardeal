//! As consultas de estoque: leitura autorizada sobre `estoque_produto`/`estoque_saldo_local`
//! — `docs/modulos/estoque.md` §10 (a ficha de produto precisa do saldo agregado por
//! produto para a lista principal da tela).

use cardeal_kernel::{Data, Dinheiro, Id, Instante, Preco, Quantidade, Resultado};
use cardeal_modkit::{Consulta, Ctx};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::produto::{EstadoLote, OrigemLote, Produto};
use crate::repositorio::{blob, id_de, movimento_de_linha, persist, produto_de_linha};
use crate::saldo::Movimento;

/// Um produto com o saldo somado entre todos os locais — o que sustenta a lista principal da
/// tela de Estoque. `custo_medio` aqui é uma aproximação (o maior custo médio entre os
/// locais, não uma média ponderada entre eles) — suficiente para a lista; o detalhe por
/// local (`docs/modulos/estoque.md` §10) fica para quando a ficha de produto existir.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemProdutoComSaldo {
    /// O produto.
    pub produto: Id,
    /// O nome.
    pub nome: String,
    /// NCM.
    pub ncm: String,
    /// A soma do disponível em todos os locais.
    pub disponivel: Quantidade,
    /// A soma do reservado em todos os locais.
    pub reservado: Quantidade,
    /// Uma aproximação do custo médio (ver nota do tipo).
    pub custo_medio: Preco,
}

/// Todos os produtos ativos com saldo agregado, por nome. Sem cursor real ainda, mesma
/// decisão do financeiro (`docs/09-protocolo-api.md` §5): um teto de 500 linhas é suficiente
/// para o catálogo de uma PME.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn produtos_com_saldo(
    conexao: &Connection,
    empresa: Id,
) -> Resultado<Vec<ItemProdutoComSaldo>> {
    let mut stmt = conexao
        .prepare(
            "SELECT p.id, p.nome, p.ncm,
                    COALESCE(SUM(s.quantidade_disponivel), 0),
                    COALESCE(SUM(s.quantidade_reservada), 0),
                    COALESCE(MAX(s.custo_medio), 0)
             FROM estoque_produto p
             LEFT JOIN estoque_saldo_local s ON s.produto = p.id
             WHERE p.empresa = ?1 AND p.ativo = 1
             GROUP BY p.id, p.nome, p.ncm
             ORDER BY p.nome ASC
             LIMIT 500",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map([blob(empresa)], |r| {
            Ok(ItemProdutoComSaldo {
                produto: id_de(r.get::<_, Vec<u8>>(0)?),
                nome: r.get(1)?,
                ncm: r.get(2)?,
                disponivel: Quantidade::interna(r.get::<_, i64>(3)?),
                reservado: Quantidade::interna(r.get::<_, i64>(4)?),
                custo_medio: Preco::interna(r.get::<_, i64>(5)?),
            })
        })
        .map_err(persist)?;
    linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)
}

/// Lista os produtos ativos da empresa com saldo agregado.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProdutosComSaldo;

impl Consulta for ProdutosComSaldo {
    type Saida = Vec<ItemProdutoComSaldo>;
    const PERMISSAO: &'static str = "estoque.produto.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        produtos_com_saldo(conexao, ctx.empresa)
    }
}

/// Busca um produto pelo código de barras (GTIN) — o que sustenta o bipe no balcão/PDV.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn produto_por_codigo_barras(
    conexao: &Connection,
    empresa: Id,
    codigo_barras: &str,
) -> Resultado<Option<Produto>> {
    conexao
        .query_row(
            "SELECT id, empresa, grupo_produto, nome, ncm, cest, codigo_barras, fabricante,
                    codigo_fabricante, categoria_tecnica, especificacao_tecnica,
                    compatibilidade, garantia_fornecedor_dias, localizacao_fisica,
                    controla_grade, controla_lote, controla_validade, unidade_padrao,
                    ponto_pedido, estoque_minimo, estoque_maximo, ativo, versao
             FROM estoque_produto WHERE empresa = ?1 AND codigo_barras = ?2",
            rusqlite::params![blob(empresa), codigo_barras],
            produto_de_linha,
        )
        .optional()
        .map_err(persist)
}

/// Busca um produto pelo código de barras bipado no balcão/PDV.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProdutoPorCodigoBarras {
    /// O código de barras (GTIN), como veio do leitor.
    pub codigo_barras: String,
}

impl Consulta for ProdutoPorCodigoBarras {
    type Saida = Option<Produto>;
    const PERMISSAO: &'static str = "estoque.produto.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        produto_por_codigo_barras(conexao, ctx.empresa, &self.codigo_barras)
    }
}

/// Um grupo de produto — o suficiente para popular um seletor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemGrupoProduto {
    /// O grupo.
    pub id: Id,
    /// O código curto.
    pub codigo: String,
    /// O nome.
    pub nome: String,
}

/// Todos os grupos de produto da empresa — para o formulário de cadastro de produto (não há
/// tantos grupos a ponto de precisar de teto/paginação numa PME).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GruposProduto;

impl Consulta for GruposProduto {
    type Saida = Vec<ItemGrupoProduto>;
    const PERMISSAO: &'static str = "estoque.produto.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let mut stmt = conexao
            .prepare("SELECT id, codigo, nome FROM estoque_grupo_produto WHERE empresa = ?1 ORDER BY nome ASC")
            .map_err(persist)?;
        let linhas = stmt
            .query_map([blob(ctx.empresa)], |r| {
                Ok(ItemGrupoProduto {
                    id: id_de(r.get::<_, Vec<u8>>(0)?),
                    codigo: r.get(1)?,
                    nome: r.get(2)?,
                })
            })
            .map_err(persist)?;
        linhas
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)
    }
}

/// Uma unidade de medida — o suficiente para popular um seletor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemUnidade {
    /// A unidade.
    pub id: Id,
    /// A sigla curta (ex.: "UN").
    pub sigla: String,
    /// O nome.
    pub nome: String,
}

/// Todas as unidades de medida da empresa.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Unidades;

impl Consulta for Unidades {
    type Saida = Vec<ItemUnidade>;
    const PERMISSAO: &'static str = "estoque.produto.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let mut stmt = conexao
            .prepare(
                "SELECT id, sigla, nome FROM estoque_unidade WHERE empresa = ?1 ORDER BY sigla ASC",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map([blob(ctx.empresa)], |r| {
                Ok(ItemUnidade {
                    id: id_de(r.get::<_, Vec<u8>>(0)?),
                    sigla: r.get(1)?,
                    nome: r.get(2)?,
                })
            })
            .map_err(persist)?;
        linhas
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)
    }
}

/// O saldo disponível de um produto somado entre todos os locais — versão "só este produto"
/// de [`produtos_com_saldo`], para quem já tem o `Id` do produto e não precisa da lista
/// inteira (ex.: `mod-os` cruzando itens de peça pendentes de aplicação contra o estoque
/// real, via a porta pública desta consulta — nunca lendo `estoque_saldo_local` direto,
/// `docs/contratos-internos.md` §7 regra 2).
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn saldo_disponivel_do_produto(conexao: &Connection, produto: Id) -> Resultado<Quantidade> {
    let soma: i64 = conexao
        .query_row(
            "SELECT COALESCE(SUM(quantidade_disponivel), 0) FROM estoque_saldo_local
             WHERE produto = ?1",
            [blob(produto)],
            |r| r.get(0),
        )
        .map_err(persist)?;
    Ok(Quantidade::interna(soma))
}

/// Consulta o saldo disponível de um produto (somado entre locais).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SaldoDisponivelDoProduto {
    /// O produto.
    pub produto: Id,
}

impl Consulta for SaldoDisponivelDoProduto {
    type Saida = Quantidade;
    const PERMISSAO: &'static str = "estoque.saldo.ver";

    fn executar(self, _ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        saldo_disponivel_do_produto(conexao, self.produto)
    }
}

/// Um local de estoque — o suficiente para popular um seletor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemLocal {
    /// O local.
    pub id: Id,
    /// O nome.
    pub nome: String,
    /// O tipo (`"Loja"`, `"Deposito"`, ...).
    pub tipo: String,
}

/// Todos os locais ativos da empresa.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Locais;

impl Consulta for Locais {
    type Saida = Vec<ItemLocal>;
    const PERMISSAO: &'static str = "estoque.produto.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let mut stmt = conexao
            .prepare("SELECT id, nome, tipo FROM estoque_local WHERE empresa = ?1 AND ativo = 1 ORDER BY nome ASC")
            .map_err(persist)?;
        let linhas = stmt
            .query_map([blob(ctx.empresa)], |r| {
                Ok(ItemLocal {
                    id: id_de(r.get::<_, Vec<u8>>(0)?),
                    nome: r.get(1)?,
                    tipo: r.get(2)?,
                })
            })
            .map_err(persist)?;
        linhas
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)
    }
}

/// Um produto cujo disponível (somado entre todos os locais) caiu abaixo do ponto de
/// pedido — o radar de reposição sobre [`crate::produto::Produto::abaixo_do_ponto`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemAbaixoDoPontoPedido {
    /// O produto.
    pub produto: Id,
    /// O nome.
    pub nome: String,
    /// A soma do disponível em todos os locais.
    pub disponivel: Quantidade,
    /// O ponto de pedido cadastrado.
    pub ponto_pedido: Quantidade,
    /// O estoque mínimo cadastrado, se houver.
    pub estoque_minimo: Option<Quantidade>,
}

/// Os produtos ativos, com ponto de pedido definido, cujo disponível agregado está abaixo
/// dele — o que sustenta o radar de reposição. Mesmo teto das demais listas desta fatia
/// (`docs/09-protocolo-api.md` §5).
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn produtos_abaixo_do_ponto_pedido(
    conexao: &Connection,
    empresa: Id,
) -> Resultado<Vec<ItemAbaixoDoPontoPedido>> {
    let mut stmt = conexao
        .prepare(
            "SELECT p.id, p.nome, p.ponto_pedido, p.estoque_minimo,
                    COALESCE(SUM(s.quantidade_disponivel), 0) AS disponivel
             FROM estoque_produto p
             LEFT JOIN estoque_saldo_local s ON s.produto = p.id
             WHERE p.empresa = ?1 AND p.ativo = 1 AND p.ponto_pedido IS NOT NULL
             GROUP BY p.id, p.nome, p.ponto_pedido, p.estoque_minimo
             HAVING disponivel < p.ponto_pedido
             ORDER BY p.nome ASC
             LIMIT 500",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map([blob(empresa)], |r| {
            Ok(ItemAbaixoDoPontoPedido {
                produto: id_de(r.get::<_, Vec<u8>>(0)?),
                nome: r.get(1)?,
                ponto_pedido: Quantidade::interna(r.get::<_, i64>(2)?),
                estoque_minimo: r.get::<_, Option<i64>>(3)?.map(Quantidade::interna),
                disponivel: Quantidade::interna(r.get::<_, i64>(4)?),
            })
        })
        .map_err(persist)?;
    linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)
}

/// Consulta os produtos abaixo do ponto de pedido.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProdutosAbaixoDoPontoPedido;

impl Consulta for ProdutosAbaixoDoPontoPedido {
    type Saida = Vec<ItemAbaixoDoPontoPedido>;
    const PERMISSAO: &'static str = "estoque.compra_sugerida.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        produtos_abaixo_do_ponto_pedido(conexao, ctx.empresa)
    }
}

/// Os movimentos de um produto — a rastreabilidade peça↔origem (ex.: `os` cruzando a peça
/// aplicada com a ordem de serviço que a consumiu), sobre os índices
/// `estoque_movimento_produto`/`estoque_movimento_origem`. Mais recente primeiro; sem
/// cursor real ainda, mesmo teto das demais listas desta fatia.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn movimentos_do_produto(
    conexao: &Connection,
    produto: Id,
    origem_modulo: Option<&str>,
) -> Resultado<Vec<Movimento>> {
    let mut stmt = conexao
        .prepare(
            "SELECT id, empresa, produto, variacao, local, tipo, quantidade, custo_unitario,
                    lote, origem_modulo, origem_id, lancamento, criado_em, criado_por
             FROM estoque_movimento
             WHERE produto = ?1 AND (?2 IS NULL OR origem_modulo = ?2)
             ORDER BY criado_em DESC
             LIMIT 500",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map(rusqlite::params![blob(produto), origem_modulo], movimento_de_linha)
        .map_err(persist)?;
    linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)
}

/// Consulta os movimentos de um produto, opcionalmente filtrados por módulo de origem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MovimentosDoProduto {
    /// O produto.
    pub produto: Id,
    /// Filtra por módulo de origem (ex.: `"os"`), quando informado.
    pub origem_modulo: Option<String>,
}

impl Consulta for MovimentosDoProduto {
    type Saida = Vec<Movimento>;
    const PERMISSAO: &'static str = "estoque.movimento.ver";

    fn executar(self, _ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        movimentos_do_produto(conexao, self.produto, self.origem_modulo.as_deref())
    }
}

// ── Lote/peça rastreável e aparelho de origem (o caso de uso do post-it) ──────────────────

/// Busca um aparelho de origem pelo id.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn buscar_aparelho_origem(
    conexao: &Connection,
    id: Id,
) -> Resultado<Option<crate::aparelho_origem::AparelhoOrigem>> {
    conexao
        .query_row(
            "SELECT id, empresa, descricao, identificador, custo_aquisicao, adquirido_em,
                    fornecedor, observacoes
             FROM estoque_aparelho_origem WHERE id = ?1",
            [blob(id)],
            crate::repositorio::aparelho_origem_de_linha,
        )
        .optional()
        .map_err(persist)
}

const COLUNAS_LOTE: &str = "id, empresa, produto, local, codigo, origem, fornecedor, \
    aparelho_origem, fabricacao, validade, quantidade_inicial, custo_unitario, estado, criado_em";

/// Busca um lote pelo id.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn buscar_lote(conexao: &Connection, id: Id) -> Resultado<Option<crate::produto::Lote>> {
    conexao
        .query_row(
            &format!("SELECT {COLUNAS_LOTE} FROM estoque_lote WHERE id = ?1"),
            [blob(id)],
            crate::repositorio::lote_de_linha,
        )
        .optional()
        .map_err(persist)
}

/// Busca um lote pelo código digitado à mão — o que sustenta a busca pelo post-it colado na
/// peça. Único por empresa, não só por produto: o técnico digita sem saber de antemão a qual
/// produto o código pertence.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn buscar_lote_por_codigo(
    conexao: &Connection,
    empresa: Id,
    codigo: &str,
) -> Resultado<Option<crate::produto::Lote>> {
    conexao
        .query_row(
            &format!("SELECT {COLUNAS_LOTE} FROM estoque_lote WHERE empresa = ?1 AND codigo = ?2"),
            rusqlite::params![blob(empresa), codigo],
            crate::repositorio::lote_de_linha,
        )
        .optional()
        .map_err(persist)
}

/// A quantidade ainda disponível de um lote específico — a soma das entradas menos as saídas
/// registradas com este `lote` em `estoque_movimento` (o log é a própria fonte da verdade;
/// não há coluna de saldo redundante no lote).
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn saldo_do_lote(conexao: &Connection, lote: Id) -> Resultado<Quantidade> {
    let saldo: i64 = conexao
        .query_row(
            "SELECT COALESCE(SUM(CASE WHEN tipo = 'Entrada' THEN quantidade ELSE -quantidade END), 0)
             FROM estoque_movimento WHERE lote = ?1",
            [blob(lote)],
            |r| r.get(0),
        )
        .map_err(persist)?;
    Ok(Quantidade::interna(saldo))
}

/// Os movimentos de um lote específico, mais recente primeiro.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn movimentos_do_lote(conexao: &Connection, lote: Id) -> Resultado<Vec<Movimento>> {
    let mut stmt = conexao
        .prepare(
            "SELECT id, empresa, produto, variacao, local, tipo, quantidade, custo_unitario,
                    lote, origem_modulo, origem_id, lancamento, criado_em, criado_por
             FROM estoque_movimento WHERE lote = ?1 ORDER BY criado_em DESC",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map([blob(lote)], movimento_de_linha)
        .map_err(persist)?;
    linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)
}

/// Um resumo do aparelho de origem — o suficiente para a consulta de detalhe do lote
/// responder "custo desse aparelho usado" sem expor a entidade inteira.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AparelhoOrigemResumo {
    /// O aparelho de origem.
    pub id: Id,
    /// Descrição livre.
    pub descricao: String,
    /// Identificador livre (IMEI/serial), quando anotado.
    pub identificador: Option<String>,
    /// Quanto custou adquirir o aparelho inteiro.
    pub custo_aquisicao: Dinheiro,
    /// Quando foi adquirido.
    pub adquirido_em: Data,
}

/// O detalhe completo de um lote/peça rastreável — a resposta a "digitando esse código, o
/// técnico precisa achar": origem, custo, aparelho de origem (se houver) com o custo dele,
/// localização física e o histórico de movimentações (entrada, e em qual OS/aparelho saiu,
/// quando). `docs/modulos/estoque.md` §3.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetalheLote {
    /// O lote.
    pub lote: Id,
    /// O código digitado (o texto do post-it).
    pub codigo: String,
    /// O produto.
    pub produto: Id,
    /// O nome do produto.
    pub produto_nome: String,
    /// O local de estoque.
    pub local: Id,
    /// O nome do local.
    pub local_nome: String,
    /// A localização física (prateleira/gaveta) cadastrada no produto, quando houver.
    pub localizacao_fisica: Option<String>,
    /// De onde a peça veio.
    pub origem: OrigemLote,
    /// O fornecedor, quando `origem = Compra`.
    pub fornecedor: Option<Id>,
    /// O aparelho de origem (com o custo dele), quando `origem = AparelhoUsado`.
    pub aparelho_origem: Option<AparelhoOrigemResumo>,
    /// Data de fabricação, quando conhecida.
    pub fabricacao: Option<Data>,
    /// Data de validade, quando o produto controla validade.
    pub validade: Option<Data>,
    /// Quantidade da entrada original.
    pub quantidade_inicial: Quantidade,
    /// Quantidade ainda disponível deste lote especificamente.
    pub quantidade_atual: Quantidade,
    /// O custo unitário desta peça específica.
    pub custo_unitario: Preco,
    /// O estado atual.
    pub estado: EstadoLote,
    /// Quando o lote foi criado.
    pub criado_em: Instante,
    /// O histórico de movimentações deste lote — entrada, e em qual OS/venda/aparelho saiu,
    /// quando (mais recente primeiro).
    pub movimentos: Vec<Movimento>,
}

/// Busca o detalhe completo de um lote pelo código digitado.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn detalhe_do_lote_por_codigo(
    conexao: &Connection,
    empresa: Id,
    codigo: &str,
) -> Resultado<Option<DetalheLote>> {
    let Some(lote) = buscar_lote_por_codigo(conexao, empresa, codigo)? else {
        return Ok(None);
    };
    let produto = produto_de_por_id(conexao, lote.produto)?;
    let (produto_nome, localizacao_fisica) = produto.map_or_else(
        || (String::new(), None),
        |p| (p.nome, p.localizacao_fisica),
    );
    let local_nome = local_nome_de(conexao, lote.local)?;
    let aparelho_origem = match lote.aparelho_origem {
        Some(id) => buscar_aparelho_origem(conexao, id)?.map(|a| AparelhoOrigemResumo {
            id: a.id,
            descricao: a.descricao,
            identificador: a.identificador,
            custo_aquisicao: a.custo_aquisicao,
            adquirido_em: a.adquirido_em,
        }),
        None => None,
    };
    let quantidade_atual = saldo_do_lote(conexao, lote.id)?;
    let movimentos = movimentos_do_lote(conexao, lote.id)?;
    Ok(Some(DetalheLote {
        lote: lote.id,
        codigo: lote.codigo,
        produto: lote.produto,
        produto_nome,
        local: lote.local,
        local_nome,
        localizacao_fisica,
        origem: lote.origem,
        fornecedor: lote.fornecedor,
        aparelho_origem,
        fabricacao: lote.fabricacao,
        validade: lote.validade,
        quantidade_inicial: lote.quantidade_inicial,
        quantidade_atual,
        custo_unitario: lote.custo_unitario,
        estado: lote.estado,
        criado_em: lote.criado_em,
        movimentos,
    }))
}

fn produto_de_por_id(conexao: &Connection, id: Id) -> Resultado<Option<Produto>> {
    conexao
        .query_row(
            "SELECT id, empresa, grupo_produto, nome, ncm, cest, codigo_barras, fabricante,
                    codigo_fabricante, categoria_tecnica, especificacao_tecnica,
                    compatibilidade, garantia_fornecedor_dias, localizacao_fisica,
                    controla_grade, controla_lote, controla_validade, unidade_padrao,
                    ponto_pedido, estoque_minimo, estoque_maximo, ativo, versao
             FROM estoque_produto WHERE id = ?1",
            [blob(id)],
            produto_de_linha,
        )
        .optional()
        .map_err(persist)
}

fn local_nome_de(conexao: &Connection, id: Id) -> Resultado<String> {
    conexao
        .query_row(
            "SELECT nome FROM estoque_local WHERE id = ?1",
            [blob(id)],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(persist)
        .map(Option::unwrap_or_default)
}

/// Busca o detalhe completo de um lote/peça rastreável pelo código digitado (o post-it).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetalheDoLotePorCodigo {
    /// O código digitado.
    pub codigo: String,
}

impl Consulta for DetalheDoLotePorCodigo {
    type Saida = Option<DetalheLote>;
    const PERMISSAO: &'static str = "estoque.lote.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        detalhe_do_lote_por_codigo(conexao, ctx.empresa, &self.codigo)
    }
}

/// Um lote/peça disponível de um produto — o suficiente para listar as opções ao aplicar uma
/// peça sem o técnico precisar digitar o código (ele pode escolher da lista quando souber o
/// produto, ou digitar o código direto quando só tem o post-it em mãos).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemLoteDisponivel {
    /// O lote.
    pub id: Id,
    /// O código (post-it).
    pub codigo: String,
    /// O local onde está.
    pub local: Id,
    /// Quantidade ainda disponível.
    pub quantidade_disponivel: Quantidade,
    /// O custo unitário desta peça específica.
    pub custo_unitario: Preco,
    /// De onde veio.
    pub origem: OrigemLote,
}

/// Lista os lotes de um produto que ainda têm saldo disponível e não estão esgotados.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn lotes_disponiveis_do_produto(
    conexao: &Connection,
    produto: Id,
) -> Resultado<Vec<ItemLoteDisponivel>> {
    let mut stmt = conexao
        .prepare(&format!(
            "SELECT {COLUNAS_LOTE} FROM estoque_lote WHERE produto = ?1 AND estado != 'Esgotado' \
             ORDER BY criado_em ASC"
        ))
        .map_err(persist)?;
    let linhas = stmt
        .query_map([blob(produto)], crate::repositorio::lote_de_linha)
        .map_err(persist)?;
    let lotes = linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)?;
    let mut itens = Vec::with_capacity(lotes.len());
    for lote in lotes {
        let disponivel = saldo_do_lote(conexao, lote.id)?;
        if disponivel.e_positiva() {
            itens.push(ItemLoteDisponivel {
                id: lote.id,
                codigo: lote.codigo,
                local: lote.local,
                quantidade_disponivel: disponivel,
                custo_unitario: lote.custo_unitario,
                origem: lote.origem,
            });
        }
    }
    Ok(itens)
}

/// Lista os lotes disponíveis (com saldo) de um produto.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LotesDisponiveisDoProduto {
    /// O produto.
    pub produto: Id,
}

impl Consulta for LotesDisponiveisDoProduto {
    type Saida = Vec<ItemLoteDisponivel>;
    const PERMISSAO: &'static str = "estoque.lote.ver";

    fn executar(self, _ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        lotes_disponiveis_do_produto(conexao, self.produto)
    }
}
