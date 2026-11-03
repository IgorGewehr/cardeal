//! A configuração do SPED, a apuração registrada de cada mês e a geração do arquivo.

use cardeal_kernel::{CodigoErro, Competencia, Dinheiro, Erro, Resultado};
use cardeal_modkit::{Consulta, Ctx};
use cardeal_storage::UnidadeDeTrabalho;
use rusqlite::{params, Connection, OptionalExtension};

use super::{
    gerar_efd, ConfiguracaoEfd, ConfigurarEfd, DadosDaEfd, EfdDoMes, GerarEfd, NotaPropria,
    RegistrarApuracaoIcms, SituacaoPropria,
};
use crate::{NotasFiscaisDoMes, SituacaoNoMes};

#[allow(clippy::needless_pass_by_value)]
fn falha(e: rusqlite::Error) -> Erro {
    Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string())
}

fn mes(c: Competencia) -> i64 {
    i64::from(c.ano()) * 100 + i64::from(c.mes())
}

pub(super) fn configuracao(
    c: &Connection,
    empresa: cardeal_kernel::Id,
) -> Resultado<ConfiguracaoEfd> {
    let dados: Option<Vec<u8>> = c
        .query_row(
            "SELECT dados FROM fiscal_configuracao_efd WHERE empresa = ?1",
            [empresa.em_bytes().as_slice()],
            |r| r.get(0),
        )
        .optional()
        .map_err(falha)?;
    dados.map_or_else(
        || Ok(ConfiguracaoEfd::default()),
        |d| {
            postcard::from_bytes(&d)
                .map_err(|e| Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string()))
        },
    )
}

pub(super) fn configurar(
    cmd: &ConfigurarEfd,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<()> {
    let dia = cmd.configuracao.dia_vencimento;
    if dia > 31 {
        return Err(Erro::novo(
            CodigoErro::VALOR_INVALIDO,
            "O dia do vencimento vai de 1 a 31",
        )
        .no_campo("dia_vencimento"));
    }
    let dados = postcard::to_allocvec(&cmd.configuracao)
        .map_err(|e| Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string()))?;
    uow.conexao()
        .execute(
            "INSERT INTO fiscal_configuracao_efd (empresa, dados, atualizado_em)
             VALUES (?1, ?2, ?3)
             ON CONFLICT (empresa) DO UPDATE SET dados = ?2, atualizado_em = ?3",
            params![
                ctx.empresa.em_bytes().as_slice(),
                dados,
                ctx.agora.em_micros()
            ],
        )
        .map_err(falha)?;
    Ok(())
}

/// O saldo credor que a apuração registrada de `competencia` mandou para o mês seguinte.
pub(super) fn saldo_registrado(
    c: &Connection,
    empresa: cardeal_kernel::Id,
    competencia: Competencia,
) -> Resultado<Option<Dinheiro>> {
    c.query_row(
        "SELECT saldo_credor_transportar FROM fiscal_apuracao_icms
         WHERE empresa = ?1 AND competencia = ?2",
        params![empresa.em_bytes().as_slice(), mes(competencia)],
        |r| r.get::<_, i64>(0),
    )
    .optional()
    .map(|v| v.map(Dinheiro::centavos))
    .map_err(falha)
}

pub(super) fn registrar(
    cmd: &RegistrarApuracaoIcms,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<()> {
    let a = &cmd.apuracao;
    uow.conexao()
        .execute(
            "INSERT INTO fiscal_apuracao_icms
               (empresa, competencia, debitos, creditos, a_recolher, saldo_credor_transportar,
                registrado_em)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT (empresa, competencia) DO UPDATE SET
               debitos = ?3, creditos = ?4, a_recolher = ?5, saldo_credor_transportar = ?6,
               registrado_em = ?7",
            params![
                ctx.empresa.em_bytes().as_slice(),
                mes(cmd.competencia),
                a.debitos.em_centavos(),
                a.creditos.em_centavos(),
                a.a_recolher.em_centavos(),
                a.saldo_credor_a_transportar.em_centavos(),
                ctx.agora.em_micros()
            ],
        )
        .map_err(falha)?;
    Ok(())
}

pub(super) fn gerar(cmd: &GerarEfd, ctx: &Ctx, c: &Connection) -> Resultado<EfdDoMes> {
    // O regime antes de tudo: a empresa do Simples não tem o que configurar.
    let regime = mod_empresa::sql::dados(c, ctx.empresa)?.regime;
    if !matches!(regime.as_str(), "LucroPresumido" | "LucroReal") {
        return Ok(EfdDoMes {
            arquivo: None,
            impedimentos: vec![super::EFD_DISPENSADA.to_owned()],
        });
    }
    let emitente = match crate::nfce::sqlite::emitente(c, ctx.empresa)? {
        Ok(e) => e,
        Err(faltas) => {
            return Ok(EfdDoMes {
                arquivo: None,
                impedimentos: faltas,
            })
        }
    };
    let email = mod_empresa::sql::identidade(c, ctx.empresa)?.email;
    let configuracao = configuracao(c, ctx.empresa)?;
    let notas = NotasFiscaisDoMes {
        mes: cmd.competencia,
    }
    .executar(ctx, c)?;
    let proprias: Vec<NotaPropria> = notas
        .notas
        .into_iter()
        .filter_map(|n| {
            let situacao = match n.situacao {
                SituacaoNoMes::Autorizada => SituacaoPropria::Autorizada,
                SituacaoNoMes::Cancelada => SituacaoPropria::Cancelada,
                // A denegada e a inutilizada não se escrituram desde 2023.
                SituacaoNoMes::Denegada | SituacaoNoMes::Inutilizada => return None,
            };
            Some(NotaPropria {
                situacao,
                modelo: n.modelo,
                serie: n.serie,
                numero: n.numero,
                chave: n.chave,
                xml: n.xml,
            })
        })
        .collect();
    let anterior = cmd.competencia.anterior();
    let (saldo, sem_registro) = match cmd.saldo_credor_anterior {
        Some(s) => (s, false),
        None => match saldo_registrado(c, ctx.empresa, anterior)? {
            Some(s) => (s, false),
            None => (Dinheiro::ZERO, true),
        },
    };
    let gerado = gerar_efd(&DadosDaEfd {
        competencia: cmd.competencia,
        substituta: cmd.substituta,
        emitente: &emitente,
        email: &email,
        configuracao: &configuracao,
        saldo_credor_anterior: saldo,
        proprias: &proprias,
        entradas: &cmd.entradas,
    });
    Ok(match gerado {
        Ok(mut arquivo) => {
            if sem_registro {
                arquivo.avisos.insert(
                    0,
                    format!(
                        "Sem a apuração de {} registrada: o saldo credor anterior saiu zero — \
                         informe-o se a empresa tinha crédito.",
                        anterior.formatar()
                    ),
                );
            }
            if !notas.sem_desfecho.is_empty() {
                arquivo.avisos.push(format!(
                    "{} nota(s) do mês sem desfecho (nem autorizada, nem inutilizada) ficaram fora.",
                    notas.sem_desfecho.len()
                ));
            }
            EfdDoMes {
                arquivo: Some(arquivo),
                impedimentos: Vec::new(),
            }
        }
        Err(impedimentos) => EfdDoMes {
            arquivo: None,
            impedimentos,
        },
    })
}
