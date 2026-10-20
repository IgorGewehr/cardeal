//! As consultas do módulo de compras — leitura autorizada sobre `compras_nota_entrada`.
//! `docs/modulos/compras.md` §10.
//!
//! Auto-contida (helpers locais de `blob`/`id_de`/`persist`) para não depender da
//! visibilidade dos helpers de `repositorio.rs`.

#[cfg(feature = "sqlite")]
use cardeal_kernel::{CodigoErro, Erro, Resultado};
use cardeal_kernel::{Data, Dinheiro, Id};
use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use cardeal_kernel::{Preco, Quantidade};

#[cfg(feature = "sqlite")]
use crate::nota::EstadoCasamento;
use crate::nota::EstadoNotaEntrada;
use crate::nota::ItemNotaEntrada;
use crate::nota::ParcelaCompra;

#[cfg(feature = "sqlite")]
fn blob(id: Id) -> Vec<u8> {
    id.em_bytes().to_vec()
}

#[cfg(feature = "sqlite")]
fn id_de(bytes: Vec<u8>) -> Id {
    Id::de_bytes(bytes.try_into().unwrap_or([0u8; 16]))
}

#[allow(clippy::needless_pass_by_value)] // usado como `map_err(persist)`
#[cfg(feature = "sqlite")]
fn persist(e: rusqlite::Error) -> Erro {
    Erro::novo(CodigoErro::FALHA_INTERNA, format!("compras/SQL: {e}"))
}

#[cfg(feature = "sqlite")]
fn estado_de(s: &str) -> EstadoNotaEntrada {
    match s {
        "Conferida" => EstadoNotaEntrada::Conferida,
        "Confirmada" => EstadoNotaEntrada::Confirmada,
        "Devolvida" => EstadoNotaEntrada::Devolvida,
        _ => EstadoNotaEntrada::AConferir,
    }
}

#[cfg(feature = "sqlite")]
fn casamento_de(s: &str) -> EstadoCasamento {
    match s {
        "Casado" => EstadoCasamento::Casado,
        "SugestaoForte" => EstadoCasamento::SugestaoForte,
        _ => EstadoCasamento::NaoCasado,
    }
}

/// Uma nota de entrada na lista de Compras — cabeçalho suficiente para a grade.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemNota {
    /// A nota.
    pub nota: Id,
    /// O fornecedor.
    pub fornecedor: Id,
    /// Número da nota.
    pub numero: String,
    /// Série.
    pub serie: String,
    /// Data de emissão.
    pub data_emissao: Data,
    /// Valor total da nota.
    pub valor_total: Dinheiro,
    /// O estado atual.
    pub estado: EstadoNotaEntrada,
    /// Quantos itens a nota tem.
    pub itens: u32,
    /// O aviso para quem confere (CFOP que não é venda), se houver.
    pub alerta: Option<String>,
}

/// Lista as notas de entrada da empresa, mais recentes primeiro. sem corte silencioso — teto de segurança `cardeal_modkit::TETO_LISTA_COMPLETA` (a lista paginada para a rede é a `.v2`)
/// (`docs/09-protocolo-api.md` §5).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotasRecentes;

impl Consulta for NotasRecentes {
    type Saida = Vec<ItemNota>;
    const PERMISSAO: &'static str = "compras.entrada.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let mut stmt = conexao
            .prepare(
                "SELECT n.id, n.fornecedor, n.numero, n.serie, n.data_emissao, n.valor_total, n.estado,
                        (SELECT COUNT(*) FROM compras_item_nota_entrada i WHERE i.nota_entrada = n.id),
                        n.alerta
                 FROM compras_nota_entrada n
                 WHERE n.empresa = ?1
                 ORDER BY n.data_emissao DESC, n.id DESC
                 LIMIT 50000",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map([blob(ctx.empresa)], |r| {
                Ok(ItemNota {
                    nota: id_de(r.get::<_, Vec<u8>>(0)?),
                    fornecedor: id_de(r.get::<_, Vec<u8>>(1)?),
                    numero: r.get(2)?,
                    serie: r.get(3)?,
                    data_emissao: Data::de_dias(i32::try_from(r.get::<_, i64>(4)?).unwrap_or(0)),
                    valor_total: Dinheiro::centavos(r.get::<_, i64>(5)?),
                    estado: estado_de(&r.get::<_, String>(6)?),
                    itens: r.get::<_, i64>(7)?.try_into().unwrap_or(0),
                    alerta: r.get(8)?,
                })
            })
            .map_err(persist)?;
        linhas
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)
    }
}

/// Os itens de uma nota de entrada — para a tela mostrar o casamento e permitir vincular.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItensDaNota {
    /// A nota.
    pub nota: Id,
}

impl Consulta for ItensDaNota {
    type Saida = Vec<ItemNotaEntrada>;
    const PERMISSAO: &'static str = "compras.entrada.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, _ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let mut stmt = conexao
            .prepare(
                "SELECT id, nota_entrada, produto_casado, codigo_fornecedor,
                        descricao_fornecedor, ncm, quantidade, valor_unitario, valor_rateio,
                        estado_casamento, codigo_barras, valor_desconto, valor_impostos
                 FROM compras_item_nota_entrada WHERE nota_entrada = ?1 ORDER BY rowid",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map([blob(self.nota)], |r| {
                Ok(ItemNotaEntrada {
                    id: id_de(r.get::<_, Vec<u8>>(0)?),
                    nota_entrada: id_de(r.get::<_, Vec<u8>>(1)?),
                    produto_casado: r.get::<_, Option<Vec<u8>>>(2)?.map(id_de),
                    codigo_fornecedor: r.get(3)?,
                    descricao_fornecedor: r.get(4)?,
                    ncm: r.get(5)?,
                    quantidade: Quantidade::interna(r.get::<_, i64>(6)?),
                    valor_unitario: Preco::interna(r.get::<_, i64>(7)?),
                    valor_rateio: cardeal_kernel::Dinheiro::centavos(r.get::<_, i64>(8)?),
                    estado_casamento: casamento_de(&r.get::<_, String>(9)?),
                    codigo_barras: r.get(10)?,
                    valor_desconto: cardeal_kernel::Dinheiro::centavos(r.get::<_, i64>(11)?),
                    valor_impostos: cardeal_kernel::Dinheiro::centavos(r.get::<_, i64>(12)?),
                })
            })
            .map_err(persist)?;
        linhas
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)
    }
}

/// As parcelas da nota: as duplicatas do XML ou as usadas na confirmação — vazio se a nota
/// não traz cobrança. Para a tela preencher as parcelas antes de confirmar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParcelasDaNota {
    /// A nota.
    pub nota: Id,
}

impl Consulta for ParcelasDaNota {
    type Saida = Vec<ParcelaCompra>;
    const PERMISSAO: &'static str = "compras.entrada.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, _ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        crate::repositorio::parcelas_da_nota(conexao, self.nota)
    }
}

/// Uma linha de estoque que o estorno tira: o produto, quanto entrou pela nota e quanto há no
/// local agora.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemDoEstorno {
    /// O produto.
    pub produto: String,
    /// Quanto entrou pela nota (e não foi desfeito).
    pub entrou: cardeal_kernel::Quantidade,
    /// Quanto há no local agora.
    pub no_local: cardeal_kernel::Quantidade,
}

impl ItemDoEstorno {
    /// Se a mercadoria ainda está toda no local — sem isso a entrada não se estorna.
    #[must_use]
    pub fn inteiro(&self) -> bool {
        self.no_local >= self.entrou
    }
}

/// O que o estorno da entrada vai fazer — para a tela mostrar antes de pedir o motivo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviaEstornoEntrada {
    /// O que sai do estoque.
    pub itens: Vec<ItemDoEstorno>,
    /// O valor da conta a pagar que será cancelada, se há.
    pub titulo: Option<Dinheiro>,
    /// Quanto dela já foi pago (o estorno exige estornar também).
    pub pago: Dinheiro,
}

impl PreviaEstornoEntrada {
    /// Por que o estorno não dá, se não dá: a mercadoria que já saiu do local.
    #[must_use]
    pub fn bloqueio(&self) -> Option<String> {
        self.itens.iter().find(|i| !i.inteiro()).map(|i| {
            format!(
                "{}: entraram {} e só restam {} no local — a mercadoria já foi vendida ou \
                 movida, e a entrada não se estorna.",
                i.produto,
                i.entrou.formatar(0),
                i.no_local.max(cardeal_kernel::Quantidade::ZERO).formatar(0)
            )
        })
    }
}

/// A prévia do estorno da entrada de uma nota ([`PreviaEstornoEntrada`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviaDoEstornoDaEntrada {
    /// A nota.
    pub nota: Id,
}

impl Consulta for PreviaDoEstornoDaEntrada {
    type Saida = PreviaEstornoEntrada;
    const PERMISSAO: &'static str = "compras.entrada.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let itens =
            mod_estoque::entradas_vivas_da_origem(conexao, ctx.empresa, "compras", self.nota)?
                .into_iter()
                .map(|v| ItemDoEstorno {
                    produto: v.nome,
                    entrou: v.quantidade,
                    no_local: v.disponivel,
                })
                .collect();
        let situacao = mod_financeiro::situacao_do_titulo_da_origem(
            conexao,
            ctx.empresa,
            "compras",
            self.nota,
        )?;
        Ok(PreviaEstornoEntrada {
            itens,
            titulo: situacao.map(|(valor, _)| valor),
            pago: situacao.map_or(Dinheiro::ZERO, |(_, pago)| pago),
        })
    }
}
