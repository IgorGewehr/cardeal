//! A listagem paginada de parcelas (`financeiro.parcelas_a_receber.v2` /
//! `financeiro.parcelas_a_pagar.v2`) — o caminho da rede.
//!
//! Aqui uma lista incompleta é um **número errado**: por isso a primeira página traz também a
//! contagem e a soma dos saldos de **tudo** o que casa com o filtro, calculadas no SQL — a tela
//! nunca soma uma página achando que é o total. Cursor `(vencimento, id)`.

use cardeal_kernel::{Dinheiro, Periodo};
use cardeal_modkit::{Consulta, Pagina, PedidoPagina};
use serde::{Deserialize, Serialize};

use crate::consultas::ItemTituloEmAberto;

#[cfg(feature = "sqlite")]
use crate::titulo::EspecieTitulo;
#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;

/// Que parcelas entram.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SituacaoParcelas {
    /// Só `Aberta`/`Parcial` — o que ainda há para receber/pagar.
    EmAberto,
    /// Qualquer estado (inclusive quitadas e canceladas).
    Todas,
}

/// O filtro comum às duas espécies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FiltroParcelas {
    /// Em aberto ou todas.
    pub situacao: SituacaoParcelas,
    /// Vencimento dentro do período (inclusive), se informado.
    pub periodo: Option<Periodo>,
    /// A página.
    pub pagina: PedidoPagina,
}

/// Uma página de parcelas, com os totais do filtro inteiro na primeira.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginaParcelas {
    /// A página.
    pub pagina: Pagina<ItemTituloEmAberto>,
    /// Soma dos saldos (`valor - baixado`) de tudo o que casa — só na primeira página.
    pub saldo_total: Option<Dinheiro>,
}

/// Parcelas a receber, por vencimento.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListarParcelasAReceber(pub FiltroParcelas);

/// Parcelas a pagar, por vencimento.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListarParcelasAPagar(pub FiltroParcelas);

impl Consulta for ListarParcelasAReceber {
    type Saida = PaginaParcelas;
    const PERMISSAO: &'static str = "financeiro.receber.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        listar(conexao, ctx.empresa, EspecieTitulo::Receber, &self.0)
    }
}

impl Consulta for ListarParcelasAPagar {
    type Saida = PaginaParcelas;
    const PERMISSAO: &'static str = "financeiro.pagar.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        listar(conexao, ctx.empresa, EspecieTitulo::Pagar, &self.0)
    }
}

/// Filtro comum: `?1` empresa, `?2` espécie, `?3` só em aberto (0/1), `?4`/`?5` período.
#[cfg(feature = "sqlite")]
const FILTRO: &str = "FROM financeiro_parcela p
     JOIN financeiro_titulo t ON t.id = p.titulo
     WHERE p.empresa = ?1 AND t.especie = ?2
       AND (?3 = 0 OR p.estado IN ('Aberta','Parcial'))
       AND (?4 IS NULL OR p.vencimento BETWEEN ?4 AND ?5)";

#[cfg(feature = "sqlite")]
fn listar(
    conexao: &Connection,
    empresa: cardeal_kernel::Id,
    especie: EspecieTitulo,
    f: &FiltroParcelas,
) -> Resultado<PaginaParcelas> {
    use crate::consultas::item_parcela_de_linha;
    use crate::repositorio::{blob, especie_txt, persist};
    use rusqlite::params;

    let so_aberto = i64::from(f.situacao == SituacaoParcelas::EmAberto);
    let (de, ate) = f
        .periodo
        .map(|p| (i64::from(p.de.em_dias()), i64::from(p.ate.em_dias())))
        .unzip();
    let depois: Option<(i64, Vec<u8>)> = f.pagina.chave()?;
    let (venc_apos, id_apos) = depois.unzip();

    let sql = format!(
        "SELECT p.id, p.titulo, p.numero, t.contraparte_tipo, t.contraparte_id,
                p.vencimento, p.valor, p.valor_baixado, p.estado,
                t.observacao, t.origem_modulo, t.origem_id, t.categoria
         {FILTRO}
           AND (?6 IS NULL OR p.vencimento > ?6 OR (p.vencimento = ?6 AND p.id > ?7))
         ORDER BY p.vencimento, p.id
         LIMIT ?8"
    );
    let linhas = conexao
        .prepare_cached(&sql)
        .map_err(persist)?
        .query_map(
            params![
                blob(empresa),
                especie_txt(especie),
                so_aberto,
                de,
                ate,
                venc_apos,
                id_apos,
                f.pagina.linhas_sql()
            ],
            |r| item_parcela_de_linha(r, especie),
        )
        .map_err(persist)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)?;

    let (total, saldo_total) = if f.pagina.apos.is_none() {
        let (n, saldo): (i64, i64) = conexao
            .prepare_cached(&format!(
                "SELECT COUNT(*), COALESCE(SUM(p.valor - p.valor_baixado), 0) {FILTRO}"
            ))
            .map_err(persist)?
            .query_row(
                params![blob(empresa), especie_txt(especie), so_aberto, de, ate],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(persist)?;
        (
            Some(u64::try_from(n).unwrap_or(0)),
            Some(Dinheiro::centavos(saldo)),
        )
    } else {
        (None, None)
    };
    Ok(PaginaParcelas {
        pagina: Pagina::montar(
            linhas,
            &f.pagina,
            |i: &ItemTituloEmAberto| {
                (
                    i64::from(i.vencimento.em_dias()),
                    i.parcela.em_bytes().to_vec(),
                )
            },
            total,
        ),
        saldo_total,
    })
}
