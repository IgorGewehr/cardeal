//! A listagem paginada de pessoas (`clientes.pessoas.v2`) — o caminho da rede: página por
//! cursor `(nome, id)`, busca no servidor, total só na primeira página. A `v1`
//! ([`crate::PessoasPorPapel`]) continua para o desktop até a migração das telas.

use cardeal_modkit::{Consulta, Pagina, PedidoPagina};
use serde::{Deserialize, Serialize};

use crate::consultas::ItemPessoa;
use crate::pessoa::Papel;

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::{params, Connection, OptionalExtension};

/// Pessoas ativas com um papel, por nome, uma página por vez.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListarPessoas {
    /// O papel (`Cliente`, `Fornecedor`…).
    pub papel: Papel,
    /// Busca por nome, documento ou telefone (pelos dígitos).
    pub busca: Option<String>,
    /// A página.
    pub pagina: PedidoPagina,
}

/// O filtro comum à página e ao total. Parâmetros: `?1` empresa, `?2` papel, `?3` termo,
/// `?4` dígitos do telefone.
#[cfg(feature = "sqlite")]
const FILTRO: &str = "FROM clientes_pessoa p
     JOIN clientes_papel cp ON cp.pessoa = p.id
     WHERE p.empresa = ?1 AND cp.papel = ?2 AND cp.ativo = 1 AND p.estado = 'Ativa'
       AND (?3 IS NULL OR p.nome LIKE ?3
            OR EXISTS (SELECT 1 FROM clientes_documento d WHERE d.pessoa = p.id AND d.numero LIKE ?3)
            OR (?4 IS NOT NULL AND EXISTS (
                SELECT 1 FROM clientes_contato c WHERE c.pessoa = p.id AND c.valor LIKE ?4)))";

impl Consulta for ListarPessoas {
    type Saida = Pagina<ItemPessoa>;
    const PERMISSAO: &'static str = "clientes.pessoa.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        use crate::repositorio::{blob, id_de, papel_txt, persist};

        let bruto = self
            .busca
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty());
        let termo = bruto.map(|t| format!("%{t}%"));
        let digitos = bruto
            .map(cardeal_kernel::texto::somente_digitos)
            .filter(|d| d.len() >= 4)
            .map(|d| format!("%{d}%"));
        let depois: Option<(String, Vec<u8>)> = self.pagina.chave()?;
        let (nome_apos, id_apos) = depois.unzip();

        let sql = format!(
            "SELECT p.id, p.nome,
                    (SELECT d.numero FROM clientes_documento d WHERE d.pessoa = p.id ORDER BY d.rowid LIMIT 1),
                    (SELECT c.valor FROM clientes_contato c
                      WHERE c.pessoa = p.id AND c.tipo IN ('Whatsapp', 'Celular', 'Telefone')
                      ORDER BY c.principal DESC, c.rowid LIMIT 1)
             {FILTRO}
               AND (?5 IS NULL OR p.nome > ?5 OR (p.nome = ?5 AND p.id > ?6))
             ORDER BY p.nome, p.id
             LIMIT ?7"
        );
        let mut stmt = conexao.prepare_cached(&sql).map_err(persist)?;
        let linhas = stmt
            .query_map(
                params![
                    blob(ctx.organizacao),
                    papel_txt(self.papel),
                    termo,
                    digitos,
                    nome_apos,
                    id_apos,
                    self.pagina.linhas_sql()
                ],
                |r| {
                    Ok(ItemPessoa {
                        pessoa: id_de(r.get::<_, Vec<u8>>(0)?),
                        nome: r.get(1)?,
                        documento: r.get(2)?,
                        telefone: r.get(3)?,
                    })
                },
            )
            .map_err(persist)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)?;

        let total = if self.pagina.apos.is_none() {
            let n: i64 = conexao
                .prepare_cached(&format!("SELECT COUNT(*) {FILTRO}"))
                .map_err(persist)?
                .query_row(
                    params![blob(ctx.organizacao), papel_txt(self.papel), termo, digitos],
                    |r| r.get(0),
                )
                .map_err(persist)?;
            Some(u64::try_from(n).unwrap_or(0))
        } else {
            None
        };
        Ok(Pagina::montar(
            linhas,
            &self.pagina,
            |p| (p.nome.clone(), p.pessoa.em_bytes().to_vec()),
            total,
        ))
    }
}

/// As pessoas ativas cujo nome casa com `termo` (por palavras, sem acento — o mesmo critério
/// das telas). Para outro módulo resolver "OS do João" sem ler `clientes_*` direto.
///
/// # Errors
/// Falha do SQLite.
#[cfg(feature = "sqlite")]
pub fn pessoas_cujo_nome_casa(
    conexao: &Connection,
    empresa: cardeal_kernel::Id,
    termo: &str,
) -> Resultado<Vec<cardeal_kernel::Id>> {
    use crate::repositorio::{blob, id_de, persist};
    let mut stmt = conexao
        .prepare_cached(
            "SELECT id, nome FROM clientes_pessoa WHERE empresa = ?1 AND estado = 'Ativa'",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map([blob(empresa)], |r| {
            Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(persist)?;
    let mut ids = Vec::new();
    for linha in linhas {
        let (id, nome) = linha.map_err(persist)?;
        if cardeal_kernel::texto::casa_por_palavras(&nome, termo) {
            ids.push(id_de(id));
        }
    }
    Ok(ids)
}

/// Os nomes de várias pessoas de uma vez (para rotular uma página de OS, títulos…).
///
/// # Errors
/// Falha do SQLite.
#[cfg(feature = "sqlite")]
pub fn nomes_das_pessoas(
    conexao: &Connection,
    ids: &[cardeal_kernel::Id],
) -> Resultado<std::collections::HashMap<cardeal_kernel::Id, String>> {
    use crate::repositorio::{blob, persist};
    let mut stmt = conexao
        .prepare_cached("SELECT nome FROM clientes_pessoa WHERE id = ?1")
        .map_err(persist)?;
    let mut nomes = std::collections::HashMap::with_capacity(ids.len());
    for id in ids {
        if nomes.contains_key(id) {
            continue;
        }
        if let Some(nome) = stmt
            .query_row([blob(*id)], |r| r.get::<_, String>(0))
            .optional()
            .map_err(persist)?
        {
            nomes.insert(*id, nome);
        }
    }
    Ok(nomes)
}
