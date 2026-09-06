//! As consultas da agenda: leitura autorizada sobre `agenda_compromisso`/`agenda_disponibilidade`
//! (`docs/modulos/agenda.md` §6).

use cardeal_kernel::{Id, Instante, Resultado};
use cardeal_modkit::{Consulta, Ctx};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::compromisso::Compromisso;
use crate::recurso::DisponibilidadeRecurso;
use crate::repositorio::{blob, compromisso_de_linha, disponibilidade_de_linha, id_de, persist};

/// As regras de disponibilidade de um recurso — sem elas, o recurso é sempre disponível.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn disponibilidade_do_recurso(
    conexao: &Connection,
    recurso: Id,
) -> Resultado<Vec<DisponibilidadeRecurso>> {
    let mut stmt = conexao
        .prepare(
            "SELECT id, recurso, dia_semana, hora_inicio, hora_fim
             FROM agenda_disponibilidade WHERE recurso = ?1",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map([blob(recurso)], disponibilidade_de_linha)
        .map_err(persist)?;
    linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)
}

/// Compromissos ativos (não `Cancelado`) do recurso que sobrepõem `[inicio, fim)` — a mesma
/// consulta usada internamente por `CriarCompromisso` para detectar `ConflitoDeAgenda`.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn compromissos_do_recurso_sobrepondo(
    conexao: &Connection,
    recurso: Id,
    inicio: Instante,
    fim: Instante,
) -> Resultado<Vec<Compromisso>> {
    let mut stmt = conexao
        .prepare(
            "SELECT c.id, c.empresa, c.titulo, c.tipo, c.cliente, c.inicio, c.fim, c.estado,
                    c.origem_modulo, c.origem_id, c.criado_por, c.versao
             FROM agenda_compromisso c
             JOIN agenda_compromisso_recurso cr ON cr.compromisso = c.id
             WHERE cr.recurso = ?1 AND c.estado != 'Cancelado'
                   AND c.inicio < ?2 AND c.fim > ?3",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map(
            params![blob(recurso), fim.em_micros(), inicio.em_micros()],
            compromisso_de_linha,
        )
        .map_err(persist)?;
    linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)
}

/// Os compromissos do recurso num período — o calendário por sala/técnico.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn compromissos_do_recurso_no_periodo(
    conexao: &Connection,
    recurso: Id,
    inicio: Instante,
    fim: Instante,
) -> Resultado<Vec<Compromisso>> {
    let mut stmt = conexao
        .prepare(
            "SELECT c.id, c.empresa, c.titulo, c.tipo, c.cliente, c.inicio, c.fim, c.estado,
                    c.origem_modulo, c.origem_id, c.criado_por, c.versao
             FROM agenda_compromisso c
             JOIN agenda_compromisso_recurso cr ON cr.compromisso = c.id
             WHERE cr.recurso = ?1 AND c.inicio < ?2 AND c.fim > ?3
             ORDER BY c.inicio",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map(
            params![blob(recurso), fim.em_micros(), inicio.em_micros()],
            compromisso_de_linha,
        )
        .map_err(persist)?;
    linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)
}

/// Os próximos compromissos ativos da empresa a partir de `a_partir_de`, teto de 200 linhas
/// (mesmo raciocínio de `mod_financeiro::titulos_em_aberto` — sem `Pagina`/`Cursor` ainda).
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn proximos_compromissos(
    conexao: &Connection,
    empresa: Id,
    a_partir_de: Instante,
) -> Resultado<Vec<Compromisso>> {
    let mut stmt = conexao
        .prepare(
            "SELECT id, empresa, titulo, tipo, cliente, inicio, fim, estado, origem_modulo,
                    origem_id, criado_por, versao
             FROM agenda_compromisso
             WHERE empresa = ?1 AND fim > ?2 AND estado != 'Cancelado'
             ORDER BY inicio ASC
             LIMIT 200",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map(
            params![blob(empresa), a_partir_de.em_micros()],
            compromisso_de_linha,
        )
        .map_err(persist)?;
    linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)
}

/// O calendário de um recurso num período — `docs/modulos/agenda.md` §6, `AgendaDoRecurso`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgendaDoRecurso {
    /// O recurso.
    pub recurso: Id,
    /// Início do período.
    pub inicio: Instante,
    /// Fim do período.
    pub fim: Instante,
}

impl Consulta for AgendaDoRecurso {
    type Saida = Vec<Compromisso>;
    const PERMISSAO: &'static str = "agenda.compromisso.ver";

    fn executar(self, _ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        compromissos_do_recurso_no_periodo(conexao, self.recurso, self.inicio, self.fim)
    }
}

/// As regras de disponibilidade de um recurso — `docs/modulos/agenda.md` §6,
/// `DisponibilidadeNoPeriodo`. Lista as janelas recorrentes; calcular o próximo horário livre
/// concreto é trabalho da UI sobre esta base (mesmo padrão de "agregação em memória" de
/// `mod_financeiro`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisponibilidadeNoPeriodo {
    /// O recurso.
    pub recurso: Id,
}

impl Consulta for DisponibilidadeNoPeriodo {
    type Saida = Vec<DisponibilidadeRecurso>;
    const PERMISSAO: &'static str = "agenda.compromisso.ver";

    fn executar(self, _ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        disponibilidade_do_recurso(conexao, self.recurso)
    }
}

/// Compromissos que colidiriam com um horário candidato — a mesma checagem de
/// `CriarCompromisso`, exposta para a UI avisar **antes** de tentar gravar (`docs/modulos/
/// agenda.md` §6, `ConflitosDeAgenda`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflitosDeAgenda {
    /// O recurso candidato.
    pub recurso: Id,
    /// Início do horário candidato.
    pub inicio: Instante,
    /// Fim do horário candidato.
    pub fim: Instante,
}

impl Consulta for ConflitosDeAgenda {
    type Saida = Vec<Compromisso>;
    const PERMISSAO: &'static str = "agenda.compromisso.ver";

    fn executar(self, _ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        compromissos_do_recurso_sobrepondo(conexao, self.recurso, self.inicio, self.fim)
    }
}

/// Os próximos compromissos da empresa a partir de agora — "exige ação hoje" (`docs/modulos/
/// agenda.md` §6, `ProximosCompromissos`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProximosCompromissos;

impl Consulta for ProximosCompromissos {
    type Saida = Vec<Compromisso>;
    const PERMISSAO: &'static str = "agenda.compromisso.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        proximos_compromissos(conexao, ctx.empresa, ctx.agora)
    }
}

/// Todos os compromissos ativos da empresa que tocam o período `[inicio, fim)` — a fonte da
/// grade de calendário (dia/semana/mês). Sem cursor: teto de 500 linhas
/// (`docs/09-protocolo-api.md` §5), suficiente para o período visível de uma PME.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompromissosNoPeriodo {
    /// Início do período.
    pub inicio: Instante,
    /// Fim do período.
    pub fim: Instante,
}

impl Consulta for CompromissosNoPeriodo {
    type Saida = Vec<Compromisso>;
    const PERMISSAO: &'static str = "agenda.compromisso.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let mut stmt = conexao
            .prepare(
                "SELECT id, empresa, titulo, tipo, cliente, inicio, fim, estado, origem_modulo,
                        origem_id, criado_por, versao
                 FROM agenda_compromisso
                 WHERE empresa = ?1 AND inicio < ?2 AND fim > ?3 AND estado != 'Cancelado'
                 ORDER BY inicio ASC
                 LIMIT 500",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map(
                params![blob(ctx.empresa), self.fim.em_micros(), self.inicio.em_micros()],
                compromisso_de_linha,
            )
            .map_err(persist)?;
        linhas.collect::<rusqlite::Result<Vec<_>>>().map_err(persist)
    }
}

/// Os recursos ativos da empresa, por nome — alimenta o seletor de recurso da tela.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recursos;

impl Consulta for Recursos {
    type Saida = Vec<crate::recurso::Recurso>;
    const PERMISSAO: &'static str = "agenda.compromisso.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        use crate::recurso::{Recurso, TipoRecurso};
        let tipo_de = |s: &str| match s {
            "Sala" => TipoRecurso::Sala,
            "Equipamento" => TipoRecurso::Equipamento,
            "Pessoa" => TipoRecurso::Pessoa,
            _ => TipoRecurso::Tecnico,
        };
        let mut stmt = conexao
            .prepare(
                "SELECT id, empresa, nome, tipo, capacidade, ativo
                 FROM agenda_recurso WHERE empresa = ?1 AND ativo = 1 ORDER BY nome ASC",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map(params![blob(ctx.empresa)], |r| {
                Ok(Recurso {
                    id: id_de(r.get::<_, Vec<u8>>(0)?),
                    empresa: id_de(r.get::<_, Vec<u8>>(1)?),
                    nome: r.get(2)?,
                    tipo: tipo_de(&r.get::<_, String>(3)?),
                    capacidade: r.get::<_, Option<i64>>(4)?.map(|n| u32::try_from(n).unwrap_or(0)),
                    ativo: r.get::<_, i64>(5)? != 0,
                })
            })
            .map_err(persist)?;
        linhas.collect::<rusqlite::Result<Vec<_>>>().map_err(persist)
    }
}
