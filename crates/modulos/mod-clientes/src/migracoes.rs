//! O esquema do módulo de clientes — as tabelas `clientes_*`.
//!
//! `docs/modulos/clientes.md` §13. Declara `depende_de: &["nucleo"]` — `nucleo_empresa` já
//! existe quando estas rodam. Fatia mínima da v1: cadastro, papéis, documento e limite de
//! crédito. A v2 acrescenta contato e endereço — carteira/dedup continuam para depois (ver
//! `src/lib.rs`).

use cardeal_storage::{ConjuntoMigracoes, Migracao, TipoMigracao};

const SQL_CADASTRO: &str = r"
CREATE TABLE clientes_pessoa (
    id                       BLOB PRIMARY KEY,
    empresa                  BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    tipo                     TEXT    NOT NULL CHECK (tipo IN ('Fisica','Juridica')),
    nome                     TEXT    NOT NULL,
    nome_fantasia            TEXT,
    data_nascimento_abertura INTEGER,
    estado                   TEXT    NOT NULL CHECK (estado IN ('Ativa','Inativa','Anonimizada')),
    anonimizado_em           INTEGER,
    observacao               TEXT,
    versao                   INTEGER NOT NULL DEFAULT 1,
    criado_em                INTEGER NOT NULL
) STRICT;
CREATE INDEX clientes_pessoa_nome ON clientes_pessoa(empresa, nome) WHERE estado = 'Ativa';

CREATE TABLE clientes_papel (
    id          BLOB PRIMARY KEY,
    empresa     BLOB    NOT NULL,
    pessoa      BLOB    NOT NULL REFERENCES clientes_pessoa(id),
    papel       TEXT    NOT NULL CHECK (papel IN ('Cliente','Fornecedor','Transportadora','Funcionario','Socio','Vendedor')),
    ativo_desde INTEGER NOT NULL,
    ativo       INTEGER NOT NULL DEFAULT 1 CHECK (ativo IN (0,1)),
    UNIQUE (pessoa, papel)
) STRICT;
CREATE INDEX clientes_papel_tipo ON clientes_papel(empresa, papel) WHERE ativo = 1;

CREATE TABLE clientes_documento (
    id                  BLOB PRIMARY KEY,
    empresa             BLOB    NOT NULL,
    pessoa              BLOB    NOT NULL REFERENCES clientes_pessoa(id),
    tipo                TEXT    NOT NULL CHECK (tipo IN ('Cpf','Cnpj','Rg','Ie','Im','Passaporte')),
    numero              TEXT    NOT NULL,
    orgao_emissor       TEXT,
    validado_sefaz_em   INTEGER,
    UNIQUE (empresa, tipo, numero)
) STRICT;

CREATE TABLE clientes_limite_credito (
    id                BLOB PRIMARY KEY,
    empresa           BLOB    NOT NULL,
    pessoa            BLOB    NOT NULL REFERENCES clientes_pessoa(id),
    limite            INTEGER NOT NULL,
    situacao          TEXT    NOT NULL CHECK (situacao IN ('Liberado','Bloqueado')),
    motivo_bloqueio   TEXT,
    bloqueado_em      INTEGER,
    liberado_por      BLOB,
    revisado_em       INTEGER NOT NULL,
    versao            INTEGER NOT NULL DEFAULT 1,
    UNIQUE (pessoa)
) STRICT;
";

const SQL_CONTATO_ENDERECO: &str = r"
CREATE TABLE clientes_contato (
    id        BLOB PRIMARY KEY,
    empresa   BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    pessoa    BLOB    NOT NULL REFERENCES clientes_pessoa(id),
    tipo      TEXT    NOT NULL CHECK (tipo IN ('Telefone','Celular','Email','Whatsapp')),
    valor     TEXT    NOT NULL,
    principal INTEGER NOT NULL DEFAULT 0 CHECK (principal IN (0,1))
) STRICT;
CREATE INDEX clientes_contato_pessoa ON clientes_contato(pessoa);

CREATE TABLE clientes_endereco (
    id          BLOB PRIMARY KEY,
    empresa     BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    pessoa      BLOB    NOT NULL REFERENCES clientes_pessoa(id),
    tipo        TEXT    NOT NULL CHECK (tipo IN ('Cobranca','Entrega','Comercial','Residencial')),
    logradouro  TEXT    NOT NULL,
    numero      TEXT    NOT NULL,
    complemento TEXT,
    bairro      TEXT    NOT NULL,
    cidade      TEXT    NOT NULL,
    uf          TEXT    NOT NULL,
    cep         TEXT    NOT NULL,
    principal   INTEGER NOT NULL DEFAULT 0 CHECK (principal IN (0,1))
) STRICT;
CREATE INDEX clientes_endereco_pessoa ON clientes_endereco(pessoa);
";

const MIGRACOES: &[Migracao] = &[
    Migracao {
        versao: 1,
        nome: "clientes_cadastro_papel_documento_credito",
        sql: SQL_CADASTRO,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 2,
        nome: "clientes_contato_e_endereco",
        sql: SQL_CONTATO_ENDERECO,
        tipo: TipoMigracao::Esquema,
    },
];

/// O conjunto de migrações do módulo de clientes.
#[must_use]
pub fn conjunto() -> ConjuntoMigracoes {
    ConjuntoMigracoes {
        modulo: "clientes",
        depende_de: &["nucleo"],
        migracoes: MIGRACOES,
    }
}
