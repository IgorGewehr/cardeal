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

/// v2: o estorno da venda finalizada. Fica numa tabela à parte, e não num estado novo no
/// `CHECK` de `pdv_cupom`: o SQLite não altera um `CHECK`, e recriar `pdv_cupom` com as chaves
/// estrangeiras ligadas quebraria itens e pagamentos, que apontam para ela. Um cupom
/// `Finalizado` com linha aqui é `Estornado` (o repositório faz a tradução).
const SQL_ESTORNO: &str = r"
CREATE TABLE pdv_estorno (
    cupom          BLOB PRIMARY KEY REFERENCES pdv_cupom(id),
    empresa        BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    motivo         TEXT    NOT NULL,
    autorizado_por BLOB    NOT NULL,
    estornado_por  BLOB    NOT NULL,
    estornado_em   INTEGER NOT NULL
) STRICT;
CREATE INDEX pdv_estorno_empresa ON pdv_estorno(empresa, estornado_em);
CREATE INDEX pdv_cupom_empresa_abertura ON pdv_cupom(empresa, abertura);
";

/// v3: a impressão do PDV — a bobina (80 ou 58 mm), se o comprovante sai sozinho ao finalizar
/// a venda e o rodapé dele. Por empresa (um balcão, em geral, tem uma impressora).
const SQL_IMPRESSAO: &str = r"
CREATE TABLE pdv_preferencias (
    empresa              BLOB PRIMARY KEY REFERENCES nucleo_empresa(id),
    bobina               TEXT    NOT NULL DEFAULT '80' CHECK (bobina IN ('80','58')),
    imprimir_comprovante INTEGER NOT NULL DEFAULT 0 CHECK (imprimir_comprovante IN (0,1)),
    rodape               TEXT
) STRICT;
";

/// v4: a venda a prazo — a forma `Prazo` e a condição (1º vencimento, parcelas, intervalo) no
/// pagamento. O SQLite não muda um `CHECK`: a tabela é recriada (nenhuma outra aponta para ela).
const SQL_PRAZO: &str = r"
CREATE TABLE pdv_pagamento_cupom_v4 (
    id                  BLOB PRIMARY KEY,
    cupom               BLOB    NOT NULL REFERENCES pdv_cupom(id),
    forma               TEXT    NOT NULL CHECK (forma IN ('Dinheiro','Pix','Debito','Credito','Prazo')),
    valor               INTEGER NOT NULL,
    primeiro_vencimento INTEGER,
    parcelas            INTEGER,
    intervalo_dias      INTEGER
) STRICT;
INSERT INTO pdv_pagamento_cupom_v4 (id, cupom, forma, valor)
SELECT id, cupom, forma, valor FROM pdv_pagamento_cupom ORDER BY rowid;
DROP TABLE pdv_pagamento_cupom;
ALTER TABLE pdv_pagamento_cupom_v4 RENAME TO pdv_pagamento_cupom;
CREATE INDEX pdv_pagamento_cupom_cupom ON pdv_pagamento_cupom(cupom);
";

/// v6: o vendedor da venda (quem ganha a comissão; o operador é quem passa a venda).
const SQL_VENDEDOR: &str = "ALTER TABLE pdv_cupom ADD COLUMN vendedor BLOB;";

const MIGRACOES: &[Migracao] = &[
    Migracao {
        versao: 1,
        nome: "pdv_cupom_e_pagamento",
        sql: SQL_CUPOM,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 2,
        nome: "pdv_estorno_da_venda",
        sql: SQL_ESTORNO,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 3,
        nome: "pdv_preferencias_de_impressao",
        sql: SQL_IMPRESSAO,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 4,
        nome: "pdv_venda_a_prazo",
        sql: SQL_PRAZO,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 5,
        nome: "pdv_desconto_no_total",
        sql: SQL_DESCONTO_NO_TOTAL,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 6,
        nome: "pdv_vendedor_da_venda",
        sql: SQL_VENDEDOR,
        tipo: TipoMigracao::Esquema,
    },
];

// O desconto no total da venda aberta (`Shift+F5`): a regra (percentual ou valor). Ao
// finalizar ele é rateado entre os itens e as colunas voltam a NULL. `ADD COLUMN` em vez de
// recriar a tabela: `pdv_cupom` é referenciada por itens, pagamentos e estorno.
const SQL_DESCONTO_NO_TOTAL: &str = r"
ALTER TABLE pdv_cupom ADD COLUMN desconto_total_tipo TEXT
    CHECK (desconto_total_tipo IN ('Percentual', 'Valor'));
ALTER TABLE pdv_cupom ADD COLUMN desconto_total_valor INTEGER;
";

/// O conjunto de migrações do módulo de PDV.
#[must_use]
pub fn conjunto() -> ConjuntoMigracoes {
    ConjuntoMigracoes {
        modulo: "pdv",
        depende_de: &["nucleo"],
        migracoes: MIGRACOES,
    }
}
