//! Todo o SQL do módulo de agenda (`docs/15-convencoes-codigo.md` §5): um único lugar com
//! strings SQL, sempre colunas explícitas.

use cardeal_kernel::{CodigoErro, Erro, Id, Instante, Resultado, Versao};
use cardeal_storage::UnidadeDeTrabalho;
use rusqlite::{params, Connection, OptionalExtension};

use crate::compromisso::{Compromisso, EstadoCompromisso, TipoCompromisso};
use crate::recurso::{DisponibilidadeRecurso, Recurso, TipoRecurso};

#[allow(clippy::needless_pass_by_value)] // usado como `.map_err(persist)`
pub(crate) fn persist(e: rusqlite::Error) -> Erro {
    Erro::novo(CodigoErro::FALHA_INTERNA, format!("agenda/SQL: {e}"))
}

pub(crate) fn blob(id: Id) -> Vec<u8> {
    id.em_bytes().to_vec()
}

fn blob_opt(id: Option<Id>) -> Option<Vec<u8>> {
    id.map(blob)
}

pub(crate) fn id_de(bytes: Vec<u8>) -> Id {
    <[u8; 16]>::try_from(bytes).map_or(Id::NULO, Id::de_bytes)
}

fn versao_de(n: i64) -> Versao {
    Versao::nova(u64::try_from(n).unwrap_or(1))
}

fn versao_i64(v: Versao) -> i64 {
    i64::try_from(v.numero()).unwrap_or(i64::MAX)
}

fn tipo_recurso_txt(t: TipoRecurso) -> &'static str {
    match t {
        TipoRecurso::Sala => "Sala",
        TipoRecurso::Tecnico => "Tecnico",
        TipoRecurso::Equipamento => "Equipamento",
        TipoRecurso::Pessoa => "Pessoa",
    }
}

fn tipo_recurso_de(s: &str) -> TipoRecurso {
    match s {
        "Tecnico" => TipoRecurso::Tecnico,
        "Equipamento" => TipoRecurso::Equipamento,
        "Pessoa" => TipoRecurso::Pessoa,
        _ => TipoRecurso::Sala,
    }
}

fn tipo_compromisso_txt(t: TipoCompromisso) -> &'static str {
    match t {
        TipoCompromisso::Interno => "Interno",
        TipoCompromisso::Cliente => "Cliente",
    }
}

fn tipo_compromisso_de(s: &str) -> TipoCompromisso {
    match s {
        "Cliente" => TipoCompromisso::Cliente,
        _ => TipoCompromisso::Interno,
    }
}

fn estado_txt(e: EstadoCompromisso) -> &'static str {
    e.rotulo()
}

fn estado_de(s: &str) -> EstadoCompromisso {
    match s {
        "Confirmado" => EstadoCompromisso::Confirmado,
        "EmAndamento" => EstadoCompromisso::EmAndamento,
        "Concluido" => EstadoCompromisso::Concluido,
        "Cancelado" => EstadoCompromisso::Cancelado,
        "NaoCompareceu" => EstadoCompromisso::NaoCompareceu,
        _ => EstadoCompromisso::Agendado,
    }
}

/// Grava e lê recursos, disponibilidade e compromissos da agenda.
pub struct RepositorioAgenda<'a, 'b> {
    uow: &'a mut UnidadeDeTrabalho<'b>,
}

impl<'a, 'b> RepositorioAgenda<'a, 'b> {
    /// Cria o repositório sobre a unidade de trabalho corrente.
    pub fn novo(uow: &'a mut UnidadeDeTrabalho<'b>) -> Self {
        Self { uow }
    }

    fn conn(&self) -> &Connection {
        self.uow.conexao()
    }

    /// Grava um recurso novo.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn inserir_recurso(&mut self, r: &Recurso) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO agenda_recurso (id, empresa, nome, tipo, capacidade, ativo)
                 VALUES (?1,?2,?3,?4,?5,?6)",
                params![
                    blob(r.id),
                    blob(r.empresa),
                    r.nome,
                    tipo_recurso_txt(r.tipo),
                    r.capacidade,
                    i64::from(r.ativo),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Busca um recurso ativo pelo nome (case-sensível) — a checagem de `NomeDuplicado`.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn recurso_ativo_por_nome(&self, empresa: Id, nome: &str) -> Resultado<Option<Recurso>> {
        self.conn()
            .query_row(
                "SELECT id, empresa, nome, tipo, capacidade, ativo
                 FROM agenda_recurso WHERE empresa = ?1 AND nome = ?2 AND ativo = 1",
                params![blob(empresa), nome],
                recurso_de_linha,
            )
            .optional()
            .map_err(persist)
    }

    /// Busca um recurso pelo id. `Ok(None)` = não existe.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn buscar_recurso(&self, id: Id) -> Resultado<Option<Recurso>> {
        self.conn()
            .query_row(
                "SELECT id, empresa, nome, tipo, capacidade, ativo
                 FROM agenda_recurso WHERE id = ?1",
                [blob(id)],
                recurso_de_linha,
            )
            .optional()
            .map_err(persist)
    }

    /// Grava uma regra de disponibilidade.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn inserir_disponibilidade(&mut self, d: &DisponibilidadeRecurso) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO agenda_disponibilidade (id, recurso, dia_semana, hora_inicio, hora_fim)
                 VALUES (?1,?2,?3,?4,?5)",
                params![
                    blob(d.id),
                    blob(d.recurso),
                    i64::from(d.dia_semana),
                    i64::from(d.hora_inicio),
                    i64::from(d.hora_fim),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Todas as regras de disponibilidade de um recurso.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn disponibilidade_do_recurso(
        &self,
        recurso: Id,
    ) -> Resultado<Vec<DisponibilidadeRecurso>> {
        crate::consultas::disponibilidade_do_recurso(self.conn(), recurso)
    }

    /// Grava um compromisso novo e os vínculos com seus recursos, na mesma transação.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn inserir_compromisso(&mut self, c: &Compromisso, recursos: &[Id]) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO agenda_compromisso
                   (id, empresa, titulo, tipo, cliente, inicio, fim, estado, origem_modulo,
                    origem_id, criado_por, versao)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
                params![
                    blob(c.id),
                    blob(c.empresa),
                    c.titulo,
                    tipo_compromisso_txt(c.tipo),
                    blob_opt(c.cliente),
                    c.inicio.em_micros(),
                    c.fim.em_micros(),
                    estado_txt(c.estado),
                    c.origem_modulo,
                    blob_opt(c.origem_id),
                    blob(c.criado_por),
                    versao_i64(c.versao),
                ],
            )
            .map_err(persist)?;
        for recurso in recursos {
            self.conn()
                .execute(
                    "INSERT INTO agenda_compromisso_recurso (compromisso, recurso) VALUES (?1,?2)",
                    params![blob(c.id), blob(*recurso)],
                )
                .map_err(persist)?;
        }
        Ok(())
    }

    /// Regrava o estado/versão de um compromisso (transições de estado).
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn atualizar_estado(&mut self, c: &Compromisso) -> Resultado<()> {
        self.conn()
            .execute(
                "UPDATE agenda_compromisso SET estado = ?2, versao = ?3 WHERE id = ?1",
                params![blob(c.id), estado_txt(c.estado), versao_i64(c.versao)],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Busca um compromisso pelo id. `Ok(None)` = não existe.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn buscar_compromisso(&self, id: Id) -> Resultado<Option<Compromisso>> {
        self.conn()
            .query_row(
                "SELECT id, empresa, titulo, tipo, cliente, inicio, fim, estado, origem_modulo,
                        origem_id, criado_por, versao
                 FROM agenda_compromisso WHERE id = ?1",
                [blob(id)],
                compromisso_de_linha,
            )
            .optional()
            .map_err(persist)
    }

    /// Os recursos vinculados a um compromisso.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn recursos_do_compromisso(&self, compromisso: Id) -> Resultado<Vec<Id>> {
        let mut stmt = self
            .conn()
            .prepare("SELECT recurso FROM agenda_compromisso_recurso WHERE compromisso = ?1")
            .map_err(persist)?;
        let linhas = stmt
            .query_map([blob(compromisso)], |r| r.get::<_, Vec<u8>>(0))
            .map_err(persist)?;
        linhas
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)
            .map(|v| v.into_iter().map(id_de).collect())
    }

    /// Compromissos ativos (não `Cancelado`) do recurso que sobrepõem `[inicio, fim)` — a
    /// checagem de `ConflitoDeAgenda` e a consulta `ConflitosDeAgenda`.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn compromissos_do_recurso_sobrepondo(
        &self,
        recurso: Id,
        inicio: Instante,
        fim: Instante,
    ) -> Resultado<Vec<Compromisso>> {
        crate::consultas::compromissos_do_recurso_sobrepondo(self.conn(), recurso, inicio, fim)
    }

    /// Os compromissos do recurso num período — `AgendaDoRecurso`.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn compromissos_do_recurso_no_periodo(
        &self,
        recurso: Id,
        inicio: Instante,
        fim: Instante,
    ) -> Resultado<Vec<Compromisso>> {
        crate::consultas::compromissos_do_recurso_no_periodo(self.conn(), recurso, inicio, fim)
    }

    /// Os próximos compromissos ativos da empresa a partir de `a_partir_de` — `docs/modulos/
    /// agenda.md` §6, `ProximosCompromissos`. Teto de 200 linhas, mesmo raciocínio de
    /// `mod_financeiro::titulos_em_aberto`.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn proximos_compromissos(
        &self,
        empresa: Id,
        a_partir_de: Instante,
    ) -> Resultado<Vec<Compromisso>> {
        crate::consultas::proximos_compromissos(self.conn(), empresa, a_partir_de)
    }

    /// Compromissos `Confirmado` cujo `fim` já passou de `agora` — a varredura de
    /// `registrar_nao_comparecimento_pendentes`.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn confirmados_vencidos(
        &self,
        empresa: Id,
        agora: Instante,
    ) -> Resultado<Vec<Compromisso>> {
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT id, empresa, titulo, tipo, cliente, inicio, fim, estado, origem_modulo,
                        origem_id, criado_por, versao
                 FROM agenda_compromisso
                 WHERE empresa = ?1 AND estado = 'Confirmado' AND fim <= ?2",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map(
                params![blob(empresa), agora.em_micros()],
                compromisso_de_linha,
            )
            .map_err(persist)?;
        linhas
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)
    }
}

fn recurso_de_linha(r: &rusqlite::Row<'_>) -> rusqlite::Result<Recurso> {
    Ok(Recurso {
        id: id_de(r.get::<_, Vec<u8>>(0)?),
        empresa: id_de(r.get::<_, Vec<u8>>(1)?),
        nome: r.get(2)?,
        tipo: tipo_recurso_de(&r.get::<_, String>(3)?),
        capacidade: r
            .get::<_, Option<i64>>(4)?
            .map(|n| u32::try_from(n).unwrap_or(0)),
        ativo: r.get::<_, i64>(5)? != 0,
    })
}

pub(crate) fn disponibilidade_de_linha(
    r: &rusqlite::Row<'_>,
) -> rusqlite::Result<DisponibilidadeRecurso> {
    Ok(DisponibilidadeRecurso {
        id: id_de(r.get::<_, Vec<u8>>(0)?),
        recurso: id_de(r.get::<_, Vec<u8>>(1)?),
        dia_semana: u8::try_from(r.get::<_, i64>(2)?).unwrap_or(0),
        hora_inicio: u16::try_from(r.get::<_, i64>(3)?).unwrap_or(0),
        hora_fim: u16::try_from(r.get::<_, i64>(4)?).unwrap_or(0),
    })
}

pub(crate) fn compromisso_de_linha(r: &rusqlite::Row<'_>) -> rusqlite::Result<Compromisso> {
    Ok(Compromisso {
        id: id_de(r.get::<_, Vec<u8>>(0)?),
        empresa: id_de(r.get::<_, Vec<u8>>(1)?),
        titulo: r.get(2)?,
        tipo: tipo_compromisso_de(&r.get::<_, String>(3)?),
        cliente: r.get::<_, Option<Vec<u8>>>(4)?.map(id_de),
        inicio: Instante::de_micros(r.get::<_, i64>(5)?),
        fim: Instante::de_micros(r.get::<_, i64>(6)?),
        estado: estado_de(&r.get::<_, String>(7)?),
        origem_modulo: r.get::<_, Option<String>>(8)?,
        origem_id: r.get::<_, Option<Vec<u8>>>(9)?.map(id_de),
        criado_por: id_de(r.get::<_, Vec<u8>>(10)?),
        versao: versao_de(r.get::<_, i64>(11)?),
    })
}
