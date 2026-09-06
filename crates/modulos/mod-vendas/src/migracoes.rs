//! O esquema do módulo de vendas — as tabelas `vendas_*`.
//!
//! `docs/modulos/vendas.md` §13, fatia mínima desta primeira versão: tabela de
//! preço/regra de preço e o ciclo de pedido (rascunho → confirmado → faturado/cancelado).
//! Orçamento, devolução, comissão e contrato recorrente ficam para depois (ver
//! `src/lib.rs`) — os submódulos essenciais são só `pedido` e `tabela_preco`
//! (`src/manifesto.rs`), então nenhuma tabela deles é essencial agora. Declara
//! `depende_de: &["nucleo"]`.

use cardeal_storage::{ConjuntoMigracoes, Migracao, TipoMigracao};

const SQL_TABELA_E_PEDIDO: &str = r"
CREATE TABLE vendas_tabela_preco (
    id           BLOB PRIMARY KEY,
    empresa      BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    nome         TEXT    NOT NULL,
    tipo         TEXT    NOT NULL CHECK (tipo IN ('Venda','Atacado','Promocional')),
    vigente_de   INTEGER NOT NULL,
    vigente_ate  INTEGER,
    ativa        INTEGER NOT NULL DEFAULT 1 CHECK (ativa IN (0,1)),
    versao       INTEGER NOT NULL DEFAULT 1
) STRICT;
CREATE INDEX vendas_tabela_preco_ativa ON vendas_tabela_preco(empresa, ativa);

CREATE TABLE vendas_regra_preco (
    id                  BLOB PRIMARY KEY,
    empresa             BLOB    NOT NULL,
    tabela_preco        BLOB    NOT NULL REFERENCES vendas_tabela_preco(id),
    produto             BLOB,
    grupo_produto       BLOB,
    quantidade_minima   INTEGER,
    preco               INTEGER NOT NULL,
    periodo_de          INTEGER,
    periodo_ate         INTEGER,
    CHECK ((produto IS NULL) <> (grupo_produto IS NULL))
) STRICT;
CREATE INDEX vendas_regra_preco_busca ON vendas_regra_preco(tabela_preco, produto);
CREATE INDEX vendas_regra_preco_grupo ON vendas_regra_preco(tabela_preco, grupo_produto);

CREATE TABLE vendas_pedido (
    id                  BLOB PRIMARY KEY,
    empresa             BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    orcamento_origem    BLOB,
    cliente             BLOB    NOT NULL,
    vendedor            BLOB    NOT NULL,
    data                INTEGER NOT NULL,
    condicao_pagamento  BLOB    NOT NULL,
    tabela_preco        BLOB    NOT NULL REFERENCES vendas_tabela_preco(id),
    centro_custo        BLOB,
    local_expedicao     BLOB    NOT NULL,
    desconto_total      INTEGER NOT NULL DEFAULT 0,
    total               INTEGER NOT NULL DEFAULT 0,
    estado              TEXT    NOT NULL CHECK (estado IN ('Rascunho','Confirmado','Faturado','EmEntrega','Concluido','Cancelado')),
    versao              INTEGER NOT NULL DEFAULT 1,
    criado_por          BLOB    NOT NULL
) STRICT;
CREATE INDEX vendas_pedido_estado ON vendas_pedido(empresa, estado, data);
CREATE INDEX vendas_pedido_cliente ON vendas_pedido(cliente, data);

CREATE TABLE vendas_item_pedido (
    id                   BLOB PRIMARY KEY,
    pedido               BLOB    NOT NULL REFERENCES vendas_pedido(id),
    produto              BLOB    NOT NULL,
    variacao             BLOB,
    quantidade           INTEGER NOT NULL,
    preco_unitario       INTEGER NOT NULL,
    desconto_percentual  INTEGER NOT NULL DEFAULT 0,
    desconto_valor       INTEGER NOT NULL DEFAULT 0,
    total_item           INTEGER NOT NULL,
    reserva              BLOB
) STRICT;
CREATE INDEX vendas_item_pedido_pedido ON vendas_item_pedido(pedido);
";

const MIGRACOES: &[Migracao] = &[Migracao {
    versao: 1,
    nome: "vendas_tabela_preco_e_pedido",
    sql: SQL_TABELA_E_PEDIDO,
    tipo: TipoMigracao::Esquema,
}];

/// O conjunto de migrações do módulo de vendas.
#[must_use]
pub fn conjunto() -> ConjuntoMigracoes {
    ConjuntoMigracoes {
        modulo: "vendas",
        depende_de: &["nucleo"],
        migracoes: MIGRACOES,
    }
}
