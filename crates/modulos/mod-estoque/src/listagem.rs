//! A listagem paginada de produtos (`estoque.produtos.v2`) — o caminho da rede: cursor
//! `(nome, id)`, busca no servidor (nome, código de barras ou NCM), total na primeira página.
//! O catálogo é da organização; o saldo, do CNPJ da sessão (ADR-0017).
//! A `v1` ([`crate::ProdutosComSaldo`]) continua para o desktop até a migração das telas.

use cardeal_modkit::{Consulta, Pagina, PedidoPagina};
use serde::{Deserialize, Serialize};

use crate::consultas::ItemProdutoComSaldo;

#[cfg(feature = "sqlite")]
use cardeal_kernel::{Preco, Quantidade, Resultado};
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::{params, Connection};

/// Produtos ativos com saldo, por nome, uma página por vez.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListarProdutos {
    /// Busca por nome (contém), código de barras ou NCM (exatos).
    pub busca: Option<String>,
    /// A página.
    pub pagina: PedidoPagina,
}

/// Filtro comum à página e ao total: `?1` empresa, `?2` termo `%…%`, `?3` termo exato.
#[cfg(feature = "sqlite")]
const FILTRO: &str = "WHERE p.empresa = ?1 AND p.ativo = 1
       AND (?2 IS NULL OR p.nome LIKE ?2 OR p.codigo_barras = ?3 OR p.ncm = ?3)";

impl Consulta for ListarProdutos {
    type Saida = Pagina<ItemProdutoComSaldo>;
    const PERMISSAO: &'static str = "estoque.produto.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        use crate::repositorio::{blob, id_de, persist};

        let bruto = self
            .busca
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty());
        let termo = bruto.map(|t| format!("%{t}%"));
        let exato = bruto.map(str::to_owned);
        let depois: Option<(String, Vec<u8>)> = self.pagina.chave()?;
        let (nome_apos, id_apos) = depois.unzip();
        let sql = format!(
            "SELECT p.id, p.nome, p.ncm,
                    COALESCE((SELECT SUM(s.quantidade_disponivel) FROM estoque_saldo_local s WHERE s.produto = p.id AND s.empresa = ?7), 0),
                    COALESCE((SELECT SUM(s.quantidade_reservada) FROM estoque_saldo_local s WHERE s.produto = p.id AND s.empresa = ?7), 0),
                    COALESCE((SELECT MAX(s.custo_medio) FROM estoque_saldo_local s WHERE s.produto = p.id AND s.empresa = ?7), 0),
                    p.codigo_barras
             FROM estoque_produto p
             {FILTRO}
               AND (?4 IS NULL OR p.nome > ?4 OR (p.nome = ?4 AND p.id > ?5))
             ORDER BY p.nome, p.id
             LIMIT ?6"
        );
        let linhas = conexao
            .prepare_cached(&sql)
            .map_err(persist)?
            .query_map(
                params![
                    blob(ctx.organizacao),
                    termo,
                    exato,
                    nome_apos,
                    id_apos,
                    self.pagina.linhas_sql(),
                    blob(ctx.empresa)
                ],
                |r| {
                    Ok(ItemProdutoComSaldo {
                        produto: id_de(r.get::<_, Vec<u8>>(0)?),
                        nome: r.get(1)?,
                        ncm: r.get(2)?,
                        disponivel: Quantidade::interna(r.get::<_, i64>(3)?),
                        reservado: Quantidade::interna(r.get::<_, i64>(4)?),
                        custo_medio: Preco::interna(r.get::<_, i64>(5)?),
                        codigo_barras: r.get(6)?,
                    })
                },
            )
            .map_err(persist)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)?;
        let total = if self.pagina.apos.is_none() {
            let n: i64 = conexao
                .prepare_cached(&format!("SELECT COUNT(*) FROM estoque_produto p {FILTRO}"))
                .map_err(persist)?
                .query_row(params![blob(ctx.organizacao), termo, exato], |r| r.get(0))
                .map_err(persist)?;
            Some(u64::try_from(n).unwrap_or(0))
        } else {
            None
        };
        Ok(Pagina::montar(
            linhas,
            &self.pagina,
            |p| (p.nome.clone(), p.produto.em_bytes().to_vec()),
            total,
        ))
    }
}
