//! O esquema do Razão — as tabelas `razao_*` de `docs/06-modelo-de-dados.md` §3.
//!
//! Entregue como [`ConjuntoMigracoes`](cardeal_storage::ConjuntoMigracoes) que declara
//! `depende_de: &["nucleo"]` — a tabela `nucleo_empresa` já existe quando estas rodam.

use cardeal_storage::{ConjuntoMigracoes, Migracao, TipoMigracao};

const SQL_INICIAL: &str = r"
CREATE TABLE razao_conta (
    id            BLOB PRIMARY KEY,
    empresa       BLOB NOT NULL REFERENCES nucleo_empresa(id),
    codigo        TEXT NOT NULL,
    nome          TEXT NOT NULL,
    natureza      TEXT NOT NULL CHECK (natureza IN ('Ativo','Passivo','PatrimonioLiquido','Receita','Despesa')),
    tipo          TEXT NOT NULL CHECK (tipo IN ('Sintetica','Analitica')),
    pai           BLOB REFERENCES razao_conta(id),
    nivel         INTEGER NOT NULL,
    grupo_fluxo   TEXT,
    modulo_origem TEXT,
    papel_padrao  TEXT,
    ativa         INTEGER NOT NULL DEFAULT 1 CHECK (ativa IN (0,1)),
    versao        INTEGER NOT NULL DEFAULT 1,
    UNIQUE (empresa, codigo)
) STRICT;
CREATE INDEX razao_conta_papel ON razao_conta(empresa, papel_padrao) WHERE papel_padrao IS NOT NULL;

CREATE TABLE razao_lancamento (
    id            BLOB PRIMARY KEY,
    empresa       BLOB    NOT NULL,
    numero        INTEGER NOT NULL,
    competencia   INTEGER NOT NULL,
    vencimento    INTEGER,
    liquidacao    INTEGER,
    estado        TEXT    NOT NULL CHECK (estado IN ('Previsto','Confirmado','Realizado','Estornado')),
    origem_modulo TEXT    NOT NULL,
    origem_tipo   TEXT    NOT NULL,
    origem_id     BLOB,
    historico     TEXT    NOT NULL,
    estorna       BLOB REFERENCES razao_lancamento(id),
    estornado_por BLOB REFERENCES razao_lancamento(id),
    criado_em     INTEGER NOT NULL,
    criado_por    BLOB    NOT NULL,
    dispositivo   BLOB    NOT NULL,
    UNIQUE (empresa, numero)
) STRICT;

CREATE TABLE razao_partida (
    lancamento       BLOB    NOT NULL REFERENCES razao_lancamento(id),
    ordem            INTEGER NOT NULL,
    conta            BLOB    NOT NULL REFERENCES razao_conta(id),
    valor            INTEGER NOT NULL,
    contraparte_tipo TEXT,
    contraparte_id   BLOB,
    centro_custo     BLOB,
    projeto          BLOB,
    documento        TEXT,
    quantidade       INTEGER,
    complemento      TEXT,
    PRIMARY KEY (lancamento, ordem)
) STRICT, WITHOUT ROWID;

CREATE TABLE razao_saldo_mensal (
    empresa  BLOB    NOT NULL,
    conta    BLOB    NOT NULL,
    ano_mes  INTEGER NOT NULL,
    saldo    INTEGER NOT NULL,
    debitos  INTEGER NOT NULL,
    creditos INTEGER NOT NULL,
    PRIMARY KEY (empresa, conta, ano_mes)
) STRICT, WITHOUT ROWID;

CREATE TABLE razao_fechamento (
    empresa     BLOB    NOT NULL,
    ate         INTEGER NOT NULL,
    fechado_em  INTEGER NOT NULL,
    fechado_por BLOB    NOT NULL,
    hash_saldos BLOB    NOT NULL,
    PRIMARY KEY (empresa, ate)
) STRICT, WITHOUT ROWID;
";

const SQL_INDICES: &str = r"
CREATE INDEX razao_lanc_competencia    ON razao_lancamento(empresa, competencia, estado);
CREATE INDEX razao_lanc_fluxo          ON razao_lancamento(empresa, liquidacao, vencimento, estado);
CREATE INDEX razao_lanc_origem         ON razao_lancamento(origem_modulo, origem_tipo, origem_id);
CREATE INDEX razao_partida_conta       ON razao_partida(conta, lancamento);
CREATE INDEX razao_partida_contraparte ON razao_partida(contraparte_tipo, contraparte_id, lancamento);
CREATE INDEX razao_partida_cc          ON razao_partida(centro_custo, lancamento) WHERE centro_custo IS NOT NULL;
";

const MIGRACOES: &[Migracao] = &[
    Migracao {
        versao: 1,
        nome: "razao_inicial",
        sql: SQL_INICIAL,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 2,
        nome: "razao_indices",
        sql: SQL_INDICES,
        tipo: TipoMigracao::Indice,
    },
];

/// O conjunto de migrações do Razão.
#[must_use]
pub fn conjunto() -> ConjuntoMigracoes {
    ConjuntoMigracoes {
        modulo: "razao",
        depende_de: &["nucleo"],
        migracoes: MIGRACOES,
    }
}
