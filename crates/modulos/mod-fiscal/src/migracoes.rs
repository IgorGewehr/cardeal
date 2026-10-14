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

const MIGRACOES: &[Migracao] = &[Migracao {
    versao: 1,
    nome: "fiscal_certificado_e_configuracao",
    sql: SQL_V1,
    tipo: TipoMigracao::Esquema,
}];

/// O conjunto de migrações do módulo fiscal.
#[must_use]
pub fn conjunto() -> ConjuntoMigracoes {
    ConjuntoMigracoes {
        modulo: "fiscal",
        depende_de: &["nucleo"],
        migracoes: MIGRACOES,
    }
}
