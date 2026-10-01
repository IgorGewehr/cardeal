//! O SQL do módulo — sobre as tabelas do núcleo. Funções livres sobre uma `Connection`, para
//! servir tanto o despacho (consultas/comandos) quanto o `MotorLocal` (que as chama direto
//! no primeiro acesso, antes de existir sessão).

use std::collections::HashMap;

use base64::Engine as _;
use cardeal_kernel::{CodigoErro, Erro, Id, Resultado};
use rusqlite::Connection;

use crate::tipos::{EmpresaResumo, IdentidadeVisual, PapelResumo, UsuarioResumo, TETO_LOGO_BYTES};

/// O engine base64 usado para guardar a logo em `nucleo_configuracao`.
const BASE64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

const ASSINATURA_PNG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

#[allow(clippy::needless_pass_by_value)]
fn falha(e: rusqlite::Error) -> Erro {
    Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string())
}

/// Os dados cadastrais.
///
/// # Errors
/// Falha do SQLite (inclusive base sem empresa).
pub fn dados(c: &Connection) -> Resultado<EmpresaResumo> {
    c.query_row(
        "SELECT razao_social, nome_fantasia, cnpj, regime, perfil FROM nucleo_empresa LIMIT 1",
        [],
        |r| {
            Ok(EmpresaResumo {
                razao_social: r.get(0)?,
                nome_fantasia: r.get(1)?,
                cnpj: r.get(2)?,
                regime: r.get(3)?,
                perfil: r.get(4)?,
            })
        },
    )
    .map_err(falha)
}

/// A identidade visual: `nucleo_empresa` + as chaves `empresa.*` de `nucleo_configuracao`.
///
/// # Errors
/// Falha do SQLite.
pub fn identidade(c: &Connection, empresa: Id) -> Resultado<IdentidadeVisual> {
    let base = dados(c)?;
    let mut stmt = c
        .prepare_cached(
            "SELECT chave, valor FROM nucleo_configuracao
             WHERE empresa = ?1 AND chave LIKE 'empresa.%'",
        )
        .map_err(falha)?;
    let config: HashMap<String, String> = stmt
        .query_map([empresa.em_bytes().as_slice()], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .map_err(falha)?
        .collect::<rusqlite::Result<_>>()
        .map_err(falha)?;
    let get = |k: &str| config.get(k).cloned().unwrap_or_default();
    let logo_png = config
        .get("empresa.logo_png_b64")
        .and_then(|b64| BASE64.decode(b64).ok())
        .filter(|v| !v.is_empty());
    Ok(IdentidadeVisual {
        razao_social: base.razao_social,
        nome_fantasia: base.nome_fantasia,
        cnpj: base.cnpj,
        endereco: get("empresa.endereco_exibicao"),
        telefone: get("empresa.telefone"),
        email: get("empresa.email"),
        site: get("empresa.site"),
        logo_png,
    })
}

/// Atualiza razão social, nome fantasia e regime.
///
/// # Errors
/// Falha do SQLite.
pub fn atualizar_dados(
    c: &Connection,
    razao_social: &str,
    nome_fantasia: &str,
    regime: &str,
) -> Resultado<()> {
    c.execute(
        "UPDATE nucleo_empresa SET razao_social = ?1, nome_fantasia = ?2, regime = ?3",
        rusqlite::params![razao_social, nome_fantasia, regime],
    )
    .map(|_| ())
    .map_err(falha)
}

/// Grava (ou atualiza) chaves de `nucleo_configuracao` da empresa.
///
/// # Errors
/// Falha do SQLite.
pub fn gravar_config(c: &Connection, empresa: Id, pares: &[(&str, &str)]) -> Resultado<()> {
    for (chave, valor) in pares {
        c.execute(
            "INSERT INTO nucleo_configuracao (empresa, chave, valor) VALUES (?1, ?2, ?3)
             ON CONFLICT(empresa, chave) DO UPDATE SET valor = excluded.valor",
            rusqlite::params![empresa.em_bytes().as_slice(), chave, valor],
        )
        .map_err(falha)?;
    }
    Ok(())
}

/// Confere que os bytes são um PNG dentro do teto.
///
/// # Errors
/// `VALOR_INVALIDO` se não é PNG ou passa de [`TETO_LOGO_BYTES`].
pub fn validar_logo(png: &[u8]) -> Resultado<()> {
    if png.len() > TETO_LOGO_BYTES {
        return Err(Erro::novo(
            CodigoErro::VALOR_INVALIDO,
            "a logo passa de 512 KB — use uma imagem menor",
        ));
    }
    if !png.starts_with(&ASSINATURA_PNG) {
        return Err(Erro::novo(
            CodigoErro::VALOR_INVALIDO,
            "a logo precisa ser um arquivo PNG",
        ));
    }
    Ok(())
}

/// Grava (`Some`) ou remove (`None`) a logo. Valida antes.
///
/// # Errors
/// Logo inválida; falha do SQLite.
pub fn gravar_logo(c: &Connection, empresa: Id, png: Option<&[u8]>) -> Resultado<()> {
    match png {
        Some(bytes) => {
            validar_logo(bytes)?;
            gravar_config(
                c,
                empresa,
                &[("empresa.logo_png_b64", &BASE64.encode(bytes))],
            )
        }
        None => c
            .execute(
                "DELETE FROM nucleo_configuracao
                 WHERE empresa = ?1 AND chave = 'empresa.logo_png_b64'",
                [empresa.em_bytes().as_slice()],
            )
            .map(|_| ())
            .map_err(falha),
    }
}

/// Todos os usuários (id, login, nome, ativo), por nome.
///
/// # Errors
/// Falha do SQLite.
pub fn usuarios(c: &Connection) -> Resultado<Vec<UsuarioResumo>> {
    let mut stmt = c
        .prepare_cached("SELECT id, login, nome, ativo FROM nucleo_usuario ORDER BY nome")
        .map_err(falha)?;
    let lista = stmt
        .query_map([], |r| {
            let id_bytes = r.get::<_, Vec<u8>>(0)?;
            Ok(UsuarioResumo {
                id: <[u8; 16]>::try_from(id_bytes).map_or(Id::NULO, Id::de_bytes),
                login: r.get(1)?,
                nome: r.get(2)?,
                ativo: r.get::<_, i64>(3)? != 0,
            })
        })
        .map_err(falha)?
        .collect::<rusqlite::Result<_>>()
        .map_err(falha);
    lista
}

/// Os papéis (nome, descrição, nº de permissões), de fábrica primeiro.
///
/// # Errors
/// Falha do SQLite.
pub fn papeis(c: &Connection) -> Resultado<Vec<PapelResumo>> {
    let mut stmt = c
        .prepare_cached(
            "SELECT p.nome, p.descricao, p.sistema,
                    (SELECT COUNT(*) FROM nucleo_papel_permissao pp WHERE pp.papel = p.id)
             FROM nucleo_papel p
             ORDER BY p.sistema DESC, p.nome",
        )
        .map_err(falha)?;
    let lista = stmt
        .query_map([], |r| {
            Ok(PapelResumo {
                nome: r.get(0)?,
                descricao: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                sistema: r.get::<_, i64>(2)? != 0,
                permissoes: usize::try_from(r.get::<_, i64>(3)?).unwrap_or(0),
            })
        })
        .map_err(falha)?
        .collect::<rusqlite::Result<_>>()
        .map_err(falha);
    lista
}
