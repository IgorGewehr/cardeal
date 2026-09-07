//! O esquema do módulo de estoque — as tabelas `estoque_*`.
//!
//! `docs/modulos/estoque.md` §13. Declara `depende_de: &["nucleo"]`. Fatia mínima desta
//! primeira versão: grupo de produto (sem perfil tributário ainda), unidade, produto, local,
//! saldo e movimento — grade/lote/validade/inventário/transferência/curva ABC ficam para
//! depois (ver `src/lib.rs`).

use cardeal_storage::{ConjuntoMigracoes, Migracao, TipoMigracao};

const SQL_CATALOGO_E_SALDO: &str = r"
CREATE TABLE estoque_grupo_produto (
    id       BLOB PRIMARY KEY,
    empresa  BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    codigo   TEXT    NOT NULL,
    nome     TEXT    NOT NULL,
    pai      BLOB REFERENCES estoque_grupo_produto(id),
    versao   INTEGER NOT NULL DEFAULT 1,
    UNIQUE (empresa, codigo)
) STRICT;

CREATE TABLE estoque_unidade (
    id           BLOB PRIMARY KEY,
    empresa      BLOB NOT NULL,
    sigla        TEXT NOT NULL,
    nome         TEXT NOT NULL,
    fracionavel  INTEGER NOT NULL DEFAULT 0 CHECK (fracionavel IN (0,1)),
    UNIQUE (empresa, sigla)
) STRICT;

CREATE TABLE estoque_produto (
    id                  BLOB PRIMARY KEY,
    empresa             BLOB    NOT NULL,
    grupo_produto       BLOB    NOT NULL REFERENCES estoque_grupo_produto(id),
    nome                TEXT    NOT NULL,
    ncm                 TEXT    NOT NULL,
    cest                TEXT,
    controla_grade      INTEGER NOT NULL DEFAULT 0 CHECK (controla_grade IN (0,1)),
    controla_lote       INTEGER NOT NULL DEFAULT 0 CHECK (controla_lote IN (0,1)),
    controla_validade   INTEGER NOT NULL DEFAULT 0 CHECK (controla_validade IN (0,1)),
    unidade_padrao      BLOB    NOT NULL REFERENCES estoque_unidade(id),
    ponto_pedido        INTEGER,
    estoque_minimo      INTEGER,
    estoque_maximo      INTEGER,
    ativo               INTEGER NOT NULL DEFAULT 1 CHECK (ativo IN (0,1)),
    versao              INTEGER NOT NULL DEFAULT 1,
    CHECK (controla_validade = 0 OR controla_lote = 1)
) STRICT;
CREATE INDEX estoque_produto_grupo ON estoque_produto(empresa, grupo_produto) WHERE ativo = 1;

CREATE TABLE estoque_local (
    id       BLOB PRIMARY KEY,
    empresa  BLOB    NOT NULL,
    nome     TEXT    NOT NULL,
    tipo     TEXT    NOT NULL CHECK (tipo IN ('Loja','Deposito','Filial','Tanque','EmTransito')),
    ativo    INTEGER NOT NULL DEFAULT 1 CHECK (ativo IN (0,1))
) STRICT;

-- variacao usa sentinela de blob vazio (X'') para produto sem grade — mantém a UNIQUE
-- estável porque SQLite trata NULL como distinto em toda comparação (mesma técnica do
-- doc estoque.md §13).
CREATE TABLE estoque_saldo_local (
    id                     BLOB PRIMARY KEY,
    empresa                BLOB    NOT NULL,
    produto                BLOB    NOT NULL REFERENCES estoque_produto(id),
    variacao               BLOB    NOT NULL DEFAULT (X''),
    local                  BLOB    NOT NULL REFERENCES estoque_local(id),
    quantidade_disponivel  INTEGER NOT NULL DEFAULT 0,
    quantidade_reservada   INTEGER NOT NULL DEFAULT 0,
    custo_medio            INTEGER NOT NULL DEFAULT 0,
    atualizado_em          INTEGER NOT NULL,
    versao                 INTEGER NOT NULL DEFAULT 1,
    UNIQUE (produto, variacao, local)
) STRICT;
CREATE INDEX estoque_saldo_ponto_pedido ON estoque_saldo_local(empresa, produto, local);

CREATE TABLE estoque_movimento (
    id               BLOB PRIMARY KEY,
    empresa          BLOB    NOT NULL,
    produto          BLOB    NOT NULL REFERENCES estoque_produto(id),
    variacao         BLOB    NOT NULL DEFAULT (X''),
    local            BLOB    NOT NULL REFERENCES estoque_local(id),
    tipo             TEXT    NOT NULL CHECK (tipo IN ('Entrada','Saida','TransferenciaSaida','TransferenciaEntrada','AjustePositivo','AjusteNegativo','Reserva','LiberacaoReserva','Producao','Perda')),
    quantidade       INTEGER NOT NULL,
    custo_unitario   INTEGER,
    lote             BLOB,
    origem_modulo    TEXT    NOT NULL,
    origem_id        BLOB,
    lancamento       BLOB,
    criado_em        INTEGER NOT NULL,
    criado_por       BLOB    NOT NULL
) STRICT;
CREATE INDEX estoque_movimento_produto ON estoque_movimento(produto, criado_em DESC);
CREATE INDEX estoque_movimento_origem ON estoque_movimento(origem_modulo, origem_id);
";

const SQL_CODIGO_BARRAS: &str = r"
ALTER TABLE estoque_produto ADD COLUMN codigo_barras TEXT;
CREATE UNIQUE INDEX estoque_produto_codigo_barras ON estoque_produto(empresa, codigo_barras)
    WHERE codigo_barras IS NOT NULL;
";

const MIGRACOES: &[Migracao] = &[
    Migracao {
        versao: 1,
        nome: "estoque_catalogo_local_saldo_movimento",
        sql: SQL_CATALOGO_E_SALDO,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 2,
        nome: "estoque_produto_codigo_barras",
        sql: SQL_CODIGO_BARRAS,
        tipo: TipoMigracao::Esquema,
    },
];

/// O conjunto de migrações do módulo de estoque.
#[must_use]
pub fn conjunto() -> ConjuntoMigracoes {
    ConjuntoMigracoes {
        modulo: "estoque",
        depende_de: &["nucleo"],
        migracoes: MIGRACOES,
    }
}
