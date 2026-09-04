//! O esquema do módulo financeiro — as tabelas `financeiro_*`.
//!
//! `docs/06-modelo-de-dados.md` §1 (convenções) e `docs/modulos/financeiro.md` §3. Declara
//! `depende_de: &["nucleo", "razao"]` — `nucleo_empresa` e `razao_conta` já existem quando
//! estas rodam.

use cardeal_storage::{ConjuntoMigracoes, Migracao, TipoMigracao};

const SQL_TITULOS: &str = r"
CREATE TABLE financeiro_titulo (
    id               BLOB PRIMARY KEY,
    empresa          BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    especie          TEXT    NOT NULL CHECK (especie IN ('Receber','Pagar')),
    contraparte_tipo TEXT    NOT NULL CHECK (contraparte_tipo IN ('Cliente','Fornecedor','Funcionario','Socio','Outro')),
    contraparte_id   BLOB    NOT NULL,
    origem_modulo    TEXT    NOT NULL,
    origem_id        BLOB,
    emissao          INTEGER NOT NULL,
    valor_original   INTEGER NOT NULL,
    forma_cobranca   TEXT    NOT NULL,
    centro_custo     BLOB,
    observacao       TEXT,
    cancelado_em     INTEGER,
    versao           INTEGER NOT NULL DEFAULT 1,
    criado_em        INTEGER NOT NULL,
    criado_por       BLOB    NOT NULL
) STRICT;
CREATE INDEX financeiro_titulo_contraparte
    ON financeiro_titulo(empresa, contraparte_tipo, contraparte_id);

CREATE TABLE financeiro_parcela (
    id             BLOB PRIMARY KEY,
    empresa        BLOB    NOT NULL,
    titulo         BLOB    NOT NULL REFERENCES financeiro_titulo(id),
    numero         INTEGER NOT NULL,
    vencimento     INTEGER NOT NULL,
    valor          INTEGER NOT NULL,
    estado         TEXT    NOT NULL CHECK (estado IN ('Aberta','Parcial','Quitada','Cancelada','Renegociada')),
    valor_baixado  INTEGER NOT NULL DEFAULT 0,
    lancamento     BLOB,
    nosso_numero   TEXT,
    politica_juros TEXT    NOT NULL CHECK (politica_juros IN ('Nenhum','SimplesDiario','SimplesMensal')),
    taxa_juros     INTEGER,
    multa          INTEGER,
    desconto_ate   INTEGER,
    desconto_valor INTEGER,
    versao         INTEGER NOT NULL DEFAULT 1
) STRICT;
CREATE INDEX financeiro_parcela_titulo ON financeiro_parcela(titulo, numero);
CREATE INDEX financeiro_parcela_vencendo
    ON financeiro_parcela(empresa, vencimento) WHERE estado IN ('Aberta','Parcial');

CREATE TABLE financeiro_baixa (
    id             BLOB PRIMARY KEY,
    empresa        BLOB    NOT NULL,
    parcela        BLOB    NOT NULL REFERENCES financeiro_parcela(id),
    data           INTEGER NOT NULL,
    valor_recebido INTEGER NOT NULL,
    principal      INTEGER NOT NULL,
    juros          INTEGER NOT NULL,
    multa          INTEGER NOT NULL,
    desconto       INTEGER NOT NULL,
    lancamento     BLOB    NOT NULL,
    estornada_em   INTEGER,
    criado_em      INTEGER NOT NULL,
    criado_por     BLOB    NOT NULL
) STRICT;
CREATE INDEX financeiro_baixa_parcela ON financeiro_baixa(parcela, data);
";

const MIGRACOES: &[Migracao] = &[Migracao {
    versao: 1,
    nome: "financeiro_titulos_e_baixas",
    sql: SQL_TITULOS,
    tipo: TipoMigracao::Esquema,
}];

/// O conjunto de migrações do módulo financeiro.
#[must_use]
pub fn conjunto() -> ConjuntoMigracoes {
    ConjuntoMigracoes {
        modulo: "financeiro",
        depende_de: &["nucleo", "razao"],
        migracoes: MIGRACOES,
    }
}
