//! O esquema do módulo de orçamentos — as tabelas `orcamentos_*`.
//!
//! `depende_de: &["nucleo"]` — `nucleo_empresa` e a sequência (`nucleo_sequencia`) já
//! existem quando estas rodam.

use cardeal_storage::{ConjuntoMigracoes, Migracao, TipoMigracao};

const SQL_INICIAL: &str = r"
CREATE TABLE orcamentos_orcamento (
    id                   BLOB PRIMARY KEY,
    empresa              BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    numero               INTEGER NOT NULL,
    cliente              BLOB,
    cliente_nome         TEXT    NOT NULL,
    cliente_documento    TEXT,
    cliente_contato      TEXT,
    assunto              TEXT    NOT NULL,
    descricao            TEXT,
    data_emissao         INTEGER NOT NULL,
    validade             INTEGER NOT NULL,
    condicoes_pagamento  TEXT,
    prazo_entrega        TEXT,
    observacoes          TEXT,
    desconto_percentual  INTEGER NOT NULL DEFAULT 0,
    estado               TEXT    NOT NULL CHECK (estado IN
                             ('Rascunho','Enviado','Aprovado','Recusado','Expirado','Cancelado','Convertido')),
    responsavel          BLOB    NOT NULL,
    aprovado_por         TEXT,
    os_gerada            BLOB,
    criado_em            INTEGER NOT NULL,
    versao               INTEGER NOT NULL DEFAULT 1,
    UNIQUE (empresa, numero)
) STRICT;
CREATE INDEX orcamentos_orcamento_empresa_estado ON orcamentos_orcamento(empresa, estado);
CREATE INDEX orcamentos_orcamento_cliente ON orcamentos_orcamento(cliente);

CREATE TABLE orcamentos_item (
    id                   BLOB PRIMARY KEY,
    orcamento            BLOB    NOT NULL REFERENCES orcamentos_orcamento(id),
    ordem                INTEGER NOT NULL,
    descricao            TEXT    NOT NULL,
    quantidade           INTEGER NOT NULL,
    unidade              TEXT    NOT NULL DEFAULT '',
    preco_unitario       INTEGER NOT NULL,
    desconto_percentual  INTEGER NOT NULL DEFAULT 0,
    total                INTEGER NOT NULL
) STRICT;
CREATE INDEX orcamentos_item_orcamento ON orcamentos_item(orcamento);
";

const MIGRACOES: &[Migracao] = &[Migracao {
    versao: 1,
    nome: "orcamentos_inicial",
    sql: SQL_INICIAL,
    tipo: TipoMigracao::Esquema,
}];

/// O conjunto de migrações do módulo de orçamentos.
#[must_use]
pub fn conjunto() -> ConjuntoMigracoes {
    ConjuntoMigracoes {
        modulo: "orcamentos",
        depende_de: &["nucleo"],
        migracoes: MIGRACOES,
    }
}
