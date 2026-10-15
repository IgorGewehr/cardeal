//! O SQL dos documentos fiscais, da numeração por série e da configuração da NFC-e.

use cardeal_fiscal::{AmbienteFiscal, ModeloFiscal};
use cardeal_kernel::{CodigoErro, Dinheiro, Erro, Id, Instante, Resultado};
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::documento::{DocumentoResumo, EstadoDocumento, Mudanca, ResumoFiscal};

#[allow(clippy::needless_pass_by_value)]
fn falha(e: rusqlite::Error) -> Erro {
    Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string())
}

fn texto_ambiente(a: AmbienteFiscal) -> &'static str {
    match a {
        AmbienteFiscal::Homologacao => "homologacao",
        AmbienteFiscal::Producao => "producao",
    }
}

fn id_de(bytes: &[u8]) -> Id {
    <[u8; 16]>::try_from(bytes).map_or(Id::de_bytes([0; 16]), Id::de_bytes)
}

/// A configuração da NFC-e gravada.
#[derive(Debug, Clone, Default)]
pub(crate) struct ConfigNfceGravada {
    pub emitir: bool,
    pub serie: u32,
    pub csc_id: Option<String>,
    pub csc_selado: Option<Vec<u8>>,
}

pub(crate) fn config_nfce(c: &Connection, empresa: Id) -> Resultado<ConfigNfceGravada> {
    Ok(c.query_row(
        "SELECT emitir_nfce, serie_nfce, csc_id, csc_selado FROM fiscal_configuracao
         WHERE empresa = ?1",
        [empresa.em_bytes().as_slice()],
        |r| {
            Ok(ConfigNfceGravada {
                emitir: r.get(0)?,
                serie: r.get(1)?,
                csc_id: r.get(2)?,
                csc_selado: r.get(3)?,
            })
        },
    )
    .optional()
    .map_err(falha)?
    .unwrap_or(ConfigNfceGravada {
        serie: 1,
        ..ConfigNfceGravada::default()
    }))
}

/// Grava a configuração (`csc_selado = None` mantém o que havia).
pub(crate) fn gravar_config_nfce(
    c: &Connection,
    empresa: Id,
    emitir: bool,
    serie: u32,
    csc_id: &str,
    csc_selado: Option<&[u8]>,
) -> Resultado<()> {
    c.execute(
        "INSERT INTO fiscal_configuracao (empresa, ambiente, emitir_nfce, serie_nfce, csc_id, csc_selado)
         VALUES (?1, 'homologacao', ?2, ?3, ?4, ?5)
         ON CONFLICT(empresa) DO UPDATE SET emitir_nfce = excluded.emitir_nfce,
           serie_nfce = excluded.serie_nfce, csc_id = excluded.csc_id,
           csc_selado = COALESCE(excluded.csc_selado, fiscal_configuracao.csc_selado)",
        params![empresa.em_bytes().as_slice(), emitir, serie, csc_id, csc_selado],
    )
    .map(|_| ())
    .map_err(falha)
}

/// O maior número já usado na série (no ambiente).
pub(crate) fn maior_numero(
    c: &Connection,
    empresa: Id,
    modelo: ModeloFiscal,
    serie: u32,
    ambiente: AmbienteFiscal,
) -> Resultado<Option<u64>> {
    c.query_row(
        "SELECT MAX(numero) FROM fiscal_documento
         WHERE empresa = ?1 AND modelo = ?2 AND serie = ?3 AND ambiente = ?4",
        params![
            empresa.em_bytes().as_slice(),
            modelo.codigo(),
            serie,
            texto_ambiente(ambiente)
        ],
        |r| r.get::<_, Option<i64>>(0),
    )
    .map(|n| n.and_then(|n| u64::try_from(n).ok()))
    .map_err(falha)
}

/// O próximo número da série: o gravado, ou o depois do maior usado, ou 1.
pub(crate) fn proximo_numero(
    c: &Connection,
    empresa: Id,
    modelo: ModeloFiscal,
    serie: u32,
    ambiente: AmbienteFiscal,
) -> Resultado<u64> {
    let gravado: Option<i64> = c
        .query_row(
            "SELECT proximo FROM fiscal_serie
             WHERE empresa = ?1 AND modelo = ?2 AND serie = ?3 AND ambiente = ?4",
            params![
                empresa.em_bytes().as_slice(),
                modelo.codigo(),
                serie,
                texto_ambiente(ambiente)
            ],
            |r| r.get(0),
        )
        .optional()
        .map_err(falha)?;
    let usado = maior_numero(c, empresa, modelo, serie, ambiente)?.unwrap_or(0);
    let gravado = gravado.and_then(|n| u64::try_from(n).ok()).unwrap_or(1);
    Ok(gravado.max(usado + 1))
}

/// Fixa o próximo número da série.
pub(crate) fn definir_proximo(
    c: &Connection,
    empresa: Id,
    modelo: ModeloFiscal,
    serie: u32,
    ambiente: AmbienteFiscal,
    proximo: u64,
) -> Resultado<()> {
    let proximo = i64::try_from(proximo)
        .map_err(|_| Erro::novo(CodigoErro::FORA_DA_FAIXA, "número grande demais"))?;
    c.execute(
        "INSERT INTO fiscal_serie (empresa, modelo, serie, ambiente, proximo) VALUES (?1,?2,?3,?4,?5)
         ON CONFLICT(empresa, modelo, serie, ambiente) DO UPDATE SET proximo = excluded.proximo",
        params![
            empresa.em_bytes().as_slice(),
            modelo.codigo(),
            serie,
            texto_ambiente(ambiente),
            proximo
        ],
    )
    .map(|_| ())
    .map_err(falha)
}

/// Reserva o próximo número (e avança a série).
pub(crate) fn reservar_numero(
    c: &Connection,
    empresa: Id,
    modelo: ModeloFiscal,
    serie: u32,
    ambiente: AmbienteFiscal,
) -> Resultado<u64> {
    let n = proximo_numero(c, empresa, modelo, serie, ambiente)?;
    definir_proximo(c, empresa, modelo, serie, ambiente, n + 1)?;
    Ok(n)
}

/// Um documento novo.
pub(crate) struct NovoDocumento<'a> {
    pub id: Id,
    pub empresa: Id,
    pub modelo: ModeloFiscal,
    pub serie: u32,
    pub numero: u64,
    pub ambiente: AmbienteFiscal,
    pub origem_modulo: &'a str,
    pub origem: Id,
    pub estado: EstadoDocumento,
    pub total: Dinheiro,
    pub nota: Option<Vec<u8>>,
    pub motivo: Option<String>,
    pub agora: Instante,
}

pub(crate) fn inserir_documento(c: &Connection, d: &NovoDocumento<'_>) -> Resultado<()> {
    c.execute(
        "INSERT INTO fiscal_documento
           (id, empresa, modelo, serie, numero, ambiente, origem_modulo, origem_id, estado, total,
            nota, motivo, criado_em, atualizado_em)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?13)",
        params![
            d.id.em_bytes().as_slice(),
            d.empresa.em_bytes().as_slice(),
            d.modelo.codigo(),
            d.serie,
            i64::try_from(d.numero).unwrap_or(i64::MAX),
            texto_ambiente(d.ambiente),
            d.origem_modulo,
            d.origem.em_bytes().as_slice(),
            d.estado.como_str(),
            d.total.em_centavos(),
            d.nota,
            d.motivo,
            d.agora.em_micros(),
        ],
    )
    .map(|_| ())
    .map_err(falha)
}

const COLUNAS: &str =
    "id, modelo, serie, numero, ambiente, estado, total, chave, protocolo, codigo,
                       motivo, origem_id, tentativas, criado_em, autorizado_em";

fn resumo_de(r: &Row<'_>) -> rusqlite::Result<DocumentoResumo> {
    Ok(DocumentoResumo {
        id: id_de(&r.get::<_, Vec<u8>>(0)?),
        modelo: if r.get::<_, String>(1)? == "55" {
            ModeloFiscal::Nfe
        } else {
            ModeloFiscal::Nfce
        },
        serie: r.get(2)?,
        numero: u64::try_from(r.get::<_, i64>(3)?).unwrap_or(0),
        ambiente: if r.get::<_, String>(4)? == "producao" {
            AmbienteFiscal::Producao
        } else {
            AmbienteFiscal::Homologacao
        },
        estado: EstadoDocumento::de_str(&r.get::<_, String>(5)?)
            .unwrap_or(EstadoDocumento::Pendente),
        total: Dinheiro::centavos(r.get(6)?),
        chave: r.get(7)?,
        protocolo: r.get(8)?,
        codigo: r.get(9)?,
        motivo: r.get(10)?,
        origem: id_de(&r.get::<_, Vec<u8>>(11)?),
        tentativas: r.get(12)?,
        criado_em: Instante::de_micros(r.get(13)?),
        autorizado_em: r.get::<_, Option<i64>>(14)?.map(Instante::de_micros),
    })
}

pub(crate) fn documento_da_origem(
    c: &Connection,
    empresa: Id,
    origem_modulo: &str,
    origem: Id,
) -> Resultado<Option<DocumentoResumo>> {
    c.query_row(
        &format!(
            "SELECT {COLUNAS} FROM fiscal_documento
             WHERE empresa = ?1 AND origem_modulo = ?2 AND origem_id = ?3"
        ),
        params![
            empresa.em_bytes().as_slice(),
            origem_modulo,
            origem.em_bytes().as_slice()
        ],
        resumo_de,
    )
    .optional()
    .map_err(falha)
}

/// Um documento com o que só o detalhe e o reenvio usam.
pub(crate) struct DocumentoGuardado {
    pub resumo: DocumentoResumo,
    /// A nota em postcard.
    pub nota: Option<Vec<u8>>,
    pub xml: Option<String>,
    pub origem_modulo: String,
}

/// O documento, a nota guardada e o XML autorizado.
pub(crate) fn documento(
    c: &Connection,
    empresa: Id,
    id: Id,
) -> Resultado<Option<DocumentoGuardado>> {
    c.query_row(
        &format!(
            "SELECT {COLUNAS}, nota, xml, origem_modulo FROM fiscal_documento
             WHERE empresa = ?1 AND id = ?2"
        ),
        params![empresa.em_bytes().as_slice(), id.em_bytes().as_slice()],
        |r| {
            Ok(DocumentoGuardado {
                resumo: resumo_de(r)?,
                nota: r.get(15)?,
                xml: r.get(16)?,
                origem_modulo: r.get(17)?,
            })
        },
    )
    .optional()
    .map_err(falha)
}

/// Os documentos, os mais novos primeiro.
pub(crate) fn documentos(
    c: &Connection,
    empresa: Id,
    estado: Option<EstadoDocumento>,
    limite: u32,
) -> Resultado<Vec<DocumentoResumo>> {
    let mut stmt = c
        .prepare(&format!(
            "SELECT {COLUNAS} FROM fiscal_documento
             WHERE empresa = ?1 AND (?2 IS NULL OR estado = ?2)
             ORDER BY criado_em DESC LIMIT ?3"
        ))
        .map_err(falha)?;
    let linhas = stmt
        .query_map(
            params![
                empresa.em_bytes().as_slice(),
                estado.map(EstadoDocumento::como_str),
                limite
            ],
            resumo_de,
        )
        .map_err(falha)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(falha);
    linhas
}

/// Um documento na fila de transmissão.
pub(crate) struct NaFila {
    pub id: Id,
    pub ambiente: AmbienteFiscal,
    pub estado: EstadoDocumento,
    pub tentativas: u32,
    pub chave: Option<String>,
    pub nota: Option<Vec<u8>>,
}

/// Os pendentes e incertos cuja hora de tentar chegou, os mais antigos primeiro.
pub(crate) fn fila(
    c: &Connection,
    empresa: Id,
    agora: Instante,
    limite: u32,
) -> Resultado<Vec<NaFila>> {
    let mut stmt = c
        .prepare(
            "SELECT id, estado, tentativas, chave, nota, ambiente FROM fiscal_documento
             WHERE empresa = ?1 AND estado IN ('Pendente','Incerta') AND proxima_tentativa <= ?2
             ORDER BY criado_em LIMIT ?3",
        )
        .map_err(falha)?;
    let linhas = stmt
        .query_map(
            params![empresa.em_bytes().as_slice(), agora.em_micros(), limite],
            |r| {
                Ok(NaFila {
                    id: id_de(&r.get::<_, Vec<u8>>(0)?),
                    estado: EstadoDocumento::de_str(&r.get::<_, String>(1)?)
                        .unwrap_or(EstadoDocumento::Pendente),
                    tentativas: r.get(2)?,
                    chave: r.get(3)?,
                    nota: r.get(4)?,
                    ambiente: if r.get::<_, String>(5)? == "producao" {
                        AmbienteFiscal::Producao
                    } else {
                        AmbienteFiscal::Homologacao
                    },
                })
            },
        )
        .map_err(falha)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(falha);
    linhas
}

/// Grava o resultado de uma tentativa — só se o documento ainda está no estado em que foi lido
/// (outra transmissão pode ter chegado antes). Devolve se gravou.
pub(crate) fn aplicar_mudanca(
    c: &Connection,
    empresa: Id,
    id: Id,
    estado_lido: EstadoDocumento,
    m: &Mudanca,
    agora: Instante,
) -> Resultado<bool> {
    let proxima = agora.em_micros() + m.espera_segundos * 1_000_000;
    let autorizado = (m.estado == EstadoDocumento::Autorizada).then(|| agora.em_micros());
    c.execute(
        "UPDATE fiscal_documento SET
           estado = ?4,
           chave = CASE WHEN ?5 THEN ?6 ELSE chave END,
           protocolo = COALESCE(?7, protocolo),
           xml = COALESCE(?8, xml),
           codigo = ?9,
           motivo = ?10,
           tentativas = tentativas + 1,
           proxima_tentativa = ?11,
           autorizado_em = COALESCE(?12, autorizado_em),
           atualizado_em = ?13
         WHERE empresa = ?1 AND id = ?2 AND estado = ?3",
        params![
            empresa.em_bytes().as_slice(),
            id.em_bytes().as_slice(),
            estado_lido.como_str(),
            m.estado.como_str(),
            m.trocar_chave,
            m.chave,
            m.protocolo,
            m.xml,
            m.codigo,
            m.motivo,
            proxima,
            autorizado,
            agora.em_micros(),
        ],
    )
    .map(|n| n > 0)
    .map_err(falha)
}

/// Troca a nota de um documento rejeitado (reenvio): volta para a fila, ou continua rejeitado
/// com os problemas novos.
pub(crate) fn remontar(
    c: &Connection,
    empresa: Id,
    id: Id,
    nota: Option<&[u8]>,
    total: Dinheiro,
    motivo: Option<&str>,
    agora: Instante,
) -> Resultado<()> {
    let estado = if nota.is_some() {
        EstadoDocumento::Pendente
    } else {
        EstadoDocumento::Rejeitada
    };
    c.execute(
        "UPDATE fiscal_documento SET estado = ?3, nota = ?4, total = ?5, motivo = ?6, codigo = NULL,
           chave = NULL, tentativas = 0, proxima_tentativa = 0, atualizado_em = ?7
         WHERE empresa = ?1 AND id = ?2 AND estado = 'Rejeitada'",
        params![
            empresa.em_bytes().as_slice(),
            id.em_bytes().as_slice(),
            estado.como_str(),
            nota,
            total.em_centavos(),
            motivo,
            agora.em_micros()
        ],
    )
    .map(|_| ())
    .map_err(falha)
}

/// O resumo da fila; "hoje" começa em `inicio_do_dia`.
pub(crate) fn resumo(
    c: &Connection,
    empresa: Id,
    inicio_do_dia: Instante,
) -> Resultado<ResumoFiscal> {
    c.query_row(
        "SELECT
           COALESCE(SUM(estado = 'Pendente'), 0),
           COALESCE(SUM(estado = 'Incerta'), 0),
           COALESCE(SUM(estado = 'Rejeitada'), 0),
           COALESCE(SUM(estado = 'Autorizada' AND autorizado_em >= ?2), 0),
           MIN(CASE WHEN estado IN ('Pendente','Incerta') THEN criado_em END)
         FROM fiscal_documento WHERE empresa = ?1",
        params![empresa.em_bytes().as_slice(), inicio_do_dia.em_micros()],
        |r| {
            Ok(ResumoFiscal {
                pendentes: r.get(0)?,
                incertas: r.get(1)?,
                rejeitadas: r.get(2)?,
                autorizadas_hoje: r.get(3)?,
                esperando_desde: r.get::<_, Option<i64>>(4)?.map(Instante::de_micros),
            })
        },
    )
    .map_err(falha)
}
