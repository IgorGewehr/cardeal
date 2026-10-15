//! As migrações do módulo fiscal.

use cardeal_storage::{ConjuntoMigracoes, Migracao, TipoMigracao};

/// v1: o certificado (uma linha por empresa — instalar outro substitui; o arquivo e a senha
/// só existem cifrados em `selado`) e a configuração (o ambiente).
const SQL_V1: &str = "
CREATE TABLE fiscal_certificado (
    empresa       BLOB PRIMARY KEY REFERENCES nucleo_empresa(id),
    titular       TEXT    NOT NULL,
    cnpj          TEXT    NOT NULL,
    emissor       TEXT    NOT NULL,
    numero_serie  TEXT    NOT NULL,
    valido_de     INTEGER NOT NULL,
    valido_ate    INTEGER NOT NULL,
    convertido    INTEGER NOT NULL CHECK (convertido IN (0,1)),
    selado        BLOB    NOT NULL,
    instalado_em  INTEGER NOT NULL,
    instalado_por BLOB    NOT NULL
) STRICT;

CREATE TABLE fiscal_configuracao (
    empresa  BLOB PRIMARY KEY REFERENCES nucleo_empresa(id),
    ambiente TEXT NOT NULL CHECK (ambiente IN ('homologacao','producao'))
) STRICT;
";

/// v2: a NFC-e — configuração (emitir nas vendas, série, CSC no cofre), a numeração por série e
/// ambiente, e o documento fiscal (a nota guardada como foi montada, o estado, a chave, o
/// protocolo, o XML, a fila de tentativas).
const SQL_V2: &str = "
ALTER TABLE fiscal_configuracao ADD COLUMN emitir_nfce INTEGER NOT NULL DEFAULT 0
    CHECK (emitir_nfce IN (0,1));
ALTER TABLE fiscal_configuracao ADD COLUMN serie_nfce INTEGER NOT NULL DEFAULT 1;
ALTER TABLE fiscal_configuracao ADD COLUMN csc_id TEXT;
ALTER TABLE fiscal_configuracao ADD COLUMN csc_selado BLOB;

CREATE TABLE fiscal_serie (
    empresa  BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    modelo   TEXT    NOT NULL CHECK (modelo IN ('55','65')),
    serie    INTEGER NOT NULL,
    ambiente TEXT    NOT NULL CHECK (ambiente IN ('homologacao','producao')),
    proximo  INTEGER NOT NULL CHECK (proximo >= 1),
    PRIMARY KEY (empresa, modelo, serie, ambiente)
) STRICT, WITHOUT ROWID;

CREATE TABLE fiscal_documento (
    id                BLOB PRIMARY KEY,
    empresa           BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    modelo            TEXT    NOT NULL CHECK (modelo IN ('55','65')),
    serie             INTEGER NOT NULL,
    numero            INTEGER NOT NULL,
    ambiente          TEXT    NOT NULL CHECK (ambiente IN ('homologacao','producao')),
    origem_modulo     TEXT    NOT NULL,
    origem_id         BLOB    NOT NULL,
    estado            TEXT    NOT NULL CHECK (estado IN
                          ('Pendente','Incerta','Autorizada','Rejeitada','Denegada','Cancelada')),
    total             INTEGER NOT NULL,
    nota              BLOB,
    chave             TEXT,
    protocolo         TEXT,
    codigo            TEXT,
    motivo            TEXT,
    xml               TEXT,
    tentativas        INTEGER NOT NULL DEFAULT 0,
    proxima_tentativa INTEGER NOT NULL DEFAULT 0,
    criado_em         INTEGER NOT NULL,
    atualizado_em     INTEGER NOT NULL,
    autorizado_em     INTEGER,
    UNIQUE (empresa, origem_modulo, origem_id),
    UNIQUE (empresa, modelo, serie, ambiente, numero)
) STRICT;
CREATE INDEX fiscal_documento_fila ON fiscal_documento(empresa, proxima_tentativa)
    WHERE estado IN ('Pendente','Incerta');
CREATE INDEX fiscal_documento_criado ON fiscal_documento(empresa, criado_em);
";

/// v3: cancelamento e inutilização como **pedidos** que a fila leva à SEFAZ (`pedido`,
/// `justificativa`, `protocolo_evento`, `evento_em`) e o estado `Inutilizada`. O SQLite não
/// muda um `CHECK`: a tabela é recriada com os dados.
const SQL_V3: &str = "
CREATE TABLE fiscal_documento_v3 (
    id                BLOB PRIMARY KEY,
    empresa           BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    modelo            TEXT    NOT NULL CHECK (modelo IN ('55','65')),
    serie             INTEGER NOT NULL,
    numero            INTEGER NOT NULL,
    ambiente          TEXT    NOT NULL CHECK (ambiente IN ('homologacao','producao')),
    origem_modulo     TEXT    NOT NULL,
    origem_id         BLOB    NOT NULL,
    estado            TEXT    NOT NULL CHECK (estado IN
                          ('Pendente','Incerta','Autorizada','Rejeitada','Denegada','Cancelada',
                           'Inutilizada')),
    total             INTEGER NOT NULL,
    nota              BLOB,
    chave             TEXT,
    protocolo         TEXT,
    codigo            TEXT,
    motivo            TEXT,
    xml               TEXT,
    tentativas        INTEGER NOT NULL DEFAULT 0,
    proxima_tentativa INTEGER NOT NULL DEFAULT 0,
    criado_em         INTEGER NOT NULL,
    atualizado_em     INTEGER NOT NULL,
    autorizado_em     INTEGER,
    pedido            TEXT    CHECK (pedido IN ('cancelar','inutilizar')),
    justificativa     TEXT,
    protocolo_evento  TEXT,
    evento_em         INTEGER,
    UNIQUE (empresa, origem_modulo, origem_id),
    UNIQUE (empresa, modelo, serie, ambiente, numero)
) STRICT;
INSERT INTO fiscal_documento_v3
    (id, empresa, modelo, serie, numero, ambiente, origem_modulo, origem_id, estado, total, nota,
     chave, protocolo, codigo, motivo, xml, tentativas, proxima_tentativa, criado_em,
     atualizado_em, autorizado_em)
SELECT id, empresa, modelo, serie, numero, ambiente, origem_modulo, origem_id, estado, total, nota,
       chave, protocolo, codigo, motivo, xml, tentativas, proxima_tentativa, criado_em,
       atualizado_em, autorizado_em
FROM fiscal_documento;
DROP TABLE fiscal_documento;
ALTER TABLE fiscal_documento_v3 RENAME TO fiscal_documento;
CREATE INDEX fiscal_documento_fila ON fiscal_documento(empresa, proxima_tentativa)
    WHERE estado IN ('Pendente','Incerta') OR pedido IS NOT NULL;
CREATE INDEX fiscal_documento_criado ON fiscal_documento(empresa, criado_em);
";

const MIGRACOES: &[Migracao] = &[
    Migracao {
        versao: 1,
        nome: "fiscal_certificado_e_configuracao",
        sql: SQL_V1,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 2,
        nome: "fiscal_nfce_documento_e_serie",
        sql: SQL_V2,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 3,
        nome: "fiscal_cancelamento_e_inutilizacao",
        sql: SQL_V3,
        tipo: TipoMigracao::Esquema,
    },
];

/// O conjunto de migrações do módulo fiscal.
#[must_use]
pub fn conjunto() -> ConjuntoMigracoes {
    ConjuntoMigracoes {
        modulo: "fiscal",
        depende_de: &["nucleo"],
        migracoes: MIGRACOES,
    }
}
