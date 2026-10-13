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

// Migração v3 (2026-09-11): cadastro técnico de peça de microeletrônica (fabricante, MPN,
// categoria, especificação, compatibilidade, garantia do fornecedor, localização física) —
// pedido do usuário. Aditiva, todas as colunas NULL por padrão: linhas existentes continuam
// válidas sem preencher nada. Nunca edite as migrações v1/v2 acima — o usuário já tem um
// banco local de verdade rodando.
const SQL_DETALHES_TECNICOS: &str = r"
ALTER TABLE estoque_produto ADD COLUMN fabricante TEXT;
ALTER TABLE estoque_produto ADD COLUMN codigo_fabricante TEXT;
ALTER TABLE estoque_produto ADD COLUMN categoria_tecnica TEXT;
ALTER TABLE estoque_produto ADD COLUMN especificacao_tecnica TEXT;
ALTER TABLE estoque_produto ADD COLUMN compatibilidade TEXT;
ALTER TABLE estoque_produto ADD COLUMN garantia_fornecedor_dias INTEGER;
ALTER TABLE estoque_produto ADD COLUMN localizacao_fisica TEXT;
";

// Migração v4 (2026-09-13): rastreabilidade individual de peça por código curto (post-it) —
// pedido do dono da assistência técnica. `estoque_aparelho_origem` é o aparelho usado
// desmontado de onde uma peça pode ter sido retirada, com o custo de aquisição dele;
// `estoque_lote` finalmente implementa a tabela que já estava só no domínio (`Lote`, nunca
// persistida) — agora com `codigo` (o texto do post-it, único por empresa, não só por
// produto — o técnico digita sem saber de antemão o produto), `custo_unitario` próprio (a
// peça específica pode ter custado diferente da média do produto) e o vínculo opcional com o
// aparelho de origem. Aditiva: nenhuma tabela existente muda.
const SQL_LOTE_E_APARELHO_ORIGEM: &str = r"
CREATE TABLE estoque_aparelho_origem (
    id                BLOB PRIMARY KEY,
    empresa           BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    descricao         TEXT    NOT NULL,
    identificador     TEXT,
    custo_aquisicao   INTEGER NOT NULL,
    adquirido_em      INTEGER NOT NULL,
    fornecedor        BLOB,
    observacoes       TEXT
) STRICT;
CREATE INDEX estoque_aparelho_origem_empresa ON estoque_aparelho_origem(empresa);

CREATE TABLE estoque_lote (
    id                  BLOB PRIMARY KEY,
    empresa             BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    produto             BLOB    NOT NULL REFERENCES estoque_produto(id),
    local               BLOB    NOT NULL REFERENCES estoque_local(id),
    codigo              TEXT    NOT NULL,
    origem              TEXT    NOT NULL CHECK (origem IN ('Compra','AparelhoUsado')),
    fornecedor          BLOB,
    aparelho_origem     BLOB REFERENCES estoque_aparelho_origem(id),
    fabricacao          INTEGER,
    validade            INTEGER,
    quantidade_inicial  INTEGER NOT NULL,
    custo_unitario      INTEGER NOT NULL,
    estado              TEXT    NOT NULL CHECK (estado IN ('Ativo','Vencido','Esgotado')),
    criado_em           INTEGER NOT NULL,
    UNIQUE (empresa, codigo)
) STRICT;
CREATE INDEX estoque_lote_produto ON estoque_lote(produto);
CREATE INDEX estoque_lote_aparelho_origem ON estoque_lote(aparelho_origem)
    WHERE aparelho_origem IS NOT NULL;
CREATE INDEX estoque_lote_validade ON estoque_lote(empresa, validade) WHERE estado = 'Ativo';
";

/// A listagem paginada de produtos ordena por `(nome, id)` e continua depois do cursor.
const SQL_LISTA_PAGINADA: &str = r"
CREATE INDEX estoque_produto_lista ON estoque_produto(empresa, nome, id) WHERE ativo = 1;
";

// Migração v6 (2026-10-09): formação de preço por markup divisor. Aditiva.
// * `margem_desejada` (grupo e produto): margem líquida em `Percentual` (1e-6 ponto);
//   NULL = herda do grupo-pai e, no fim, do padrão da empresa.
// * `icms_st`/`monofasico` (grupo): perfil fiscal do Simples; NULL = herda do pai.
// * `estoque_parametros_preco`: uma linha por CNPJ (empresa) — regime, custo operacional
//   (manual ou derivado do Razão), comissão, taxa de pagamento, margem padrão/mínima e se o
//   preço sugerido é obrigatório no PDV. Sem linha, vale o padrão derivado de
//   `nucleo_empresa.regime`.
const SQL_PRECIFICACAO: &str = r"
ALTER TABLE estoque_grupo_produto ADD COLUMN margem_desejada INTEGER;
ALTER TABLE estoque_grupo_produto ADD COLUMN icms_st INTEGER CHECK (icms_st IN (0,1));
ALTER TABLE estoque_grupo_produto ADD COLUMN monofasico INTEGER CHECK (monofasico IN (0,1));
ALTER TABLE estoque_produto ADD COLUMN margem_desejada INTEGER;

CREATE TABLE estoque_parametros_preco (
    empresa                    BLOB PRIMARY KEY REFERENCES nucleo_empresa(id),
    regime                     TEXT    NOT NULL CHECK (regime IN ('Mei','Simples','Manual')),
    anexo                      TEXT    NOT NULL DEFAULT 'I' CHECK (anexo IN ('I','II','III','IV','V')),
    imposto_manual             INTEGER NOT NULL DEFAULT 0,
    custo_operacional          INTEGER NOT NULL DEFAULT 0,
    custo_operacional_derivado INTEGER NOT NULL DEFAULT 0 CHECK (custo_operacional_derivado IN (0,1)),
    comissao                   INTEGER NOT NULL DEFAULT 0,
    taxa_pagamento             INTEGER NOT NULL DEFAULT 0,
    margem_padrao              INTEGER,
    margem_minima              INTEGER NOT NULL DEFAULT 0,
    preco_obrigatorio          INTEGER NOT NULL DEFAULT 0 CHECK (preco_obrigatorio IN (0,1)),
    versao                     INTEGER NOT NULL DEFAULT 1
) STRICT;
";

// Migração v7 (2026-10-09): alteração em lote do catálogo com prévia e desfazer (24h). Aditiva.
// `estoque_alteracao_lote` é o cabeçalho (quem, quando, o que mexeu, se foi desfeita) e
// `estoque_alteracao_lote_item` guarda o antes e o depois de cada produto, para desfazer só o
// que ainda está como a alteração deixou.
const SQL_ALTERACAO_EM_LOTE: &str = r"
CREATE TABLE estoque_alteracao_lote (
    id          BLOB PRIMARY KEY,
    empresa     BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    usuario     BLOB    NOT NULL,
    criado_em   INTEGER NOT NULL,
    descricao   TEXT    NOT NULL,
    campos      TEXT    NOT NULL,
    quantidade  INTEGER NOT NULL,
    desfeito_em INTEGER
) STRICT;
CREATE INDEX estoque_alteracao_lote_recentes ON estoque_alteracao_lote(empresa, criado_em);

CREATE TABLE estoque_alteracao_lote_item (
    lote           BLOB    NOT NULL REFERENCES estoque_alteracao_lote(id),
    produto        BLOB    NOT NULL REFERENCES estoque_produto(id),
    antes_grupo    BLOB    NOT NULL,
    antes_margem   INTEGER,
    antes_ativo    INTEGER NOT NULL,
    depois_grupo   BLOB    NOT NULL,
    depois_margem  INTEGER,
    depois_ativo   INTEGER NOT NULL,
    PRIMARY KEY (lote, produto)
) STRICT, WITHOUT ROWID;
";

// Migração v8 (2026-10-09): perfil fiscal completo (origem, CST/CSOSN, CFOPs, alíquotas, MVA-ST,
// CEST, benefício) herdado pela árvore de grupos, e NCM/perfil no antes/depois da alteração em
// lote. O perfil é um blob `postcard` de `CamposFiscais` (nulo = herda tudo), no grupo e no produto.
const SQL_PERFIL_FISCAL: &str = r"
ALTER TABLE estoque_grupo_produto ADD COLUMN perfil_fiscal BLOB;
ALTER TABLE estoque_produto ADD COLUMN perfil_fiscal BLOB;
ALTER TABLE estoque_alteracao_lote_item ADD COLUMN antes_ncm TEXT;
ALTER TABLE estoque_alteracao_lote_item ADD COLUMN depois_ncm TEXT;
ALTER TABLE estoque_alteracao_lote_item ADD COLUMN antes_fiscal BLOB;
ALTER TABLE estoque_alteracao_lote_item ADD COLUMN depois_fiscal BLOB;
";

// Migração v9 (2026-10-12): qual custo alimenta o preço sugerido (médio, última compra ou o
// maior dos dois). O padrão preserva o comportamento anterior (custo médio).
const SQL_CUSTO_DE_REFERENCIA: &str = r"
ALTER TABLE estoque_parametros_preco ADD COLUMN custo_referencia TEXT NOT NULL DEFAULT 'CustoMedio'
    CHECK (custo_referencia IN ('CustoMedio','UltimaCompra','MaiorDosDois'));
";

// Migração v10 (2026-10-13): comissão de vendedor por estrutura mercadológica — o percentual
// próprio do grupo e do produto (nulo = herda), como a margem, e o antes/depois na alteração em
// lote. O padrão continua sendo `estoque_parametros_preco.comissao`.
const SQL_COMISSAO_POR_ESTRUTURA: &str = r"
ALTER TABLE estoque_grupo_produto ADD COLUMN comissao INTEGER;
ALTER TABLE estoque_produto ADD COLUMN comissao INTEGER;
ALTER TABLE estoque_alteracao_lote_item ADD COLUMN antes_comissao INTEGER;
ALTER TABLE estoque_alteracao_lote_item ADD COLUMN depois_comissao INTEGER;
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
    Migracao {
        versao: 3,
        nome: "estoque_produto_detalhes_tecnicos",
        sql: SQL_DETALHES_TECNICOS,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 4,
        nome: "estoque_lote_e_aparelho_origem",
        sql: SQL_LOTE_E_APARELHO_ORIGEM,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 5,
        nome: "estoque_lista_paginada",
        sql: SQL_LISTA_PAGINADA,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 6,
        nome: "estoque_precificacao",
        sql: SQL_PRECIFICACAO,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 7,
        nome: "estoque_alteracao_em_lote",
        sql: SQL_ALTERACAO_EM_LOTE,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 8,
        nome: "estoque_perfil_fiscal",
        sql: SQL_PERFIL_FISCAL,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 9,
        nome: "estoque_custo_de_referencia",
        sql: SQL_CUSTO_DE_REFERENCIA,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 10,
        nome: "estoque_comissao_por_estrutura",
        sql: SQL_COMISSAO_POR_ESTRUTURA,
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
