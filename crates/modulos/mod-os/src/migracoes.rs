//! O esquema do módulo de ordens de serviço — as tabelas `os_*`.
//!
//! `docs/modulos/os.md` §13, adaptado à fatia mínima desta primeira versão: sem
//! `os_reincidencia` ainda (fica para `AcionarGarantia`). Declara
//! `depende_de: &["nucleo", "razao"]` — `nucleo_empresa` e `razao_conta` já existem quando
//! estas rodam.

use cardeal_storage::{ConjuntoMigracoes, Migracao, TipoMigracao};

const SQL_ORDEM_E_ITENS: &str = r"
CREATE TABLE os_ordem_servico (
    id                   BLOB PRIMARY KEY,
    empresa              BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    numero               INTEGER NOT NULL,
    cliente              BLOB    NOT NULL,
    equipamento          TEXT    NOT NULL,
    data_abertura        INTEGER NOT NULL,
    tecnico_responsavel  BLOB    NOT NULL,
    estado               TEXT    NOT NULL CHECK (estado IN ('Aberta','EmDiagnostico','AguardandoAprovacao','Aprovada','EmExecucao','Concluida','Faturada','Cancelada','Reprovada')),
    aprovado_por         TEXT,
    garantia_dias        INTEGER NOT NULL DEFAULT 90,
    valor_total          INTEGER NOT NULL DEFAULT 0,
    versao               INTEGER NOT NULL DEFAULT 1,
    UNIQUE (empresa, numero)
) STRICT;
CREATE INDEX os_ordem_estado ON os_ordem_servico(empresa, estado);
CREATE INDEX os_ordem_cliente_equip ON os_ordem_servico(cliente, equipamento);

CREATE TABLE os_laudo_tecnico (
    id                  BLOB PRIMARY KEY,
    ordem_servico       BLOB    NOT NULL REFERENCES os_ordem_servico(id),
    descricao_problema  TEXT    NOT NULL,
    diagnostico         TEXT,
    tecnico             BLOB    NOT NULL,
    criado_em           INTEGER NOT NULL
) STRICT;

CREATE TABLE os_item_peca (
    id                BLOB PRIMARY KEY,
    ordem_servico     BLOB    NOT NULL REFERENCES os_ordem_servico(id),
    produto           BLOB    NOT NULL,
    quantidade        INTEGER NOT NULL,
    preco_unitario    INTEGER NOT NULL,
    custo_unitario    INTEGER NOT NULL DEFAULT 0,
    coberto_garantia  INTEGER NOT NULL DEFAULT 0 CHECK (coberto_garantia IN (0,1)),
    aplicada          INTEGER NOT NULL DEFAULT 0 CHECK (aplicada IN (0,1))
) STRICT;
CREATE INDEX os_item_peca_ordem ON os_item_peca(ordem_servico);

CREATE TABLE os_item_mao_de_obra (
    id             BLOB PRIMARY KEY,
    ordem_servico  BLOB    NOT NULL REFERENCES os_ordem_servico(id),
    descricao      TEXT    NOT NULL,
    valor          INTEGER NOT NULL,
    tecnico        BLOB    NOT NULL,
    horas          INTEGER
) STRICT;
CREATE INDEX os_item_mao_de_obra_ordem ON os_item_mao_de_obra(ordem_servico);
";

const SQL_ITENS_ORCAMENTO: &str = r"
ALTER TABLE os_ordem_servico ADD COLUMN itens_orcamento INTEGER NOT NULL DEFAULT 0;
";

const SQL_APONTAMENTO_TEMPO: &str = r"
CREATE TABLE os_apontamento_tempo (
    id             BLOB PRIMARY KEY,
    ordem_servico  BLOB    NOT NULL REFERENCES os_ordem_servico(id),
    tecnico        BLOB    NOT NULL,
    inicio         INTEGER NOT NULL,
    fim            INTEGER,
    ajustado       INTEGER NOT NULL DEFAULT 0 CHECK (ajustado IN (0,1)),
    motivo_ajuste  TEXT,
    versao         INTEGER NOT NULL DEFAULT 1
) STRICT;
CREATE INDEX os_apontamento_ordem ON os_apontamento_tempo(ordem_servico);
CREATE INDEX os_apontamento_tecnico_aberto ON os_apontamento_tempo(tecnico, fim);
";

// Migração v4 (2026-09-11): defeito relatado pelo cliente, capturado na recepção — pedido
// explícito do usuário de que abrir OS exija só nome do cliente + este campo. Aditiva:
// `DEFAULT ''` preenche as linhas existentes sem quebrar o banco local já em uso. Não
// alteramos/removemos `os_laudo_tecnico.descricao_problema` — mesma cautela: uma DROP COLUMN
// arrisca a base de produção do usuário sem necessidade real (o campo só deixa de ser a fonte
// de verdade do relato do cliente, sem causar dano ficando onde está).
const SQL_DEFEITO_RELATADO: &str = r"
ALTER TABLE os_ordem_servico ADD COLUMN defeito_relatado TEXT NOT NULL DEFAULT '';
";

const MIGRACOES: &[Migracao] = &[
    Migracao {
        versao: 1,
        nome: "os_ordem_laudo_itens",
        sql: SQL_ORDEM_E_ITENS,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 2,
        nome: "os_ordem_itens_orcamento",
        sql: SQL_ITENS_ORCAMENTO,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 3,
        nome: "os_apontamento_tempo",
        sql: SQL_APONTAMENTO_TEMPO,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 4,
        nome: "os_defeito_relatado",
        sql: SQL_DEFEITO_RELATADO,
        tipo: TipoMigracao::Esquema,
    },
];

/// O conjunto de migrações do módulo de ordens de serviço.
#[must_use]
pub fn conjunto() -> ConjuntoMigracoes {
    ConjuntoMigracoes {
        modulo: "os",
        depende_de: &["nucleo", "razao"],
        migracoes: MIGRACOES,
    }
}
