//! O esquema do núcleo — as tabelas `nucleo_*` de `docs/06-modelo-de-dados.md` §2.
//!
//! É a primeira migração que roda, sempre. Os módulos de negócio trazem seus próprios
//! [`ConjuntoMigracoes`](crate::ConjuntoMigracoes) declarando `depende_de: &["nucleo"]`.

use crate::migracao::{ConjuntoMigracoes, Migracao, TipoMigracao};

const SQL_INICIAL: &str = r"
CREATE TABLE nucleo_empresa (
    id            BLOB PRIMARY KEY,
    razao_social  TEXT    NOT NULL,
    nome_fantasia TEXT    NOT NULL,
    cnpj          TEXT    NOT NULL,
    ie            TEXT,
    im            TEXT,
    regime        TEXT    NOT NULL CHECK (regime IN ('MEI','SimplesNacional','LucroPresumido','LucroReal')),
    matriz        BLOB    REFERENCES nucleo_empresa(id),
    endereco      TEXT    NOT NULL,
    perfil        TEXT    NOT NULL,
    fuso          INTEGER NOT NULL DEFAULT -180,
    ativa         INTEGER NOT NULL DEFAULT 1 CHECK (ativa IN (0,1)),
    versao        INTEGER NOT NULL DEFAULT 1,
    criado_em     INTEGER NOT NULL
) STRICT;

CREATE TABLE nucleo_usuario (
    id               BLOB PRIMARY KEY,
    login            TEXT    NOT NULL UNIQUE,
    nome             TEXT    NOT NULL,
    email            TEXT,
    senha_hash       TEXT    NOT NULL,
    senha_trocada_em INTEGER NOT NULL,
    exige_troca      INTEGER NOT NULL DEFAULT 0 CHECK (exige_troca IN (0,1)),
    mfa_segredo      TEXT,
    ativo            INTEGER NOT NULL DEFAULT 1 CHECK (ativo IN (0,1)),
    bloqueado_ate    INTEGER,
    tentativas       INTEGER NOT NULL DEFAULT 0,
    versao           INTEGER NOT NULL DEFAULT 1,
    criado_em        INTEGER NOT NULL,
    criado_por       BLOB
) STRICT;

CREATE TABLE nucleo_papel (
    id        BLOB PRIMARY KEY,
    empresa   BLOB REFERENCES nucleo_empresa(id),
    nome      TEXT    NOT NULL,
    descricao TEXT,
    sistema   INTEGER NOT NULL DEFAULT 0 CHECK (sistema IN (0,1)),
    versao    INTEGER NOT NULL DEFAULT 1
) STRICT;

CREATE TABLE nucleo_papel_permissao (
    papel     BLOB NOT NULL REFERENCES nucleo_papel(id),
    permissao TEXT NOT NULL,
    PRIMARY KEY (papel, permissao)
) STRICT, WITHOUT ROWID;

CREATE TABLE nucleo_usuario_papel (
    usuario BLOB NOT NULL REFERENCES nucleo_usuario(id),
    papel   BLOB NOT NULL REFERENCES nucleo_papel(id),
    empresa BLOB REFERENCES nucleo_empresa(id),
    PRIMARY KEY (usuario, papel, empresa)
) STRICT, WITHOUT ROWID;

CREATE TABLE nucleo_dispositivo (
    id            BLOB PRIMARY KEY,
    apelido       TEXT    NOT NULL,
    tipo          TEXT    NOT NULL CHECK (tipo IN ('Servidor','Pdv','Retaguarda','Movel')),
    chave_publica BLOB    NOT NULL,
    certificado   BLOB,
    empresa       BLOB    REFERENCES nucleo_empresa(id),
    ativo         INTEGER NOT NULL DEFAULT 1 CHECK (ativo IN (0,1)),
    visto_em      INTEGER,
    versao_app    TEXT,
    criado_em     INTEGER NOT NULL
) STRICT;

CREATE TABLE nucleo_sessao (
    id           BLOB PRIMARY KEY,
    usuario      BLOB NOT NULL REFERENCES nucleo_usuario(id),
    dispositivo  BLOB NOT NULL REFERENCES nucleo_dispositivo(id),
    empresa      BLOB NOT NULL REFERENCES nucleo_empresa(id),
    token_hash   BLOB NOT NULL,
    expira_em    INTEGER NOT NULL,
    criado_em    INTEGER NOT NULL,
    encerrado_em INTEGER
) STRICT;

CREATE TABLE nucleo_modulo_ativo (
    empresa     BLOB NOT NULL REFERENCES nucleo_empresa(id),
    modulo      TEXT NOT NULL,
    submodulo   TEXT NOT NULL DEFAULT '',
    ativo       INTEGER NOT NULL CHECK (ativo IN (0,1)),
    ativado_em  INTEGER NOT NULL,
    ativado_por BLOB,
    PRIMARY KEY (empresa, modulo, submodulo)
) STRICT, WITHOUT ROWID;

CREATE TABLE nucleo_configuracao (
    empresa BLOB NOT NULL,
    chave   TEXT NOT NULL,
    valor   TEXT NOT NULL,
    PRIMARY KEY (empresa, chave)
) STRICT, WITHOUT ROWID;

CREATE TABLE nucleo_migracao (
    modulo      TEXT    NOT NULL,
    versao      INTEGER NOT NULL,
    nome        TEXT    NOT NULL,
    hash        BLOB    NOT NULL,
    aplicada_em INTEGER NOT NULL,
    duracao_ms  INTEGER NOT NULL,
    PRIMARY KEY (modulo, versao)
) STRICT, WITHOUT ROWID;

CREATE TABLE nucleo_outbox (
    seq         INTEGER PRIMARY KEY AUTOINCREMENT,
    empresa     BLOB    NOT NULL,
    tipo        TEXT    NOT NULL,
    agregado    BLOB,
    carga       BLOB    NOT NULL,
    criado_em   INTEGER NOT NULL,
    entregue_em INTEGER,
    tentativas  INTEGER NOT NULL DEFAULT 0
) STRICT;
CREATE INDEX nucleo_outbox_pendente ON nucleo_outbox(seq) WHERE entregue_em IS NULL;

CREATE TABLE nucleo_idempotencia (
    chave       BLOB PRIMARY KEY,
    dispositivo BLOB NOT NULL,
    comando     TEXT NOT NULL,
    resposta    BLOB NOT NULL,
    criado_em   INTEGER NOT NULL
) STRICT;

CREATE TABLE nucleo_trava (
    recurso   TEXT PRIMARY KEY,
    dono      BLOB NOT NULL,
    expira_em INTEGER NOT NULL,
    motivo    TEXT
) STRICT, WITHOUT ROWID;

CREATE TABLE nucleo_auditoria (
    seq           INTEGER PRIMARY KEY AUTOINCREMENT,
    empresa       BLOB    NOT NULL,
    usuario       BLOB    NOT NULL,
    dispositivo   BLOB    NOT NULL,
    quando        INTEGER NOT NULL,
    acao          TEXT    NOT NULL,
    entidade      TEXT    NOT NULL,
    entidade_id   BLOB,
    antes         BLOB,
    depois        BLOB,
    hash_anterior BLOB NOT NULL,
    hash          BLOB NOT NULL
) STRICT;
CREATE INDEX nucleo_auditoria_entidade ON nucleo_auditoria(entidade, entidade_id, seq DESC);

CREATE TABLE nucleo_sequencia (
    empresa BLOB NOT NULL,
    nome    TEXT NOT NULL,
    proximo INTEGER NOT NULL,
    PRIMARY KEY (empresa, nome)
) STRICT, WITHOUT ROWID;
";

/// `nucleo_papel_limite` — os limites quantitativos de papel (`docs/08 §3.4`).
///
/// Separado de `nucleo_papel_permissao` porque limite é "quanto", não "se": um teto
/// tipado, não um booleano. Cada linha é uma dimensão (`Dinheiro`, `Percentual`,
/// `Contagem`, `Dias`) ou `Ilimitado`; `valor` guarda o teto na unidade interna da
/// dimensão (centavos, 1e-6 de ponto percentual, contagem, dias) e é ignorado para
/// `Ilimitado`.
const SQL_PAPEL_LIMITE: &str = r"
CREATE TABLE nucleo_papel_limite (
    papel BLOB NOT NULL REFERENCES nucleo_papel(id),
    chave TEXT NOT NULL,
    tipo  TEXT NOT NULL CHECK (tipo IN ('Dinheiro','Percentual','Contagem','Dias','Ilimitado')),
    valor INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (papel, chave)
) STRICT, WITHOUT ROWID;
";

const MIGRACOES: &[Migracao] = &[
    Migracao {
        versao: 1,
        nome: "nucleo_inicial",
        sql: SQL_INICIAL,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 2,
        nome: "nucleo_papel_limite",
        sql: SQL_PAPEL_LIMITE,
        tipo: TipoMigracao::Esquema,
    },
];

/// O conjunto de migrações do núcleo.
#[must_use]
pub fn conjunto() -> ConjuntoMigracoes {
    ConjuntoMigracoes {
        modulo: "nucleo",
        depende_de: &[],
        migracoes: MIGRACOES,
    }
}
