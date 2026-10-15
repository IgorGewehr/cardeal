//! O que um produto leva para a nota fiscal: nome, NCM, CEST, GTIN, unidade e o **perfil fiscal
//! efetivo** (os campos próprios por cima da cadeia de grupos) — a consulta pública que
//! `mod-fiscal` usa para montar a NFC-e de uma venda (`docs/contratos-internos.md` §7 regra 2:
//! nada de ler tabela de outro módulo).

use std::collections::HashMap;

use cardeal_kernel::{CodigoErro, Erro, Id, Resultado};
use rusqlite::{params, Connection, OptionalExtension};

use crate::fiscal::{perfil_do_produto, CamposFiscais};
use crate::hierarquia::{Arvore, NoGrupo};

/// Um produto como a nota o vê.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProdutoParaNota {
    /// O produto.
    pub id: Id,
    /// O nome (vai como descrição).
    pub nome: String,
    /// NCM (vazio = pendente).
    pub ncm: String,
    /// CEST.
    pub cest: Option<String>,
    /// GTIN/EAN.
    pub gtin: Option<String>,
    /// A sigla da unidade padrão ("UN", "KG").
    pub unidade: String,
    /// O perfil fiscal efetivo.
    pub fiscal: CamposFiscais,
}

#[allow(clippy::needless_pass_by_value)]
fn falha(e: rusqlite::Error) -> Erro {
    Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string())
}

/// Os produtos pedidos que existem na organização, por id.
///
/// # Errors
/// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn produtos_para_nota(
    conn: &Connection,
    organizacao: Id,
    produtos: &[Id],
) -> Resultado<HashMap<Id, ProdutoParaNota>> {
    let grupos = crate::repositorio_preco::perfis_dos_grupos(conn, organizacao)?;
    let arvore = Arvore::de_nos(
        &grupos
            .iter()
            .map(|g| NoGrupo {
                id: g.id,
                pai: g.pai,
            })
            .collect::<Vec<_>>(),
    );
    let mut stmt = conn
        .prepare_cached(
            "SELECT p.nome, p.ncm, p.cest, p.codigo_barras, u.sigla, p.grupo_produto,
                    p.perfil_fiscal
             FROM estoque_produto p JOIN estoque_unidade u ON u.id = p.unidade_padrao
             WHERE p.empresa = ?1 AND p.id = ?2",
        )
        .map_err(falha)?;
    let mut saida = HashMap::new();
    for &id in produtos {
        if saida.contains_key(&id) {
            continue;
        }
        let linha = stmt
            .query_row(
                params![organizacao.em_bytes().as_slice(), id.em_bytes().as_slice()],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Option<String>>(2)?,
                        r.get::<_, Option<String>>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, Vec<u8>>(5)?,
                        r.get::<_, Option<Vec<u8>>>(6)?,
                    ))
                },
            )
            .optional()
            .map_err(falha)?;
        let Some((nome, ncm, cest, gtin, unidade, grupo, proprio)) = linha else {
            continue;
        };
        let grupo = <[u8; 16]>::try_from(grupo.as_slice())
            .map(Id::de_bytes)
            .map_err(|_| Erro::novo(CodigoErro::FALHA_INTERNA, "grupo com id inválido"))?;
        let cadeia: Vec<&CamposFiscais> = arvore
            .ancestrais(grupo)
            .iter()
            .filter_map(|g| grupos.iter().find(|x| x.id == *g).map(|x| &x.fiscal))
            .collect();
        let fiscal = perfil_do_produto(&CamposFiscais::de_blob(proprio.as_deref()), &cadeia);
        saida.insert(
            id,
            ProdutoParaNota {
                id,
                nome,
                ncm,
                cest: cest.filter(|c| !c.is_empty()),
                gtin: gtin.filter(|g| !g.is_empty()),
                unidade,
                fiscal,
            },
        );
    }
    Ok(saida)
}
