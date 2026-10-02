//! O esquema do diretório global, versionado por `PRAGMA user_version`. Só migrações
//! aditivas — nunca `DROP TABLE` de tabela com filhos (ver o histórico do projeto).

pub(super) const MIGRACOES: &[&str] = &[
    // v1 — contas, empresas, vínculos, sessões.
    "CREATE TABLE conta (
        id            BLOB    PRIMARY KEY,
        email         TEXT    NOT NULL UNIQUE,
        nome          TEXT    NOT NULL,
        senha_hash    TEXT    NOT NULL,
        tentativas    INTEGER NOT NULL DEFAULT 0,
        bloqueado_ate INTEGER,
        criada_em     INTEGER NOT NULL
    ) STRICT;

    CREATE TABLE empresa (
        id        BLOB    PRIMARY KEY,
        nome      TEXT    NOT NULL,
        ativa     INTEGER NOT NULL DEFAULT 1,
        criada_em INTEGER NOT NULL
    ) STRICT;

    -- Uma conta entra numa empresa como um usuário daquela base. Os papéis (o que pode fazer)
    -- moram na base da empresa; aqui só se decide se pode entrar.
    CREATE TABLE vinculo (
        conta   BLOB NOT NULL REFERENCES conta(id),
        empresa BLOB NOT NULL REFERENCES empresa(id),
        usuario BLOB NOT NULL,
        PRIMARY KEY (conta, empresa)
    ) STRICT, WITHOUT ROWID;

    CREATE TABLE sessao (
        token_hash BLOB    PRIMARY KEY,
        id         BLOB    NOT NULL,
        conta      BLOB    NOT NULL REFERENCES conta(id),
        criada_em  INTEGER NOT NULL,
        expira_em  INTEGER NOT NULL
    ) STRICT, WITHOUT ROWID;
    CREATE INDEX sessao_expira ON sessao(expira_em);",
    // v2 — organização com vários CNPJs (ADR-0017): a base em que a empresa mora. `NULL` =
    // a própria empresa (a matriz, que dá nome ao arquivo) — as empresas de antes ficam como
    // estão.
    "ALTER TABLE empresa ADD COLUMN base BLOB;",
];
