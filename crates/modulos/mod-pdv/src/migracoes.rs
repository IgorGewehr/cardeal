//! O esquema do módulo de PDV — as tabelas `pdv_*`.
//!
//! `docs/modulos/pdv.md` §13, fatia mínima desta primeira versão: o ciclo do cupom
//! (rascunho/`EmAndamento` → `Finalizado`/`Cancelado`), itens e pagamentos, e uma numeração
//! por terminal simplificada (ver nota em `crate::comandos::abrir_cupom`). Ficam de fora
//! desta fatia — porque nenhum comando ainda os grava — os campos de entrega
//! (`entrega_endereco`/`entrega_estado`, submódulo `delivery`, não essencial) e
//! `documento_fiscal` (a emissão de NFC-e é responsabilidade de `mod-fiscal`, que ainda não
//! existe implementado — `docs/10-modulo-fiscal.md` §3). O diário durável do carrinho
//! (`docs/modulos/pdv.md` §11 regra 1, arquivo append-only com CRC32 por terminal) também não
//! é uma tabela SQL — fica para quando existir um terminal de verdade rodando em loop para se
//! beneficiar da durabilidade sub-transação; hoje cada mutação do cupom já é sua própria
//! transação `SAVEPOINT`, então a recuperação após queda de energia já funciona pela
//! durabilidade normal do SQLite, só não na granularidade de "o último caractere digitado".
//! Declara `depende_de: &["nucleo"]`.

use cardeal_storage::{ConjuntoMigracoes, Migracao, TipoMigracao};

const SQL_CUPOM: &str = r"
CREATE TABLE pdv_faixa_numeracao (
    id              BLOB PRIMARY KEY,
    empresa         BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    terminal        BLOB    NOT NULL,
    serie_fiscal    INTEGER NOT NULL,
    numero_inicial  INTEGER NOT NULL,
    numero_final    INTEGER NOT NULL,
    proximo         INTEGER NOT NULL,
    reservada_em    INTEGER NOT NULL,
    UNIQUE (empresa, terminal, serie_fiscal)
) STRICT;

CREATE TABLE pdv_cupom (
    id                 BLOB PRIMARY KEY,
    empresa            BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    sessao_caixa       BLOB    NOT NULL,
    terminal           BLOB    NOT NULL,
    numero_terminal    INTEGER NOT NULL,
    serie_fiscal       INTEGER NOT NULL,
    cliente            BLOB,
    operador           BLOB    NOT NULL,
    tabela_preco       BLOB    NOT NULL,
    local_expedicao    BLOB    NOT NULL,
    abertura           INTEGER NOT NULL,
    subtotal           INTEGER NOT NULL DEFAULT 0,
    desconto           INTEGER NOT NULL DEFAULT 0,
    total              INTEGER NOT NULL DEFAULT 0,
    estado             TEXT    NOT NULL CHECK (estado IN ('EmAndamento','Finalizado','Cancelado')),
    versao             INTEGER NOT NULL DEFAULT 1,
    UNIQUE (empresa, terminal, serie_fiscal, numero_terminal)
) STRICT;
CREATE INDEX pdv_cupom_sessao ON pdv_cupom(sessao_caixa, abertura);
CREATE INDEX pdv_cupom_em_andamento ON pdv_cupom(terminal) WHERE estado = 'EmAndamento';

CREATE TABLE pdv_item_cupom (
    id                   BLOB PRIMARY KEY,
    cupom                BLOB    NOT NULL REFERENCES pdv_cupom(id),
    produto              BLOB    NOT NULL,
    variacao             BLOB,
    quantidade           INTEGER NOT NULL,   -- Quantidade, escala 1e-4
    preco_unitario       INTEGER NOT NULL,   -- Preco, escala 1e-6
    desconto_percentual  INTEGER NOT NULL DEFAULT 0,
    desconto_valor       INTEGER NOT NULL DEFAULT 0,
    total_item           INTEGER NOT NULL,
    cancelado            INTEGER NOT NULL DEFAULT 0 CHECK (cancelado IN (0,1)),
    motivo_cancelamento  TEXT
) STRICT;
CREATE INDEX pdv_item_cupom_cupom ON pdv_item_cupom(cupom);

CREATE TABLE pdv_pagamento_cupom (
    id      BLOB PRIMARY KEY,
    cupom   BLOB    NOT NULL REFERENCES pdv_cupom(id),
    forma   TEXT    NOT NULL CHECK (forma IN ('Dinheiro','Pix','Debito','Credito')),
    valor   INTEGER NOT NULL
) STRICT;
CREATE INDEX pdv_pagamento_cupom_cupom ON pdv_pagamento_cupom(cupom);
";

const MIGRACOES: &[Migracao] = &[Migracao {
    versao: 1,
    nome: "pdv_cupom_e_pagamento",
    sql: SQL_CUPOM,
    tipo: TipoMigracao::Esquema,
}];

/// O conjunto de migrações do módulo de PDV.
#[must_use]
pub fn conjunto() -> ConjuntoMigracoes {
    ConjuntoMigracoes {
        modulo: "pdv",
        depende_de: &["nucleo"],
        migracoes: MIGRACOES,
    }
}
