//! A NFS-e no SQLite: a configuração, a nota e a fila.

use cardeal_fiscal::MotivoCancelamentoNfse;
use cardeal_kernel::{CodigoErro, Dinheiro, Erro, Id, Instante, Percentual, Resultado};
use cardeal_modkit::Ctx;
use cardeal_storage::UnidadeDeTrabalho;
use rusqlite::{params, Connection, OptionalExtension, Row};

use super::{
    codigo_nacional, item_lc116, montar_nfse_da_os, tomador_do_cliente, CancelarNfse,
    ConfiguracaoNfse, ConfigurarNfse, DestinoNfse, EstadoNfse, NfseResumo, OsParaNota,
    JUSTIFICATIVA_MINIMA, ORIGEM_OS,
};
use crate::AmbienteFiscal;

#[allow(clippy::needless_pass_by_value)]
fn falha(e: rusqlite::Error) -> Erro {
    Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string())
}

fn id_de(b: &[u8]) -> Id {
    <[u8; 16]>::try_from(b).map_or(Id::NULO, Id::de_bytes)
}

const fn ambiente_txt(a: AmbienteFiscal) -> &'static str {
    match a {
        AmbienteFiscal::Homologacao => "homologacao",
        AmbienteFiscal::Producao => "producao",
    }
}

fn ambiente_de(s: &str) -> AmbienteFiscal {
    if s == "producao" {
        AmbienteFiscal::Producao
    } else {
        AmbienteFiscal::Homologacao
    }
}

const fn coluna_do_proximo(a: AmbienteFiscal) -> &'static str {
    match a {
        AmbienteFiscal::Homologacao => "proximo_homologacao",
        AmbienteFiscal::Producao => "proximo_producao",
    }
}

pub(crate) fn configuracao(c: &Connection, empresa: Id) -> Resultado<Option<ConfiguracaoNfse>> {
    let ambiente = crate::sql::ambiente(c, empresa)?;
    c.query_row(
        &format!(
            "SELECT codigo_tributacao_nacional, item_lc116, codigo_tributacao_municipal, nbs,
                    aliquota, serie, {}
             FROM fiscal_configuracao_nfse WHERE empresa = ?1",
            coluna_do_proximo(ambiente)
        ),
        [empresa.em_bytes().as_slice()],
        |r| {
            Ok(ConfiguracaoNfse {
                codigo_tributacao_nacional: r.get(0)?,
                item_lc116: r.get(1)?,
                codigo_tributacao_municipal: r.get(2)?,
                nbs: r.get(3)?,
                aliquota: Percentual::unidades(r.get(4)?),
                serie: r.get(5)?,
                proximo_numero: r.get(6)?,
                ambiente,
            })
        },
    )
    .optional()
    .map_err(falha)
}

fn invalido(campo: &str, mensagem: &str) -> Erro {
    Erro::novo(CodigoErro::VALOR_INVALIDO, mensagem).no_campo(campo)
}

pub(super) fn configurar(
    cmd: &ConfigurarNfse,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<ConfiguracaoNfse> {
    let codigo = codigo_nacional(&cmd.codigo_tributacao_nacional).ok_or_else(|| {
        invalido(
            "codigo_tributacao_nacional",
            "O código de tributação nacional tem 6 dígitos (ex.: 14.01.01)",
        )
    })?;
    let item = item_lc116(&cmd.item_lc116)
        .ok_or_else(|| invalido("item_lc116", "O item da LC 116 é como 14.01"))?;
    if !(1..=99_999).contains(&cmd.serie) {
        return Err(invalido("serie", "A série do DPS vai de 1 a 99999"));
    }
    if cmd.aliquota.unidades_internas() < 0 || cmd.aliquota > Percentual::pontos(100) {
        return Err(invalido("aliquota", "O percentual vai de 0 a 100"));
    }
    let nbs = cmd
        .nbs
        .as_deref()
        .map(|n| n.chars().filter(char::is_ascii_digit).collect::<String>())
        .filter(|n| !n.is_empty());
    if nbs.as_ref().is_some_and(|n| n.len() != 9) {
        return Err(invalido("nbs", "O NBS tem 9 dígitos (ex.: 1.0101.00.00)"));
    }
    let municipal = cmd
        .codigo_tributacao_municipal
        .as_deref()
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .map(str::to_owned);
    let c = uow.conexao();
    c.execute(
        "INSERT INTO fiscal_configuracao_nfse
           (empresa, codigo_tributacao_nacional, item_lc116, codigo_tributacao_municipal, nbs,
            aliquota, serie)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT (empresa) DO UPDATE SET
           codigo_tributacao_nacional = excluded.codigo_tributacao_nacional,
           item_lc116 = excluded.item_lc116,
           codigo_tributacao_municipal = excluded.codigo_tributacao_municipal,
           nbs = excluded.nbs, aliquota = excluded.aliquota, serie = excluded.serie",
        params![
            ctx.empresa.em_bytes().as_slice(),
            codigo,
            item,
            municipal,
            nbs,
            cmd.aliquota.unidades_internas(),
            cmd.serie
        ],
    )
    .map_err(falha)?;
    if let Some(n) = cmd.proximo_numero {
        if n == 0 {
            return Err(invalido("proximo_numero", "O próximo número começa em 1"));
        }
        let ambiente = crate::sql::ambiente(c, ctx.empresa)?;
        c.execute(
            &format!(
                "UPDATE fiscal_configuracao_nfse SET {} = ?2 WHERE empresa = ?1",
                coluna_do_proximo(ambiente)
            ),
            params![ctx.empresa.em_bytes().as_slice(), n],
        )
        .map_err(falha)?;
    }
    configuracao(c, ctx.empresa)?.ok_or_else(|| Erro::nao_encontrado("configuração da NFS-e"))
}

const COLUNAS: &str = "id, origem, ambiente, serie, numero, estado, total, numero_nfse, chave,
                       codigo_verificacao, motivo, cancelar, criado_em, autorizado_em";

fn resumo_de(r: &Row<'_>) -> rusqlite::Result<NfseResumo> {
    Ok(NfseResumo {
        id: id_de(&r.get::<_, Vec<u8>>(0)?),
        origem: id_de(&r.get::<_, Vec<u8>>(1)?),
        ambiente: ambiente_de(&r.get::<_, String>(2)?),
        serie: r.get(3)?,
        numero: r.get(4)?,
        estado: EstadoNfse::de_str(&r.get::<_, String>(5)?),
        total: Dinheiro::centavos(r.get(6)?),
        numero_nfse: r.get(7)?,
        chave: r.get(8)?,
        codigo_verificacao: r.get(9)?,
        motivo: r.get(10)?,
        cancelamento_pedido: r.get::<_, i64>(11)? != 0,
        criado_em: Instante::de_micros(r.get(12)?),
        autorizado_em: r.get::<_, Option<i64>>(13)?.map(Instante::de_micros),
    })
}

/// A NFS-e de uma origem: a viva, ou a última cancelada.
pub(crate) fn da_origem(
    c: &Connection,
    empresa: Id,
    origem_modulo: &str,
    origem: Id,
) -> Resultado<Option<NfseResumo>> {
    c.query_row(
        &format!(
            "SELECT {COLUNAS} FROM fiscal_nfse
             WHERE empresa = ?1 AND origem_modulo = ?2 AND origem = ?3
             ORDER BY estado = 'Cancelada', criado_em DESC LIMIT 1"
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

fn por_id(c: &Connection, empresa: Id, id: Id) -> Resultado<NfseResumo> {
    c.query_row(
        &format!("SELECT {COLUNAS} FROM fiscal_nfse WHERE empresa = ?1 AND id = ?2"),
        params![empresa.em_bytes().as_slice(), id.em_bytes().as_slice()],
        resumo_de,
    )
    .optional()
    .map_err(falha)?
    .ok_or_else(|| Erro::nao_encontrado("NFS-e"))
}

/// Reserva o próximo número do DPS no ambiente.
fn reservar(c: &Connection, empresa: Id, ambiente: AmbienteFiscal) -> Resultado<(u32, u64)> {
    let coluna = coluna_do_proximo(ambiente);
    let (serie, numero): (u32, u64) = c
        .query_row(
            &format!("SELECT serie, {coluna} FROM fiscal_configuracao_nfse WHERE empresa = ?1"),
            [empresa.em_bytes().as_slice()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(falha)?;
    c.execute(
        &format!("UPDATE fiscal_configuracao_nfse SET {coluna} = {coluna} + 1 WHERE empresa = ?1"),
        [empresa.em_bytes().as_slice()],
    )
    .map_err(falha)?;
    Ok((serie, numero))
}

/// A OS que vira nota, e o cliente — pelo `mod-os` e pelo `mod-clientes`.
fn ler_os(uow: &mut UnidadeDeTrabalho, ordem: Id) -> Resultado<(OsParaNota, Id, Dinheiro)> {
    let os = mod_os::RepositorioOs::novo(uow)
        .buscar_ordem(ordem)?
        .ok_or_else(|| Erro::nao_encontrado("ordem de serviço"))?;
    if os.estado != mod_os::EstadoOs::Faturada {
        return Err(Erro::novo(
            CodigoErro::ESTADO_INVALIDO,
            "A NFS-e sai da OS faturada",
        ));
    }
    let servicos: Vec<(String, Dinheiro)> = mod_os::RepositorioOs::novo(uow)
        .itens_mao_de_obra_da_ordem(ordem)?
        .into_iter()
        .map(|m| (m.descricao, m.valor))
        .collect();
    let total = servicos.iter().map(|(_, v)| *v).sum();
    Ok((
        OsParaNota {
            numero: os.numero,
            equipamento: os.equipamento,
            servicos,
        },
        os.cliente,
        total,
    ))
}

pub(super) fn emitir_da_os(
    ordem: Id,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<NfseResumo> {
    let existente = da_origem(uow.conexao(), ctx.empresa, ORIGEM_OS, ordem)?
        .filter(|n| n.estado != EstadoNfse::Cancelada);
    if let Some(n) = &existente {
        if n.estado != EstadoNfse::Rejeitada {
            return Ok(n.clone());
        }
    }
    let (os, cliente, total) = ler_os(uow, ordem)?;
    let c = uow.conexao();
    let cfg = configuracao(c, ctx.empresa)?.ok_or_else(|| {
        Erro::novo(
            CodigoErro::REGRA_VIOLADA,
            "Configure a NFS-e em Configurações › Fiscal (o código do serviço e a série)",
        )
    })?;
    let ambiente = crate::sql::ambiente(c, ctx.empresa)?;
    let (serie, numero) = match &existente {
        Some(n) => (n.serie, n.numero),
        None => reservar(c, ctx.empresa, ambiente)?,
    };
    let mut problemas = Vec::new();
    if crate::sql::certificado(c, ctx.empresa)?.is_none() {
        problemas.push(
            "o certificado digital A1 não está instalado (Configurações › Fiscal)".to_string(),
        );
    }
    let tomador = tomador_da_os(c, cliente, &mut problemas)?;
    let nota = match crate::nfce::sqlite::emitente(c, ctx.empresa)? {
        Ok(e) => montar_nfse_da_os(e, tomador, &os, &cfg, serie, numero, ctx.hoje()),
        Err(p) => Err(p),
    };
    let nota = match nota {
        Ok(n) if problemas.is_empty() => Ok(n),
        Ok(_) => Err(problemas),
        Err(mut p) => {
            problemas.append(&mut p);
            Err(problemas)
        }
    };
    let (estado, blob, motivo) = match nota {
        Ok(n) => (
            EstadoNfse::Pendente,
            Some(
                postcard::to_stdvec(&n)
                    .map_err(|e| Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string()))?,
            ),
            None,
        ),
        Err(p) => (EstadoNfse::Rejeitada, None, Some(p.join("; "))),
    };
    gravar(
        c,
        ctx,
        existente.as_ref(),
        ordem,
        (ambiente, serie, numero),
        (estado, blob, motivo),
        total,
    )?;
    da_origem(c, ctx.empresa, ORIGEM_OS, ordem)?.ok_or_else(|| Erro::nao_encontrado("NFS-e"))
}

/// Insere a NFS-e nova, ou remonta a rejeitada no mesmo número.
#[allow(clippy::type_complexity)]
fn gravar(
    c: &Connection,
    ctx: &Ctx,
    existente: Option<&NfseResumo>,
    ordem: Id,
    (ambiente, serie, numero): (AmbienteFiscal, u32, u64),
    (estado, blob, motivo): (EstadoNfse, Option<Vec<u8>>, Option<String>),
    total: Dinheiro,
) -> Resultado<()> {
    match existente {
        Some(n) => {
            c.execute(
                "UPDATE fiscal_nfse SET estado = ?2, nota = ?3, motivo = ?4, total = ?5,
               tentativas = 0, proxima_tentativa = 0
             WHERE id = ?1",
                params![
                    n.id.em_bytes().as_slice(),
                    estado.como_str(),
                    blob,
                    motivo,
                    total.em_centavos()
                ],
            )
            .map_err(falha)?;
        }
        None => {
            c.execute(
                "INSERT INTO fiscal_nfse
               (id, empresa, origem_modulo, origem, ambiente, serie, numero, estado, total,
                nota, motivo, criado_em)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    Id::novo().em_bytes().as_slice(),
                    ctx.empresa.em_bytes().as_slice(),
                    ORIGEM_OS,
                    ordem.em_bytes().as_slice(),
                    ambiente_txt(ambiente),
                    serie,
                    numero,
                    estado.como_str(),
                    total.em_centavos(),
                    blob,
                    motivo,
                    ctx.agora.em_micros()
                ],
            )
            .map_err(falha)?;
        }
    }
    Ok(())
}

/// O tomador: o cliente da OS.
fn tomador_da_os(
    c: &Connection,
    cliente: Id,
    problemas: &mut Vec<String>,
) -> Resultado<cardeal_fiscal::TomadorNfse> {
    let Some(p) = mod_clientes::pessoa_por_id(c, cliente)? else {
        problemas.push("o cliente da OS não existe mais".into());
        return Ok(tomador_do_cliente("", &[], &[], None));
    };
    let email = mod_clientes::contatos_da_pessoa(c, cliente)?
        .into_iter()
        .filter(|ct| ct.tipo == mod_clientes::TipoContato::Email)
        .max_by_key(|ct| ct.principal)
        .map(|ct| ct.valor.trim().to_string());
    Ok(tomador_do_cliente(
        &p.nome,
        &mod_clientes::documentos_da_pessoa(c, cliente)?,
        &mod_clientes::enderecos_da_pessoa(c, cliente)?,
        email,
    ))
}

pub(super) fn pedir_cancelamento(
    cmd: &CancelarNfse,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<NfseResumo> {
    let justificativa = cmd
        .justificativa
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if justificativa.chars().count() < JUSTIFICATIVA_MINIMA {
        return Err(invalido(
            "justificativa",
            "A justificativa do cancelamento precisa de ao menos 15 caracteres",
        ));
    }
    let c = uow.conexao();
    let n = por_id(c, ctx.empresa, cmd.nfse)?;
    if n.estado != EstadoNfse::Autorizada {
        return Err(Erro::novo(
            CodigoErro::ESTADO_INVALIDO,
            format!(
                "Só NFS-e autorizada se cancela (esta está {})",
                n.estado.rotulo()
            ),
        ));
    }
    if n.cancelamento_pedido {
        return Err(Erro::novo(
            CodigoErro::ESTADO_INVALIDO,
            "O cancelamento desta NFS-e já está na fila",
        ));
    }
    c.execute(
        "UPDATE fiscal_nfse SET cancelar = 1, motivo_cancelamento = ?2, justificativa = ?3,
           tentativas = 0, proxima_tentativa = 0
         WHERE id = ?1",
        params![
            n.id.em_bytes().as_slice(),
            cmd.motivo.codigo(),
            justificativa
        ],
    )
    .map_err(falha)?;
    por_id(c, ctx.empresa, n.id)
}

/// Uma NFS-e esperando a fila.
pub(crate) struct NfseNaFila {
    pub id: Id,
    pub ambiente: AmbienteFiscal,
    pub estado: EstadoNfse,
    pub cancelar: bool,
    pub serie: u32,
    pub numero: u64,
    pub nota: Option<Vec<u8>>,
    pub chave: Option<String>,
    pub numero_nfse: Option<u64>,
    pub motivo_cancelamento: Option<MotivoCancelamentoNfse>,
    pub justificativa: Option<String>,
    pub tentativas: u32,
}

fn motivo_de(codigo: &str) -> MotivoCancelamentoNfse {
    match codigo {
        "2" => MotivoCancelamentoNfse::ServicoNaoPrestado,
        "3" => MotivoCancelamentoNfse::Duplicidade,
        "4" => MotivoCancelamentoNfse::ErroDeProcessamento,
        _ => MotivoCancelamentoNfse::ErroNaEmissao,
    }
}

/// As NFS-e cuja hora chegou: as pendentes, as incertas e os cancelamentos pedidos.
pub(crate) fn fila(
    c: &Connection,
    empresa: Id,
    agora: Instante,
    limite: u32,
) -> Resultado<Vec<NfseNaFila>> {
    let mut stmt = c
        .prepare(
            "SELECT id, ambiente, estado, cancelar, serie, numero, nota, chave, numero_nfse,
                    motivo_cancelamento, justificativa, tentativas
             FROM fiscal_nfse
             WHERE empresa = ?1 AND proxima_tentativa <= ?2
               AND (estado IN ('Pendente', 'Incerta') OR (estado = 'Autorizada' AND cancelar = 1))
             ORDER BY criado_em LIMIT ?3",
        )
        .map_err(falha)?;
    let v = stmt
        .query_map(
            params![empresa.em_bytes().as_slice(), agora.em_micros(), limite],
            |r| {
                Ok(NfseNaFila {
                    id: id_de(&r.get::<_, Vec<u8>>(0)?),
                    ambiente: ambiente_de(&r.get::<_, String>(1)?),
                    estado: EstadoNfse::de_str(&r.get::<_, String>(2)?),
                    cancelar: r.get::<_, i64>(3)? != 0,
                    serie: r.get(4)?,
                    numero: r.get(5)?,
                    nota: r.get(6)?,
                    chave: r.get(7)?,
                    numero_nfse: r.get(8)?,
                    motivo_cancelamento: r.get::<_, Option<String>>(9)?.map(|m| motivo_de(&m)),
                    justificativa: r.get(10)?,
                    tentativas: r.get(11)?,
                })
            },
        )
        .map_err(falha)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(falha);
    v
}

/// Quantas NFS-e esperam a fila (para a vigia de fundo).
pub(crate) fn quantas_na_fila(c: &Connection, empresa: Id) -> Resultado<u32> {
    c.query_row(
        "SELECT COUNT(*) FROM fiscal_nfse WHERE empresa = ?1
           AND (estado IN ('Pendente', 'Incerta') OR (estado = 'Autorizada' AND cancelar = 1))",
        [empresa.em_bytes().as_slice()],
        |r| r.get(0),
    )
    .map_err(falha)
}

/// Grava o que veio do município.
pub(crate) fn aplicar(
    c: &Connection,
    id: Id,
    destino: &DestinoNfse,
    tentativas: u32,
    agora: Instante,
) -> Resultado<()> {
    let id = id.em_bytes();
    let id = id.as_slice();
    let depois = agora.em_micros() + crate::documento::espera(tentativas) * 1_000_000;
    let r = match destino {
        DestinoNfse::Autorizada(r) => c.execute(
            "UPDATE fiscal_nfse SET estado = 'Autorizada', numero_nfse = ?2, chave = ?3,
               codigo_verificacao = ?4, xml = ?5, motivo = NULL, autorizado_em = ?6,
               tentativas = tentativas + 1
             WHERE id = ?1",
            params![
                id,
                r.numero,
                r.chave,
                r.codigo_verificacao,
                r.xml,
                agora.em_micros()
            ],
        ),
        DestinoNfse::Rejeitada(m) => c.execute(
            "UPDATE fiscal_nfse SET estado = 'Rejeitada', motivo = ?2,
               tentativas = tentativas + 1
             WHERE id = ?1",
            params![id, m],
        ),
        DestinoNfse::Conferir(m) => c.execute(
            "UPDATE fiscal_nfse SET estado = 'Incerta', motivo = ?2,
               tentativas = tentativas + 1, proxima_tentativa = ?3
             WHERE id = ?1",
            params![id, m, depois],
        ),
        DestinoNfse::Reenviar => c.execute(
            "UPDATE fiscal_nfse SET estado = 'Pendente', tentativas = tentativas + 1,
               proxima_tentativa = 0
             WHERE id = ?1",
            params![id],
        ),
        DestinoNfse::Cancelada(xml) => c.execute(
            "UPDATE fiscal_nfse SET estado = 'Cancelada', cancelar = 0, xml_evento = ?2,
               cancelado_em = ?3, motivo = NULL
             WHERE id = ?1",
            params![id, xml, agora.em_micros()],
        ),
        DestinoNfse::CancelamentoRecusado(m) => c.execute(
            "UPDATE fiscal_nfse SET cancelar = 0, motivo = ?2 WHERE id = ?1",
            params![id, format!("Cancelamento recusado: {m}")],
        ),
        DestinoNfse::Esperar(m) => c.execute(
            "UPDATE fiscal_nfse SET motivo = ?2, tentativas = tentativas + 1,
               proxima_tentativa = ?3
             WHERE id = ?1",
            params![id, m, depois],
        ),
    };
    r.map(|_| ()).map_err(falha)
}

/// As NFS-e de produção que fecharam no mês (autorizadas e canceladas), com os XMLs — para o
/// pacote do contador. O mês é o da autorização.
pub(crate) fn do_mes(
    c: &Connection,
    empresa: Id,
    de: Instante,
    ate: Instante,
) -> Resultado<Vec<crate::fechamento::NfseDoMes>> {
    let mut stmt = c
        .prepare(
            "SELECT numero_nfse, chave, total, estado, xml, xml_evento, autorizado_em
             FROM fiscal_nfse
             WHERE empresa = ?1 AND ambiente = 'producao'
               AND estado IN ('Autorizada', 'Cancelada')
               AND autorizado_em >= ?2 AND autorizado_em < ?3
             ORDER BY numero_nfse",
        )
        .map_err(falha)?;
    let v = stmt
        .query_map(
            params![
                empresa.em_bytes().as_slice(),
                de.em_micros(),
                ate.em_micros()
            ],
            |r| {
                Ok(crate::fechamento::NfseDoMes {
                    numero: r.get(0)?,
                    chave: r.get(1)?,
                    total: Dinheiro::centavos(r.get(2)?),
                    cancelada: r.get::<_, String>(3)? == "Cancelada",
                    xml: r.get(4)?,
                    xml_evento: r.get(5)?,
                    autorizada_em: Instante::de_micros(r.get(6)?),
                })
            },
        )
        .map_err(falha)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(falha);
    v
}
