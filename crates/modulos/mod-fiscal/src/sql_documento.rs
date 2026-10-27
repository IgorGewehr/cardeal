//! O SQL dos documentos fiscais, da numeração por série e da configuração da NFC-e.

use cardeal_fiscal::{AmbienteFiscal, ModeloFiscal};
use cardeal_kernel::{CodigoErro, Dinheiro, Erro, Id, Instante, Resultado};
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::documento::{
    Busca, DocumentoResumo, EstadoDocumento, Mudanca, PedidoDocumento, ResumoFiscal,
    TENTATIVAS_DA_BUSCA,
};

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
    /// A nota que esta substitui.
    pub substitui: Option<Id>,
}

pub(crate) fn inserir_documento(c: &Connection, d: &NovoDocumento<'_>) -> Resultado<()> {
    c.execute(
        "INSERT INTO fiscal_documento
           (id, empresa, modelo, serie, numero, ambiente, origem_modulo, origem_id, estado, total,
            nota, motivo, criado_em, atualizado_em, substitui)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?13,?14)",
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
            d.substitui.map(|s| s.em_bytes().to_vec()),
        ],
    )
    .map(|_| ())
    .map_err(falha)
}

/// A nota passa a ser substituída por `por` (só se ainda não era): deixa de ser a vigente da
/// venda. Devolve se marcou.
pub(crate) fn marcar_substituida(c: &Connection, empresa: Id, id: Id, por: Id) -> Resultado<bool> {
    c.execute(
        "UPDATE fiscal_documento SET substituida_por = ?3
         WHERE empresa = ?1 AND id = ?2 AND substituida_por IS NULL",
        params![
            empresa.em_bytes().as_slice(),
            id.em_bytes().as_slice(),
            por.em_bytes().as_slice()
        ],
    )
    .map(|n| n > 0)
    .map_err(falha)
}

/// O estado e a chave da substituta.
pub(crate) fn estado_e_chave(
    c: &Connection,
    empresa: Id,
    id: Id,
) -> Resultado<Option<(EstadoDocumento, Option<String>)>> {
    c.query_row(
        "SELECT estado, chave FROM fiscal_documento WHERE empresa = ?1 AND id = ?2",
        params![empresa.em_bytes().as_slice(), id.em_bytes().as_slice()],
        |r| {
            Ok((
                EstadoDocumento::de_str(&r.get::<_, String>(0)?)
                    .unwrap_or(EstadoDocumento::Pendente),
                r.get(1)?,
            ))
        },
    )
    .optional()
    .map_err(falha)
}

const COLUNAS: &str =
    "id, modelo, serie, numero, ambiente, estado, total, chave, protocolo, codigo,
                       motivo, origem_id, tentativas, criado_em, autorizado_em, pedido,
                       anulada, contingencia_em, substitui, substituida_por";

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
        pedido: r
            .get::<_, Option<String>>(15)?
            .as_deref()
            .and_then(PedidoDocumento::de_str),
        anulada: r.get::<_, i64>(16)? != 0,
        contingencia_em: r.get::<_, Option<i64>>(17)?.map(Instante::de_micros),
        substitui: r.get::<_, Option<Vec<u8>>>(18)?.map(|b| id_de(&b)),
        substituida_por: r.get::<_, Option<Vec<u8>>>(19)?.map(|b| id_de(&b)),
    })
}

/// Os documentos de várias origens de um módulo (as NFC-e das vendas do dia) — uma consulta,
/// não uma por venda.
pub(crate) fn documentos_das_origens(
    c: &Connection,
    empresa: Id,
    origem_modulo: &str,
    origens: &[Id],
) -> Resultado<Vec<DocumentoResumo>> {
    let mut stmt = c
        .prepare(&format!(
            "SELECT {COLUNAS} FROM fiscal_documento
             WHERE empresa = ?1 AND origem_modulo = ?2 AND origem_id = ?3
               AND substituida_por IS NULL"
        ))
        .map_err(falha)?;
    let mut docs = Vec::new();
    for origem in origens {
        if let Some(d) = stmt
            .query_row(
                params![
                    empresa.em_bytes().as_slice(),
                    origem_modulo,
                    origem.em_bytes().as_slice()
                ],
                resumo_de,
            )
            .optional()
            .map_err(falha)?
        {
            docs.push(d);
        }
    }
    Ok(docs)
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
             WHERE empresa = ?1 AND origem_modulo = ?2 AND origem_id = ?3
               AND substituida_por IS NULL"
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
    /// A nota assinada (a da contingência, ou da emissão incerta) enquanto o `nfeProc` não vem.
    pub xml_assinado: Option<String>,
}

/// O documento, a nota guardada e o XML autorizado.
pub(crate) fn documento(
    c: &Connection,
    empresa: Id,
    id: Id,
) -> Resultado<Option<DocumentoGuardado>> {
    c.query_row(
        &format!(
            "SELECT {COLUNAS}, nota, xml, origem_modulo, xml_assinado FROM fiscal_documento
             WHERE empresa = ?1 AND id = ?2"
        ),
        params![empresa.em_bytes().as_slice(), id.em_bytes().as_slice()],
        |r| {
            Ok(DocumentoGuardado {
                resumo: resumo_de(r)?,
                nota: r.get(20)?,
                xml: r.get(21)?,
                origem_modulo: r.get(22)?,
                xml_assinado: r.get(23)?,
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
    pub pedido: Option<PedidoDocumento>,
    pub justificativa: Option<String>,
    pub protocolo: Option<String>,
    pub serie: u32,
    pub numero: u64,
    pub criado_em: Instante,
    pub anulada: bool,
    pub estado: EstadoDocumento,
    pub tentativas: u32,
    pub chave: Option<String>,
    pub nota: Option<Vec<u8>>,
    /// A nota assinada da emissão incerta (vai na consulta).
    pub xml_assinado: Option<String>,
    /// Um documento fechado com XML a buscar na consulta.
    pub buscar_xml: bool,
    /// A origem (o cupom) e o módulo dela.
    pub origem: Id,
    pub origem_modulo: String,
    pub total: Dinheiro,
    /// A nota que esta substitui (a substituta sai sempre em contingência).
    pub substitui: Option<Id>,
    /// A substituta desta.
    pub substituida_por: Option<Id>,
}

/// Os pendentes, os incertos, os pedidos e as buscas de XML cuja hora chegou, os mais antigos
/// primeiro.
pub(crate) fn fila(
    c: &Connection,
    empresa: Id,
    agora: Instante,
    limite: u32,
) -> Resultado<Vec<NaFila>> {
    let mut stmt = c
        .prepare(
            "SELECT id, estado, tentativas, chave, nota, ambiente, pedido, justificativa, protocolo,
                    serie, numero, criado_em, anulada, xml_assinado, xml_a_buscar, origem_id,
                    origem_modulo, total, substitui, substituida_por
             FROM fiscal_documento
             WHERE empresa = ?1
               AND (estado IN ('Pendente','Incerta','Contingencia') OR pedido IS NOT NULL
                    OR xml_a_buscar > 0)
               AND proxima_tentativa <= ?2
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
                    pedido: r
                        .get::<_, Option<String>>(6)?
                        .as_deref()
                        .and_then(PedidoDocumento::de_str),
                    justificativa: r.get(7)?,
                    protocolo: r.get(8)?,
                    serie: r.get(9)?,
                    numero: u64::try_from(r.get::<_, i64>(10)?).unwrap_or(0),
                    criado_em: Instante::de_micros(r.get(11)?),
                    anulada: r.get::<_, i64>(12)? != 0,
                    xml_assinado: r.get(13)?,
                    buscar_xml: r.get::<_, i64>(14)? > 0,
                    origem: id_de(&r.get::<_, Vec<u8>>(15)?),
                    origem_modulo: r.get(16)?,
                    total: Dinheiro::centavos(r.get(17)?),
                    substitui: r.get::<_, Option<Vec<u8>>>(18)?.map(|b| id_de(&b)),
                    substituida_por: r.get::<_, Option<Vec<u8>>>(19)?.map(|b| id_de(&b)),
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
           atualizado_em = ?13,
           pedido = CASE WHEN ?14 THEN NULL ELSE pedido END,
           protocolo_evento = COALESCE(?15, protocolo_evento),
           evento_em = CASE WHEN ?15 IS NULL THEN evento_em ELSE ?13 END,
           xml_evento = COALESCE(?16, xml_evento),
           xml_assinado = CASE WHEN ?8 IS NOT NULL THEN NULL ELSE COALESCE(?17, xml_assinado) END,
           xml_a_buscar = COALESCE(?18, xml_a_buscar),
           contingencia_em = COALESCE(?19, contingencia_em)
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
            m.limpar_pedido,
            m.protocolo_evento,
            m.xml_evento,
            m.xml_assinado,
            m.buscar_xml.then_some(TENTATIVAS_DA_BUSCA),
            m.contingencia_em.map(Instante::em_micros),
        ],
    )
    .map(|n| n > 0)
    .map_err(falha)
}

/// Grava o resultado da busca de XMLs de um documento fechado: guarda o que veio (sem
/// sobrescrever o que já havia) e encerra a busca — a próxima consulta da mesma chave só
/// depois de uma hora (a SEFAZ barra consulta repetida, "consumo indevido") —, ou adia.
pub(crate) fn aplicar_busca(
    c: &Connection,
    empresa: Id,
    id: Id,
    busca: &Busca,
    agora: Instante,
) -> Resultado<()> {
    let (empresa, id) = (empresa.em_bytes(), id.em_bytes());
    match busca {
        Busca::Encerrar {
            xml,
            xml_evento,
            protocolo_evento,
        } => c.execute(
            "UPDATE fiscal_documento SET
               xml = COALESCE(xml, ?3),
               xml_evento = COALESCE(xml_evento, ?4),
               protocolo_evento = COALESCE(protocolo_evento, ?5),
               xml_assinado = CASE WHEN ?3 IS NOT NULL THEN NULL ELSE xml_assinado END,
               xml_a_buscar = 0,
               proxima_tentativa = ?6 + 3600000000,
               atualizado_em = ?6
             WHERE empresa = ?1 AND id = ?2",
            params![
                empresa.as_slice(),
                id.as_slice(),
                xml,
                xml_evento,
                protocolo_evento,
                agora.em_micros()
            ],
        ),
        Busca::TentarDeNovo {
            espera_segundos,
            gasta,
        } => c.execute(
            "UPDATE fiscal_documento SET
               proxima_tentativa = ?3,
               xml_a_buscar = CASE WHEN ?4 THEN MAX(xml_a_buscar - 1, 0) ELSE xml_a_buscar END
             WHERE empresa = ?1 AND id = ?2",
            params![
                empresa.as_slice(),
                id.as_slice(),
                agora.em_micros() + espera_segundos * 1_000_000,
                gasta
            ],
        ),
    }
    .map(|_| ())
    .map_err(falha)
}

/// Põe na busca os documentos de produção que podem ter o XML recuperado pela consulta da
/// nota: o cancelamento que não veio e a nota sem `nfeProc` cuja nota assinada ficou guardada.
/// Devolve quantos estão na busca.
pub(crate) fn marcar_xmls_a_buscar(c: &Connection, empresa: Id) -> Resultado<u32> {
    let empresa = empresa.em_bytes();
    c.execute(
        "UPDATE fiscal_documento SET xml_a_buscar = ?2
         WHERE empresa = ?1 AND ambiente = 'producao' AND xml_a_buscar = 0
           AND chave IS NOT NULL
           AND ((estado = 'Cancelada' AND xml_evento IS NULL)
             OR (estado IN ('Autorizada','Cancelada','Denegada') AND xml IS NULL
                 AND xml_assinado IS NOT NULL))",
        params![empresa.as_slice(), TENTATIVAS_DA_BUSCA],
    )
    .map_err(falha)?;
    c.query_row(
        "SELECT COUNT(*) FROM fiscal_documento WHERE empresa = ?1 AND xml_a_buscar > 0",
        params![empresa.as_slice()],
        |r| r.get(0),
    )
    .map_err(falha)
}

/// Um documento que fechou (autorizado, cancelado, inutilizado, denegado), com os XMLs.
pub(crate) struct DocumentoFechado {
    pub resumo: DocumentoResumo,
    pub xml: Option<String>,
    pub xml_evento: Option<String>,
    pub protocolo_evento: Option<String>,
    /// Quando o evento (cancelamento, inutilização) foi aceito — ou a última mudança.
    pub evento_em: Instante,
    /// A nota assinada da emissão incerta ficou guardada (a busca recupera o `nfeProc`).
    pub xml_assinado_guardado: bool,
}

/// Os documentos de produção que fecharam entre `de` e `ate` (exclusive): a inutilizada pela
/// hora da inutilização, os outros pela da autorização (ou da criação).
pub(crate) fn fechados_entre(
    c: &Connection,
    empresa: Id,
    de: Instante,
    ate: Instante,
) -> Resultado<Vec<DocumentoFechado>> {
    let mut stmt = c
        .prepare(&format!(
            "SELECT {COLUNAS}, xml, xml_evento, protocolo_evento,
                    COALESCE(evento_em, atualizado_em), xml_assinado IS NOT NULL
             FROM fiscal_documento
             WHERE empresa = ?1 AND ambiente = 'producao'
               AND estado IN ('Autorizada','Cancelada','Inutilizada','Denegada')
               AND CASE WHEN estado = 'Inutilizada' THEN COALESCE(evento_em, atualizado_em)
                        ELSE COALESCE(autorizado_em, criado_em) END >= ?2
               AND CASE WHEN estado = 'Inutilizada' THEN COALESCE(evento_em, atualizado_em)
                        ELSE COALESCE(autorizado_em, criado_em) END < ?3"
        ))
        .map_err(falha)?;
    let linhas = stmt
        .query_map(
            params![
                empresa.em_bytes().as_slice(),
                de.em_micros(),
                ate.em_micros()
            ],
            |r| {
                Ok(DocumentoFechado {
                    resumo: resumo_de(r)?,
                    xml: r.get(20)?,
                    xml_evento: r.get(21)?,
                    protocolo_evento: r.get(22)?,
                    evento_em: Instante::de_micros(r.get(23)?),
                    xml_assinado_guardado: r.get(24)?,
                })
            },
        )
        .map_err(falha)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(falha);
    linhas
}

/// Os documentos de produção criados antes de `ate` que ainda não tiveram desfecho, por série
/// e número.
pub(crate) fn sem_desfecho_ate(
    c: &Connection,
    empresa: Id,
    ate: Instante,
) -> Resultado<Vec<DocumentoResumo>> {
    let mut stmt = c
        .prepare(&format!(
            "SELECT {COLUNAS} FROM fiscal_documento
             WHERE empresa = ?1 AND ambiente = 'producao'
               AND estado IN ('Pendente','Incerta','Contingencia','Rejeitada')
               AND criado_em < ?2
             ORDER BY modelo, serie, numero"
        ))
        .map_err(falha)?;
    let linhas = stmt
        .query_map(
            params![empresa.em_bytes().as_slice(), ate.em_micros()],
            resumo_de,
        )
        .map_err(falha)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(falha);
    linhas
}

/// Quantos documentos de homologação o período teve (pela autorização ou criação).
pub(crate) fn de_homologacao_entre(
    c: &Connection,
    empresa: Id,
    de: Instante,
    ate: Instante,
) -> Resultado<u32> {
    c.query_row(
        "SELECT COUNT(*) FROM fiscal_documento
         WHERE empresa = ?1 AND ambiente = 'homologacao'
           AND COALESCE(autorizado_em, criado_em) >= ?2
           AND COALESCE(autorizado_em, criado_em) < ?3",
        params![
            empresa.em_bytes().as_slice(),
            de.em_micros(),
            ate.em_micros()
        ],
        |r| r.get(0),
    )
    .map_err(falha)
}

/// Marca o documento como de venda desfeita (permanente), guarda a justificativa e, quando
/// já há o que pedir, registra o pedido (se ainda não houver outro). Devolve se marcou.
pub(crate) fn anular(
    c: &Connection,
    empresa: Id,
    id: Id,
    justificativa: &str,
    pedido: Option<PedidoDocumento>,
    agora: Instante,
) -> Resultado<bool> {
    c.execute(
        "UPDATE fiscal_documento SET anulada = 1, justificativa = ?3,
           pedido = COALESCE(pedido, ?4),
           tentativas = CASE WHEN ?4 IS NOT NULL AND pedido IS NULL THEN 0 ELSE tentativas END,
           proxima_tentativa = 0, atualizado_em = ?5
         WHERE empresa = ?1 AND id = ?2 AND anulada = 0",
        params![
            empresa.em_bytes().as_slice(),
            id.em_bytes().as_slice(),
            justificativa,
            pedido.map(PedidoDocumento::como_str),
            agora.em_micros()
        ],
    )
    .map(|n| n > 0)
    .map_err(falha)
}

/// Registra um pedido (cancelar/inutilizar) num documento que está em `estado_exigido` e sem
/// pedido. Devolve se registrou.
pub(crate) fn pedir(
    c: &Connection,
    empresa: Id,
    id: Id,
    pedido: PedidoDocumento,
    justificativa: &str,
    estado_exigido: EstadoDocumento,
    agora: Instante,
) -> Resultado<bool> {
    c.execute(
        "UPDATE fiscal_documento SET pedido = ?3, justificativa = ?4, tentativas = 0,
           proxima_tentativa = 0, atualizado_em = ?5
         WHERE empresa = ?1 AND id = ?2 AND estado = ?6 AND pedido IS NULL",
        params![
            empresa.em_bytes().as_slice(),
            id.em_bytes().as_slice(),
            pedido.como_str(),
            justificativa,
            agora.em_micros(),
            estado_exigido.como_str()
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
           chave = NULL, xml_assinado = NULL, contingencia_em = NULL, tentativas = 0,
           proxima_tentativa = 0, atualizado_em = ?7
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

/// O resumo da fila; "hoje" começa em `inicio_do_dia`; as buscas de XML contam se a hora
/// delas chegou em `agora`.
pub(crate) fn resumo(
    c: &Connection,
    empresa: Id,
    inicio_do_dia: Instante,
    agora: Instante,
) -> Resultado<ResumoFiscal> {
    c.query_row(
        "SELECT
           COALESCE(SUM(estado = 'Pendente'), 0),
           COALESCE(SUM(estado = 'Incerta'), 0),
           COALESCE(SUM(estado = 'Rejeitada'), 0),
           COALESCE(SUM(estado = 'Autorizada' AND autorizado_em >= ?2), 0),
           MIN(CASE WHEN estado IN ('Pendente','Incerta') THEN criado_em END),
           COALESCE(SUM(xml_a_buscar > 0 AND proxima_tentativa <= ?3), 0),
           COALESCE(SUM(estado = 'Contingencia'), 0),
           MIN(CASE WHEN estado = 'Contingencia' THEN contingencia_em END)
         FROM fiscal_documento WHERE empresa = ?1",
        params![
            empresa.em_bytes().as_slice(),
            inicio_do_dia.em_micros(),
            agora.em_micros()
        ],
        |r| {
            Ok(ResumoFiscal {
                pendentes: r.get(0)?,
                incertas: r.get(1)?,
                rejeitadas: r.get(2)?,
                autorizadas_hoje: r.get(3)?,
                esperando_desde: r.get::<_, Option<i64>>(4)?.map(Instante::de_micros),
                xmls_a_buscar: r.get(5)?,
                em_contingencia: r.get(6)?,
                contingencia_desde: r.get::<_, Option<i64>>(7)?.map(Instante::de_micros),
            })
        },
    )
    .map_err(falha)
}
