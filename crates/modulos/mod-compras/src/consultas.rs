//! As consultas do módulo de compras — leitura autorizada sobre `compras_nota_entrada`.
//! `docs/modulos/compras.md` §10.
//!
//! Auto-contida (helpers locais de `blob`/`id_de`/`persist`) para não depender da
//! visibilidade dos helpers de `repositorio.rs`.

use cardeal_kernel::{CodigoErro, Data, Dinheiro, Erro, Id, Resultado};
use cardeal_modkit::{Consulta, Ctx};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::nota::EstadoNotaEntrada;

fn blob(id: Id) -> Vec<u8> {
    id.em_bytes().to_vec()
}

fn id_de(bytes: Vec<u8>) -> Id {
    Id::de_bytes(bytes.try_into().unwrap_or([0u8; 16]))
}

#[allow(clippy::needless_pass_by_value)] // usado como `map_err(persist)`
fn persist(e: rusqlite::Error) -> Erro {
    Erro::novo(CodigoErro::FALHA_INTERNA, format!("compras/SQL: {e}"))
}

fn estado_de(s: &str) -> EstadoNotaEntrada {
    match s {
        "Conferida" => EstadoNotaEntrada::Conferida,
        "Confirmada" => EstadoNotaEntrada::Confirmada,
        "Devolvida" => EstadoNotaEntrada::Devolvida,
        _ => EstadoNotaEntrada::AConferir,
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
}

/// Lista as notas de entrada da empresa, mais recentes primeiro. Teto de 200 linhas
/// (`docs/09-protocolo-api.md` §5).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotasRecentes;

impl Consulta for NotasRecentes {
    type Saida = Vec<ItemNota>;
    const PERMISSAO: &'static str = "compras.entrada.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let mut stmt = conexao
            .prepare(
                "SELECT n.id, n.fornecedor, n.numero, n.serie, n.data_emissao, n.valor_total, n.estado,
                        (SELECT COUNT(*) FROM compras_item_nota_entrada i WHERE i.nota_entrada = n.id)
                 FROM compras_nota_entrada n
                 WHERE n.empresa = ?1
                 ORDER BY n.data_emissao DESC, n.id DESC
                 LIMIT 200",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map([blob(ctx.empresa)], |r| {
                Ok(ItemNota {
                    nota: id_de(r.get::<_, Vec<u8>>(0)?),
                    fornecedor: id_de(r.get::<_, Vec<u8>>(1)?),
                    numero: r.get(2)?,
                    serie: r.get(3)?,
                    data_emissao: Data::de_dias(
                        i32::try_from(r.get::<_, i64>(4)?).unwrap_or(0),
                    ),
                    valor_total: Dinheiro::centavos(r.get::<_, i64>(5)?),
                    estado: estado_de(&r.get::<_, String>(6)?),
                    itens: r.get::<_, i64>(7)?.try_into().unwrap_or(0),
                })
            })
            .map_err(persist)?;
        linhas.collect::<rusqlite::Result<Vec<_>>>().map_err(persist)
    }
}
