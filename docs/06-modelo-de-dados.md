# 06 — Modelo de dados

## 1. Convenções obrigatórias

| Regra | Exemplo |
|---|---|
| Tabela prefixada pelo módulo dono | `estoque_produto`, `razao_lancamento`, `nucleo_usuario` |
| `snake_case`, singular | `venda_item`, não `VendaItens` |
| Toda tabela é `STRICT` | força tipagem real no SQLite |
| PK sempre `id BLOB(16)` UUIDv7 | ordenável no tempo, sem hotspot de índice |
| Toda entidade mutável tem `versao INTEGER` | bloqueio otimista |
| Toda entidade tem `criado_em`, `criado_por` | e `alterado_em`, `alterado_por` se mutável |
| Multiempresa: `empresa BLOB(16) NOT NULL` | primeira coluna de todo índice de negócio |
| Dinheiro: `INTEGER` em centavos | nunca `REAL`, nunca `TEXT` |
| Data: `INTEGER` dias desde 1970-01-01 | civil, sem fuso |
| Instante: `INTEGER` microssegundos UTC | |
| Booleano: `INTEGER` 0/1 com `CHECK` | |
| Enum: `TEXT` com `CHECK (col IN (...))` | legível em inspeção manual, validado |
| Sem `ON DELETE CASCADE` | nada é apagado; usamos `ativo`/`cancelado_em` |
| Sem trigger | lógica em Rust, testável |
| Sem view materializada por trigger | projeções são calculadas ou reconstruíveis |

### Por que UUIDv7 e não `INTEGER AUTOINCREMENT`

- Terminal offline gera id sem falar com o servidor — **requisito do modo autônomo**.
- Ordenado no tempo: o índice B-tree cresce à direita, sem fragmentação (problema do UUIDv4).
- 16 bytes fixos, comparação rápida, sem colisão prática.
- O número **legível** para o usuário (`numero INTEGER`) é separado, sequencial por empresa, e é
  atribuído no servidor no momento do commit.

### Por que dias-desde-época para datas

Data de competência é **civil** — "10 de março" não tem fuso horário. Guardar como timestamp gera a
classe de bug em que o lançamento do dia 1º aparece no mês anterior para quem está em outro fuso.
`Data` é um `i32` de dias, `Instante` é um `i64` de micros UTC, e a conversão entre os dois exige um
fuso explícito. Ver `cardeal-kernel::tempo`.

## 2. Tabelas do núcleo

```sql
-- ─────────────────────────────────────────── identidade e organização

CREATE TABLE nucleo_empresa (
    id            BLOB PRIMARY KEY,
    razao_social  TEXT    NOT NULL,
    nome_fantasia TEXT    NOT NULL,
    cnpj          TEXT    NOT NULL,
    ie            TEXT,
    im            TEXT,
    regime        TEXT    NOT NULL CHECK (regime IN ('MEI','SimplesNacional','LucroPresumido','LucroReal')),
    matriz        BLOB    REFERENCES nucleo_empresa(id),
    endereco      TEXT    NOT NULL,             -- JSON
    perfil        TEXT    NOT NULL,             -- id do perfil ativo
    ativa         INTEGER NOT NULL DEFAULT 1 CHECK (ativa IN (0,1)),
    versao        INTEGER NOT NULL DEFAULT 1,
    criado_em     INTEGER NOT NULL
) STRICT;

CREATE TABLE nucleo_usuario (
    id            BLOB PRIMARY KEY,
    login         TEXT    NOT NULL UNIQUE,
    nome          TEXT    NOT NULL,
    email         TEXT,
    senha_hash    TEXT    NOT NULL,             -- Argon2id, PHC string
    senha_trocada_em INTEGER NOT NULL,
    exige_troca   INTEGER NOT NULL DEFAULT 0 CHECK (exige_troca IN (0,1)),
    mfa_segredo   TEXT,                          -- TOTP, opcional
    ativo         INTEGER NOT NULL DEFAULT 1 CHECK (ativo IN (0,1)),
    bloqueado_ate INTEGER,
    tentativas    INTEGER NOT NULL DEFAULT 0,
    versao        INTEGER NOT NULL DEFAULT 1,
    criado_em     INTEGER NOT NULL,
    criado_por    BLOB
) STRICT;

CREATE TABLE nucleo_papel (
    id          BLOB PRIMARY KEY,
    empresa     BLOB REFERENCES nucleo_empresa(id),  -- NULL = papel global do sistema
    nome        TEXT    NOT NULL,
    descricao   TEXT,
    sistema     INTEGER NOT NULL DEFAULT 0,          -- papéis de fábrica não são editáveis
    versao      INTEGER NOT NULL DEFAULT 1
) STRICT;

CREATE TABLE nucleo_papel_permissao (
    papel      BLOB NOT NULL REFERENCES nucleo_papel(id),
    permissao  TEXT NOT NULL,                        -- "financeiro.pagar.baixar"
    PRIMARY KEY (papel, permissao)
) STRICT, WITHOUT ROWID;

CREATE TABLE nucleo_papel_limite (                   -- os limites de doc 08 §3.4
    papel      BLOB NOT NULL REFERENCES nucleo_papel(id),
    chave      TEXT NOT NULL,                        -- "vendas.desconto_maximo"
    tipo       TEXT NOT NULL CHECK (tipo IN ('Dinheiro','Percentual','Contagem','Dias','Ilimitado')),
    valor      INTEGER NOT NULL DEFAULT 0,           -- teto na unidade interna da dimensão
    PRIMARY KEY (papel, chave)
) STRICT, WITHOUT ROWID;

CREATE TABLE nucleo_usuario_papel (
    usuario  BLOB NOT NULL REFERENCES nucleo_usuario(id),
    papel    BLOB NOT NULL REFERENCES nucleo_papel(id),
    empresa  BLOB REFERENCES nucleo_empresa(id),     -- escopo: papel vale nesta empresa
    PRIMARY KEY (usuario, papel, empresa)
) STRICT, WITHOUT ROWID;

CREATE TABLE nucleo_dispositivo (
    id             BLOB PRIMARY KEY,
    apelido        TEXT    NOT NULL,                 -- "PDV 1 — Caixa da frente"
    tipo           TEXT    NOT NULL CHECK (tipo IN ('Servidor','Pdv','Retaguarda','Movel')),
    chave_publica  BLOB    NOT NULL,
    certificado    BLOB,
    empresa        BLOB    REFERENCES nucleo_empresa(id),
    ativo          INTEGER NOT NULL DEFAULT 1 CHECK (ativo IN (0,1)),
    visto_em       INTEGER,
    versao_app     TEXT,
    criado_em      INTEGER NOT NULL
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

-- ─────────────────────────────────────────── módulos e configuração

CREATE TABLE nucleo_modulo_ativo (
    empresa    BLOB NOT NULL REFERENCES nucleo_empresa(id),
    modulo     TEXT NOT NULL,
    submodulo  TEXT NOT NULL DEFAULT '',      -- '' = o módulo em si
    ativo      INTEGER NOT NULL CHECK (ativo IN (0,1)),
    ativado_em INTEGER NOT NULL,
    ativado_por BLOB,
    PRIMARY KEY (empresa, modulo, submodulo)
) STRICT, WITHOUT ROWID;

CREATE TABLE nucleo_configuracao (
    empresa BLOB NOT NULL,
    chave   TEXT NOT NULL,                    -- "pdv.turno_obrigatorio"
    valor   TEXT NOT NULL,                    -- JSON
    PRIMARY KEY (empresa, chave)
) STRICT, WITHOUT ROWID;

CREATE TABLE nucleo_migracao (
    modulo     TEXT    NOT NULL,
    versao     INTEGER NOT NULL,
    nome       TEXT    NOT NULL,
    hash       BLOB    NOT NULL,              -- BLAKE3 do SQL aplicado
    aplicada_em INTEGER NOT NULL,
    duracao_ms INTEGER NOT NULL,
    PRIMARY KEY (modulo, versao)
) STRICT, WITHOUT ROWID;

-- ─────────────────────────────────────────── confiabilidade

CREATE TABLE nucleo_outbox (
    seq         INTEGER PRIMARY KEY AUTOINCREMENT,  -- ordem total de entrega
    empresa     BLOB    NOT NULL,
    tipo        TEXT    NOT NULL,                   -- "vendas.pedido_faturado.v1"
    agregado    BLOB,
    carga       BLOB    NOT NULL,                   -- postcard
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
    recurso    TEXT PRIMARY KEY,               -- "caixa:<uuid>"
    dono       BLOB NOT NULL,                  -- sessão
    expira_em  INTEGER NOT NULL,
    motivo     TEXT
) STRICT, WITHOUT ROWID;

CREATE TABLE nucleo_auditoria (
    seq         INTEGER PRIMARY KEY AUTOINCREMENT,
    empresa     BLOB    NOT NULL,
    usuario     BLOB    NOT NULL,
    dispositivo BLOB    NOT NULL,
    quando      INTEGER NOT NULL,
    acao        TEXT    NOT NULL,              -- "financeiro.pagar.baixar"
    entidade    TEXT    NOT NULL,
    entidade_id BLOB,
    antes       BLOB,                          -- JSON, só campos alterados
    depois      BLOB,
    hash_anterior BLOB NOT NULL,               -- encadeamento: detecta remoção de linha
    hash        BLOB NOT NULL
) STRICT;
CREATE INDEX nucleo_auditoria_entidade ON nucleo_auditoria(entidade, entidade_id, seq DESC);

CREATE TABLE nucleo_sequencia (
    empresa BLOB NOT NULL,
    nome    TEXT NOT NULL,                     -- "lancamento", "venda", "nfce.serie1"
    proximo INTEGER NOT NULL,
    PRIMARY KEY (empresa, nome)
) STRICT, WITHOUT ROWID;
```

### 2.1 Auditoria encadeada

`hash = BLAKE3(hash_anterior || seq || quando || usuario || acao || entidade_id || antes || depois)`.

Remover ou alterar uma linha quebra a cadeia. A verificação diária percorre a cadeia e reporta o
`seq` exato da quebra. Isso transforma a auditoria de "tabela que alguém pode limpar" em prova.

## 3. Tabelas do razão

```sql
CREATE TABLE razao_conta (
    id               BLOB PRIMARY KEY,
    empresa          BLOB NOT NULL REFERENCES nucleo_empresa(id),
    codigo           TEXT NOT NULL,
    nome             TEXT NOT NULL,
    natureza         TEXT NOT NULL CHECK (natureza IN ('Ativo','Passivo','PatrimonioLiquido','Receita','Despesa')),
    tipo             TEXT NOT NULL CHECK (tipo IN ('Sintetica','Analitica')),
    pai              BLOB REFERENCES razao_conta(id),
    nivel            INTEGER NOT NULL,
    grupo_fluxo      TEXT,          -- 'Operacional','Investimento','Financiamento', NULL = não é caixa
    modulo_origem    TEXT,
    papel_padrao     TEXT,          -- 'caixa','estoque','clientes',... usado pelo receituário
    ativa            INTEGER NOT NULL DEFAULT 1 CHECK (ativa IN (0,1)),
    versao           INTEGER NOT NULL DEFAULT 1,
    UNIQUE (empresa, codigo)
) STRICT;

CREATE TABLE razao_lancamento (
    id             BLOB PRIMARY KEY,
    empresa        BLOB    NOT NULL,
    numero         INTEGER NOT NULL,
    competencia    INTEGER NOT NULL,
    vencimento     INTEGER,
    liquidacao     INTEGER,
    estado         TEXT    NOT NULL CHECK (estado IN ('Previsto','Confirmado','Realizado','Estornado')),
    origem_modulo  TEXT    NOT NULL,
    origem_tipo    TEXT    NOT NULL,
    origem_id      BLOB,
    historico      TEXT    NOT NULL,
    estorna        BLOB REFERENCES razao_lancamento(id),
    estornado_por  BLOB REFERENCES razao_lancamento(id),
    criado_em      INTEGER NOT NULL,
    criado_por     BLOB    NOT NULL,
    dispositivo    BLOB    NOT NULL,
    UNIQUE (empresa, numero)
) STRICT;

CREATE INDEX razao_lanc_competencia ON razao_lancamento(empresa, competencia, estado);
CREATE INDEX razao_lanc_fluxo       ON razao_lancamento(empresa, liquidacao, vencimento, estado);
CREATE INDEX razao_lanc_origem      ON razao_lancamento(origem_modulo, origem_tipo, origem_id);

CREATE TABLE razao_partida (
    lancamento     BLOB    NOT NULL REFERENCES razao_lancamento(id),
    ordem          INTEGER NOT NULL,
    conta          BLOB    NOT NULL REFERENCES razao_conta(id),
    valor          INTEGER NOT NULL,        -- centavos, + débito / - crédito
    contraparte_tipo TEXT,                  -- 'Cliente','Fornecedor','Funcionario','Socio'
    contraparte_id BLOB,
    centro_custo   BLOB,
    projeto        BLOB,
    documento      TEXT,
    quantidade     INTEGER,                 -- escala 1e-4
    complemento    TEXT,
    PRIMARY KEY (lancamento, ordem)
) STRICT, WITHOUT ROWID;

CREATE INDEX razao_partida_conta       ON razao_partida(conta, lancamento);
CREATE INDEX razao_partida_contraparte ON razao_partida(contraparte_tipo, contraparte_id, lancamento);
CREATE INDEX razao_partida_cc          ON razao_partida(centro_custo, lancamento) WHERE centro_custo IS NOT NULL;

CREATE TABLE razao_saldo_mensal (        -- derivado, reconstruível
    empresa  BLOB    NOT NULL,
    conta    BLOB    NOT NULL,
    ano_mes  INTEGER NOT NULL,           -- 202603
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
```

**Nota sobre `WITHOUT ROWID`:** usamos em tabelas com PK composta e linhas pequenas
(`razao_partida`, tabelas de junção, configuração). Elimina o índice secundário e o nível de
indireção do rowid — em `razao_partida`, que é a maior tabela do sistema, economiza cerca de
**18% de espaço** e uma busca de B-tree por leitura.

## 4. Migrações

Um arquivo SQL por versão, dentro do módulo dono:

```
crates/modulos/mod-estoque/migracoes/
├── 0001_inicial.sql
├── 0002_lote_e_validade.sql
└── 0003_indice_saldo_local.sql
```

Regras:

1. Migração publicada é **imutável**. Corrigir = nova migração. O `hash` na tabela `nucleo_migracao`
   detecta alteração e o motor recusa subir.
2. Migração roda em transação. Falhou = rollback, motor não sobe, mensagem clara.
3. Migração que precisa transformar dados em massa roda em lotes com progresso na UI.
4. Toda migração tem teste em `testes/migracoes.rs` que aplica da versão anterior e valida.
5. Ordem: núcleo → módulos em ordem topológica → índices adiados.

```rust
pub struct Migracao {
    pub versao: u32,
    pub nome: &'static str,
    pub sql: &'static str,
    pub tipo: TipoMigracao,   // Esquema | Dados | Indice
}
```

## 5. Estratégia de índices

Regras que evitam o pior erro de ERP (índice demais ou de menos):

- Todo índice de negócio começa por `empresa`.
- Índice parcial (`WHERE ativo = 1`, `WHERE entregue_em IS NULL`) sempre que a seletividade justificar
  — SQLite suporta e o ganho de tamanho é grande.
- Índice de cobertura para as 5 consultas mais quentes de cada módulo, documentadas no README do módulo.
- Nenhum índice sem consulta correspondente documentada. `xtask indices --orfaos` lista índices sem
  uso registrado no plano de nenhuma consulta conhecida.
- `ANALYZE` roda semanalmente e após grandes cargas.

## 6. Tamanho esperado

Base de um comércio médio (1.200 cupons/dia, 3 anos, 8.000 SKUs):

| Tabela | Linhas | Tamanho |
|---|---|---|
| `razao_partida` | ~13 M | ~1,1 GB |
| `razao_lancamento` | ~3,2 M | ~420 MB |
| `venda_item` | ~4,3 M | ~380 MB |
| `estoque_movimento` | ~4,3 M | ~310 MB |
| demais | — | ~200 MB |
| **Total** | | **~2,4 GB** |

SQLite lida com isso confortavelmente. O plano de arquivamento (mover exercícios encerrados para um
`.db` anexado, somente leitura) entra a partir de ~20 GB — documentado em
[ADR-0011](adr/0011-arquivamento-de-exercicios.md).
