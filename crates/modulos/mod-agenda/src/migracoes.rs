//! O esquema do módulo de agenda — as tabelas `agenda_*` (`docs/modulos/agenda.md` §13).
//!
//! `Lembrete`/`agenda_lembrete` ficam para quando houver um consumidor real (nenhum comando
//! os dispara ainda — ver `src/lib.rs`), então esta v1 não cria a tabela.
//!
//! `agenda_compromisso.cliente` referencia `clientes_pessoa(id)` de verdade — a agenda
//! precisa estar interligada ao cadastro de clientes, não só carregar um `Id` solto — por
//! isso `depende_de: &["nucleo", "clientes"]` abaixo, para a migração de `clientes` já ter
//! rodado quando esta rodar.

use cardeal_storage::{ConjuntoMigracoes, Migracao, TipoMigracao};

const SQL_AGENDA: &str = r"
CREATE TABLE agenda_recurso (
    id          BLOB PRIMARY KEY,
    empresa     BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    nome        TEXT    NOT NULL,
    tipo        TEXT    NOT NULL CHECK (tipo IN ('Sala','Tecnico','Equipamento','Pessoa')),
    capacidade  INTEGER,
    ativo       INTEGER NOT NULL DEFAULT 1 CHECK (ativo IN (0,1))
) STRICT;
CREATE UNIQUE INDEX agenda_recurso_nome_ativo ON agenda_recurso(empresa, nome) WHERE ativo = 1;

CREATE TABLE agenda_disponibilidade (
    id           BLOB PRIMARY KEY,
    recurso      BLOB    NOT NULL REFERENCES agenda_recurso(id),
    dia_semana   INTEGER NOT NULL CHECK (dia_semana BETWEEN 0 AND 6),
    hora_inicio  INTEGER NOT NULL,
    hora_fim     INTEGER NOT NULL
) STRICT;
CREATE INDEX agenda_disponibilidade_recurso ON agenda_disponibilidade(recurso, dia_semana);

CREATE TABLE agenda_compromisso (
    id             BLOB PRIMARY KEY,
    empresa        BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    titulo         TEXT    NOT NULL,
    tipo           TEXT    NOT NULL CHECK (tipo IN ('Interno','Cliente')),
    cliente        BLOB    REFERENCES clientes_pessoa(id),
    inicio         INTEGER NOT NULL,
    fim            INTEGER NOT NULL,
    estado         TEXT    NOT NULL CHECK (estado IN ('Agendado','Confirmado','EmAndamento','Concluido','Cancelado','NaoCompareceu')),
    origem_modulo  TEXT,
    origem_id      BLOB,
    criado_por     BLOB    NOT NULL,
    versao         INTEGER NOT NULL DEFAULT 1,
    CHECK (fim > inicio)
) STRICT;
CREATE INDEX agenda_compromisso_periodo
    ON agenda_compromisso(empresa, inicio, fim) WHERE estado NOT IN ('Cancelado');

CREATE TABLE agenda_compromisso_recurso (
    compromisso  BLOB NOT NULL REFERENCES agenda_compromisso(id),
    recurso      BLOB NOT NULL REFERENCES agenda_recurso(id),
    PRIMARY KEY (compromisso, recurso)
) STRICT, WITHOUT ROWID;
CREATE INDEX agenda_compromisso_recurso_inv ON agenda_compromisso_recurso(recurso, compromisso);
";

const MIGRACOES: &[Migracao] = &[Migracao {
    versao: 1,
    nome: "agenda_compromisso_e_recurso",
    sql: SQL_AGENDA,
    tipo: TipoMigracao::Esquema,
}];

/// O conjunto de migrações do módulo de agenda.
#[must_use]
pub fn conjunto() -> ConjuntoMigracoes {
    ConjuntoMigracoes {
        modulo: "agenda",
        depende_de: &["nucleo", "clientes"],
        migracoes: MIGRACOES,
    }
}
