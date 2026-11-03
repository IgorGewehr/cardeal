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

// Migração v2 (2026-10-13): comissão do vendedor. O percentual de cada item é congelado no
// faturamento (a comissão por estrutura do estoque, naquele momento); a apuração mensal grava
// uma comissão por pedido. `competencia` é a data do lançamento da apuração (sempre dentro do
// mês); `lancamento` é o do pagamento, como no agregado `Comissao`.
const SQL_COMISSAO: &str = r"
ALTER TABLE vendas_item_pedido ADD COLUMN comissao_percentual INTEGER;

CREATE TABLE vendas_comissao (
    id                    BLOB PRIMARY KEY,
    empresa               BLOB    NOT NULL,
    vendedor              BLOB    NOT NULL,
    pedido                BLOB    NOT NULL UNIQUE REFERENCES vendas_pedido(id),
    base_calculo          INTEGER NOT NULL,
    percentual            INTEGER NOT NULL,
    valor                 INTEGER NOT NULL,
    competencia           INTEGER NOT NULL,
    estado                TEXT    NOT NULL CHECK (estado IN ('Apurada','Paga','Cancelada')),
    lancamento_apuracao   BLOB    NOT NULL,
    lancamento            BLOB,
    criado_em             INTEGER NOT NULL
) STRICT;
CREATE INDEX vendas_comissao_competencia ON vendas_comissao(empresa, competencia, vendedor);
";

/// v3: a devolução de venda, item a item, e o estorno da comissão da parte devolvida de um
/// pedido já apurado.
const SQL_DEVOLUCAO: &str = "
CREATE TABLE vendas_devolucao (
    id                 BLOB PRIMARY KEY,
    empresa            BLOB    NOT NULL,
    pedido             BLOB    NOT NULL REFERENCES vendas_pedido(id),
    cliente            BLOB    NOT NULL,
    data               INTEGER NOT NULL,
    motivo             TEXT    NOT NULL,
    tipo               TEXT    NOT NULL CHECK (tipo IN ('Total','Parcial')),
    valor_total        INTEGER NOT NULL,
    estado             TEXT    NOT NULL
                       CHECK (estado IN ('Solicitada','Aprovada','Concluida','Cancelada')),
    estorno_lancamento BLOB,
    versao             INTEGER NOT NULL DEFAULT 1,
    criada_em          INTEGER NOT NULL,
    criada_por         BLOB    NOT NULL
) STRICT;
CREATE INDEX vendas_devolucao_pedido ON vendas_devolucao(pedido);
CREATE INDEX vendas_devolucao_estado ON vendas_devolucao(empresa, estado);
CREATE TABLE vendas_devolucao_item (
    devolucao       BLOB    NOT NULL REFERENCES vendas_devolucao(id),
    item_pedido     BLOB    NOT NULL REFERENCES vendas_item_pedido(id),
    produto         BLOB    NOT NULL,
    variacao        BLOB,
    quantidade      INTEGER NOT NULL CHECK (quantidade > 0),
    valor_estornado INTEGER NOT NULL,
    PRIMARY KEY (devolucao, item_pedido)
) STRICT;
CREATE TABLE vendas_comissao_ajuste (
    id                  BLOB PRIMARY KEY,
    empresa             BLOB    NOT NULL,
    vendedor            BLOB    NOT NULL,
    pedido              BLOB    NOT NULL REFERENCES vendas_pedido(id),
    devolucao           BLOB    NOT NULL UNIQUE REFERENCES vendas_devolucao(id),
    base                INTEGER NOT NULL,
    valor               INTEGER NOT NULL CHECK (valor > 0),
    data                INTEGER NOT NULL,
    estado              TEXT    NOT NULL CHECK (estado IN ('Apurada','Paga')),
    lancamento_apuracao BLOB    NOT NULL,
    lancamento          BLOB,
    criado_em           INTEGER NOT NULL
) STRICT;
CREATE INDEX vendas_comissao_ajuste_vendedor ON vendas_comissao_ajuste(empresa, vendedor, estado);
";

/// v4: a venda comissionável de outro módulo (a do PDV) — a comissão e o estorno dela deixam de
/// apontar só para `vendas_pedido` (as tabelas são recriadas sem essa chave estrangeira, com a
/// origem).
const SQL_COMISSAO_PDV: &str = "
CREATE TABLE vendas_comissao_nova (
    id                    BLOB PRIMARY KEY,
    empresa               BLOB    NOT NULL,
    vendedor              BLOB    NOT NULL,
    pedido                BLOB    NOT NULL UNIQUE,
    origem                TEXT    NOT NULL DEFAULT 'pedido' CHECK (origem IN ('pedido','pdv')),
    base_calculo          INTEGER NOT NULL,
    percentual            INTEGER NOT NULL,
    valor                 INTEGER NOT NULL,
    competencia           INTEGER NOT NULL,
    estado                TEXT    NOT NULL CHECK (estado IN ('Apurada','Paga','Cancelada')),
    lancamento_apuracao   BLOB    NOT NULL,
    lancamento            BLOB,
    criado_em             INTEGER NOT NULL
) STRICT;
INSERT INTO vendas_comissao_nova
    (id, empresa, vendedor, pedido, base_calculo, percentual, valor, competencia, estado,
     lancamento_apuracao, lancamento, criado_em)
  SELECT id, empresa, vendedor, pedido, base_calculo, percentual, valor, competencia, estado,
         lancamento_apuracao, lancamento, criado_em
  FROM vendas_comissao;
DROP TABLE vendas_comissao;
ALTER TABLE vendas_comissao_nova RENAME TO vendas_comissao;
CREATE INDEX vendas_comissao_competencia ON vendas_comissao(empresa, competencia, vendedor);

CREATE TABLE vendas_comissao_ajuste_nova (
    id                  BLOB PRIMARY KEY,
    empresa             BLOB    NOT NULL,
    vendedor            BLOB    NOT NULL,
    pedido              BLOB    NOT NULL,
    devolucao           BLOB    NOT NULL UNIQUE,
    base                INTEGER NOT NULL,
    valor               INTEGER NOT NULL CHECK (valor > 0),
    data                INTEGER NOT NULL,
    estado              TEXT    NOT NULL CHECK (estado IN ('Apurada','Paga')),
    lancamento_apuracao BLOB    NOT NULL,
    lancamento          BLOB,
    criado_em           INTEGER NOT NULL
) STRICT;
INSERT INTO vendas_comissao_ajuste_nova SELECT * FROM vendas_comissao_ajuste;
DROP TABLE vendas_comissao_ajuste;
ALTER TABLE vendas_comissao_ajuste_nova RENAME TO vendas_comissao_ajuste;
CREATE INDEX vendas_comissao_ajuste_vendedor ON vendas_comissao_ajuste(empresa, vendedor, estado);

CREATE TABLE vendas_venda_comissionavel (
    id         BLOB PRIMARY KEY,
    empresa    BLOB    NOT NULL,
    origem     TEXT    NOT NULL,
    origem_id  BLOB    NOT NULL,
    vendedor   BLOB    NOT NULL,
    data       INTEGER NOT NULL,
    estado     TEXT    NOT NULL CHECK (estado IN ('Valida','Estornada')),
    criado_em  INTEGER NOT NULL,
    UNIQUE (origem, origem_id)
) STRICT;
CREATE INDEX vendas_venda_comissionavel_data ON vendas_venda_comissionavel(empresa, data);
CREATE TABLE vendas_venda_comissionavel_item (
    venda               BLOB    NOT NULL REFERENCES vendas_venda_comissionavel(id),
    produto             BLOB    NOT NULL,
    total               INTEGER NOT NULL,
    comissao_percentual INTEGER
) STRICT;
CREATE INDEX vendas_venda_comissionavel_item_venda ON vendas_venda_comissionavel_item(venda);
";

const MIGRACOES: &[Migracao] = &[
    Migracao {
        versao: 1,
        nome: "vendas_tabela_preco_e_pedido",
        sql: SQL_TABELA_E_PEDIDO,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 2,
        nome: "vendas_comissao",
        sql: SQL_COMISSAO,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 3,
        nome: "vendas_devolucao",
        sql: SQL_DEVOLUCAO,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 4,
        nome: "vendas_comissao_do_pdv",
        sql: SQL_COMISSAO_PDV,
        tipo: TipoMigracao::Esquema,
    },
];

/// O conjunto de migrações do módulo de vendas.
#[must_use]
pub fn conjunto() -> ConjuntoMigracoes {
    ConjuntoMigracoes {
        modulo: "vendas",
        depende_de: &["nucleo"],
        migracoes: MIGRACOES,
    }
}
