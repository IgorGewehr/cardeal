//! O esquema do módulo de compras — as tabelas `compras_*`.
//!
//! `docs/modulos/compras.md` §13, fatia mínima desta primeira versão: nota de entrada,
//! itens, regra de casamento aprendida, preferências de importação e o cursor da
//! distribuição `DFe`. Cotação e pedido de compra ficam para depois (ver `src/lib.rs`).
//! Declara `depende_de: &["nucleo"]`.

use cardeal_storage::{ConjuntoMigracoes, Migracao, TipoMigracao};

const SQL_ENTRADA: &str = r"
CREATE TABLE compras_nota_entrada (
    id                    BLOB PRIMARY KEY,
    empresa               BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    fornecedor            BLOB    NOT NULL,
    chave_acesso          TEXT,
    numero                TEXT    NOT NULL,
    serie                 TEXT    NOT NULL,
    data_emissao          INTEGER NOT NULL,
    valor_produtos        INTEGER NOT NULL,
    valor_frete           INTEGER NOT NULL DEFAULT 0,
    valor_seguro          INTEGER NOT NULL DEFAULT 0,
    valor_outras_despesas INTEGER NOT NULL DEFAULT 0,
    valor_total           INTEGER NOT NULL,
    estado                TEXT    NOT NULL CHECK (estado IN ('AConferir','Conferida','Confirmada','Devolvida')),
    versao                INTEGER NOT NULL DEFAULT 1,
    UNIQUE (empresa, chave_acesso)
) STRICT;
CREATE INDEX compras_nota_estado ON compras_nota_entrada(empresa, estado);

CREATE TABLE compras_item_nota_entrada (
    id                     BLOB PRIMARY KEY,
    nota_entrada           BLOB    NOT NULL REFERENCES compras_nota_entrada(id),
    produto_casado         BLOB,
    codigo_fornecedor      TEXT    NOT NULL,
    descricao_fornecedor   TEXT    NOT NULL,
    ncm                    TEXT    NOT NULL,
    quantidade             INTEGER NOT NULL,
    valor_unitario         INTEGER NOT NULL,
    valor_rateio           INTEGER NOT NULL DEFAULT 0,
    estado_casamento       TEXT    NOT NULL CHECK (estado_casamento IN ('NaoCasado','SugestaoForte','Casado'))
) STRICT;
CREATE INDEX compras_item_nota_casamento ON compras_item_nota_entrada(nota_entrada, estado_casamento);

CREATE TABLE compras_regra_casamento (
    empresa           BLOB    NOT NULL,
    fornecedor        BLOB    NOT NULL,
    codigo_fornecedor TEXT    NOT NULL,
    produto           BLOB    NOT NULL,
    aprendido_em      INTEGER NOT NULL,
    PRIMARY KEY (empresa, fornecedor, codigo_fornecedor)
) STRICT, WITHOUT ROWID;

CREATE TABLE compras_preferencias (
    empresa                                    BLOB PRIMARY KEY,
    confirma_automaticamente_quando_tudo_casa  INTEGER NOT NULL DEFAULT 0 CHECK (confirma_automaticamente_quando_tudo_casa IN (0,1)),
    gera_titulo_a_pagar                        INTEGER NOT NULL DEFAULT 1 CHECK (gera_titulo_a_pagar IN (0,1)),
    rateio_por                                 TEXT    NOT NULL DEFAULT 'Valor' CHECK (rateio_por IN ('Valor','Peso')),
    local_padrao                               BLOB
) STRICT, WITHOUT ROWID;

-- O cursor (NSU) da distribuição `DFe` já visto por empresa — a varredura pede sempre desde o
-- último (docs/modulos/compras.md §5, sequência da tarefa agendada).
CREATE TABLE compras_estado_dfe (
    empresa      BLOB PRIMARY KEY,
    ultimo_nsu   INTEGER NOT NULL DEFAULT 0
) STRICT, WITHOUT ROWID;
";

const MIGRACOES: &[Migracao] = &[Migracao {
    versao: 1,
    nome: "compras_entrada_casamento_preferencias",
    sql: SQL_ENTRADA,
    tipo: TipoMigracao::Esquema,
}];

/// O conjunto de migrações do módulo de compras.
#[must_use]
pub fn conjunto() -> ConjuntoMigracoes {
    ConjuntoMigracoes {
        modulo: "compras",
        depende_de: &["nucleo"],
        migracoes: MIGRACOES,
    }
}
