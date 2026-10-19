//! O SQL do módulo.

use cardeal_fiscal::AmbienteFiscal;
use cardeal_kernel::{CodigoErro, Erro, Id, Instante, Resultado};
use rusqlite::{Connection, OptionalExtension};

use crate::tipos::CertificadoResumo;

#[allow(clippy::needless_pass_by_value)]
fn falha(e: rusqlite::Error) -> Erro {
    Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string())
}

/// Grava o certificado da empresa (substitui o anterior).
pub(crate) fn gravar_certificado(
    c: &Connection,
    empresa: Id,
    resumo: &CertificadoResumo,
    selado: &[u8],
    instalado_por: Id,
) -> Resultado<()> {
    c.execute(
        "INSERT INTO fiscal_certificado
           (empresa, titular, cnpj, emissor, numero_serie, valido_de, valido_ate, convertido,
            selado, instalado_em, instalado_por)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)
         ON CONFLICT(empresa) DO UPDATE SET
           titular = excluded.titular, cnpj = excluded.cnpj, emissor = excluded.emissor,
           numero_serie = excluded.numero_serie, valido_de = excluded.valido_de,
           valido_ate = excluded.valido_ate, convertido = excluded.convertido,
           selado = excluded.selado, instalado_em = excluded.instalado_em,
           instalado_por = excluded.instalado_por",
        rusqlite::params![
            empresa.em_bytes().as_slice(),
            resumo.titular,
            resumo.cnpj,
            resumo.emissor,
            resumo.numero_serie,
            resumo.valido_de.em_micros(),
            resumo.valido_ate.em_micros(),
            resumo.convertido,
            selado,
            resumo.instalado_em.em_micros(),
            instalado_por.em_bytes().as_slice(),
        ],
    )
    .map(|_| ())
    .map_err(falha)
}

/// O certificado instalado (só a parte pública).
pub(crate) fn certificado(c: &Connection, empresa: Id) -> Resultado<Option<CertificadoResumo>> {
    c.query_row(
        "SELECT titular, cnpj, emissor, numero_serie, valido_de, valido_ate, convertido,
                instalado_em
         FROM fiscal_certificado WHERE empresa = ?1",
        [empresa.em_bytes().as_slice()],
        |r| {
            Ok(CertificadoResumo {
                titular: r.get(0)?,
                cnpj: r.get(1)?,
                emissor: r.get(2)?,
                numero_serie: r.get(3)?,
                valido_de: Instante::de_micros(r.get(4)?),
                valido_ate: Instante::de_micros(r.get(5)?),
                convertido: r.get(6)?,
                instalado_em: Instante::de_micros(r.get(7)?),
            })
        },
    )
    .optional()
    .map_err(falha)
}

/// O selado do cofre (arquivo + senha cifrados).
pub(crate) fn selado(c: &Connection, empresa: Id) -> Resultado<Option<Vec<u8>>> {
    c.query_row(
        "SELECT selado FROM fiscal_certificado WHERE empresa = ?1",
        [empresa.em_bytes().as_slice()],
        |r| r.get(0),
    )
    .optional()
    .map_err(falha)
}

/// Remove o certificado da empresa. Devolve se havia um.
pub(crate) fn remover_certificado(c: &Connection, empresa: Id) -> Resultado<bool> {
    c.execute(
        "DELETE FROM fiscal_certificado WHERE empresa = ?1",
        [empresa.em_bytes().as_slice()],
    )
    .map(|n| n > 0)
    .map_err(falha)
}

/// O ambiente da empresa — homologação enquanto ninguém ligar a produção.
pub(crate) fn ambiente(c: &Connection, empresa: Id) -> Resultado<AmbienteFiscal> {
    let texto: Option<String> = c
        .query_row(
            "SELECT ambiente FROM fiscal_configuracao WHERE empresa = ?1",
            [empresa.em_bytes().as_slice()],
            |r| r.get(0),
        )
        .optional()
        .map_err(falha)?;
    Ok(match texto.as_deref() {
        Some("producao") => AmbienteFiscal::Producao,
        _ => AmbienteFiscal::Homologacao,
    })
}

/// Grava o ambiente.
pub(crate) fn gravar_ambiente(
    c: &Connection,
    empresa: Id,
    ambiente: AmbienteFiscal,
) -> Resultado<()> {
    let texto = match ambiente {
        AmbienteFiscal::Homologacao => "homologacao",
        AmbienteFiscal::Producao => "producao",
    };
    c.execute(
        "INSERT INTO fiscal_configuracao (empresa, ambiente) VALUES (?1, ?2)
         ON CONFLICT(empresa) DO UPDATE SET ambiente = excluded.ambiente",
        rusqlite::params![empresa.em_bytes().as_slice(), texto],
    )
    .map(|_| ())
    .map_err(falha)
}
