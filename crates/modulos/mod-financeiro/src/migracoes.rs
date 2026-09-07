//! O esquema do módulo financeiro — as tabelas `financeiro_*`.
//!
//! `docs/06-modelo-de-dados.md` §1 (convenções) e `docs/modulos/financeiro.md` §3. Declara
//! `depende_de: &["nucleo", "razao"]` — `nucleo_empresa` e `razao_conta` já existem quando
//! estas rodam.

use cardeal_storage::{ConjuntoMigracoes, Migracao, TipoMigracao};

const SQL_TITULOS: &str = r"
CREATE TABLE financeiro_titulo (
    id               BLOB PRIMARY KEY,
    empresa          BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    especie          TEXT    NOT NULL CHECK (especie IN ('Receber','Pagar')),
    contraparte_tipo TEXT    NOT NULL CHECK (contraparte_tipo IN ('Cliente','Fornecedor','Funcionario','Socio','Outro')),
    contraparte_id   BLOB    NOT NULL,
    origem_modulo    TEXT    NOT NULL,
    origem_id        BLOB,
    emissao          INTEGER NOT NULL,
    valor_original   INTEGER NOT NULL,
    forma_cobranca   TEXT    NOT NULL,
    centro_custo     BLOB,
    observacao       TEXT,
    cancelado_em     INTEGER,
    versao           INTEGER NOT NULL DEFAULT 1,
    criado_em        INTEGER NOT NULL,
    criado_por       BLOB    NOT NULL
) STRICT;
CREATE INDEX financeiro_titulo_contraparte
    ON financeiro_titulo(empresa, contraparte_tipo, contraparte_id);

CREATE TABLE financeiro_parcela (
    id             BLOB PRIMARY KEY,
    empresa        BLOB    NOT NULL,
    titulo         BLOB    NOT NULL REFERENCES financeiro_titulo(id),
    numero         INTEGER NOT NULL,
    vencimento     INTEGER NOT NULL,
    valor          INTEGER NOT NULL,
    estado         TEXT    NOT NULL CHECK (estado IN ('Aberta','Parcial','Quitada','Cancelada','Renegociada')),
    valor_baixado  INTEGER NOT NULL DEFAULT 0,
    lancamento     BLOB,
    nosso_numero   TEXT,
    politica_juros TEXT    NOT NULL CHECK (politica_juros IN ('Nenhum','SimplesDiario','SimplesMensal')),
    taxa_juros     INTEGER,
    multa          INTEGER,
    desconto_ate   INTEGER,
    desconto_valor INTEGER,
    versao         INTEGER NOT NULL DEFAULT 1
) STRICT;
CREATE INDEX financeiro_parcela_titulo ON financeiro_parcela(titulo, numero);
CREATE INDEX financeiro_parcela_vencendo
    ON financeiro_parcela(empresa, vencimento) WHERE estado IN ('Aberta','Parcial');

CREATE TABLE financeiro_baixa (
    id             BLOB PRIMARY KEY,
    empresa        BLOB    NOT NULL,
    parcela        BLOB    NOT NULL REFERENCES financeiro_parcela(id),
    data           INTEGER NOT NULL,
    valor_recebido INTEGER NOT NULL,
    principal      INTEGER NOT NULL,
    juros          INTEGER NOT NULL,
    multa          INTEGER NOT NULL,
    desconto       INTEGER NOT NULL,
    lancamento     BLOB    NOT NULL,
    estornada_em   INTEGER,
    criado_em      INTEGER NOT NULL,
    criado_por     BLOB    NOT NULL
) STRICT;
CREATE INDEX financeiro_baixa_parcela ON financeiro_baixa(parcela, data);
";

const SQL_CAIXA: &str = r"
CREATE TABLE financeiro_caixa (
    id               BLOB PRIMARY KEY,
    empresa          BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    nome             TEXT    NOT NULL,
    local_operacao   BLOB,
    conta_razao      BLOB    NOT NULL,
    permite_negativo INTEGER NOT NULL DEFAULT 0 CHECK (permite_negativo IN (0,1)),
    ativo            INTEGER NOT NULL DEFAULT 1 CHECK (ativo IN (0,1)),
    versao           INTEGER NOT NULL DEFAULT 1
) STRICT;
CREATE INDEX financeiro_caixa_empresa ON financeiro_caixa(empresa) WHERE ativo = 1;

CREATE TABLE financeiro_sessao_caixa (
    id             BLOB PRIMARY KEY,
    empresa        BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    caixa          BLOB    NOT NULL REFERENCES financeiro_caixa(id),
    operador       BLOB    NOT NULL,
    dispositivo    BLOB    NOT NULL,
    abertura       INTEGER NOT NULL,
    fechamento     INTEGER,
    valor_abertura INTEGER NOT NULL,
    valor_esperado INTEGER,
    valor_contado  INTEGER,
    quebra         INTEGER,
    estado         TEXT    NOT NULL CHECK (estado IN ('Aberta','Fechada','Auditada')),
    versao         INTEGER NOT NULL DEFAULT 1
) STRICT;
-- Só pode haver uma sessão Aberta por caixa — reforça em SQL a regra que `AbrirCaixa` já
-- checa na aplicação (`docs/modulos/financeiro.md` §5, erro `CaixaJaAberto`).
CREATE UNIQUE INDEX financeiro_sessao_caixa_uma_aberta
    ON financeiro_sessao_caixa(caixa) WHERE estado = 'Aberta';

CREATE TABLE financeiro_movimento_caixa (
    id              BLOB PRIMARY KEY,
    empresa         BLOB    NOT NULL,
    sessao          BLOB    NOT NULL REFERENCES financeiro_sessao_caixa(id),
    tipo            TEXT    NOT NULL CHECK (tipo IN ('Suprimento','Sangria','Venda','Recebimento','Pagamento','QuebraCaixa')),
    valor           INTEGER NOT NULL,
    forma_pagamento BLOB,
    lancamento      BLOB    NOT NULL,
    motivo          TEXT,
    criado_em       INTEGER NOT NULL,
    criado_por      BLOB    NOT NULL
) STRICT;
CREATE INDEX financeiro_movimento_caixa_sessao ON financeiro_movimento_caixa(sessao, criado_em);
";

const SQL_CATEGORIA_E_RECORRENCIA: &str = r"
CREATE TABLE financeiro_categoria (
    id      BLOB PRIMARY KEY,
    empresa BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    nome    TEXT    NOT NULL,
    especie TEXT    CHECK (especie IN ('Receber','Pagar')),
    ativa   INTEGER NOT NULL DEFAULT 1 CHECK (ativa IN (0,1))
) STRICT;
CREATE UNIQUE INDEX financeiro_categoria_nome_ativa
    ON financeiro_categoria(empresa, nome) WHERE ativa = 1;

CREATE TABLE financeiro_recorrencia (
    id                        BLOB PRIMARY KEY,
    empresa                   BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    descricao                 TEXT    NOT NULL,
    especie                   TEXT    NOT NULL CHECK (especie IN ('Receber','Pagar')),
    contraparte_tipo          TEXT    NOT NULL CHECK (contraparte_tipo IN ('Cliente','Fornecedor','Funcionario','Socio','Outro')),
    contraparte_id            BLOB    NOT NULL,
    tipo_valor                TEXT    NOT NULL CHECK (tipo_valor IN ('Fixo','Indexado','Variavel')),
    valor_fixo                INTEGER,
    indice                    TEXT,
    media_ultimos_n           INTEGER,
    periodicidade             TEXT    NOT NULL CHECK (periodicidade IN ('Mensal','Semanal','Anual','Personalizada')),
    dia_referencia            INTEGER,
    expressao_cron            TEXT,
    inicio                    INTEGER NOT NULL,
    fim                       INTEGER,
    conta_contrapartida       BLOB    NOT NULL,
    centro_custo              BLOB,
    categoria                 BLOB    REFERENCES financeiro_categoria(id),
    antecedencia_geracao_dias INTEGER NOT NULL,
    ativa                     INTEGER NOT NULL DEFAULT 1 CHECK (ativa IN (0,1)),
    versao                    INTEGER NOT NULL DEFAULT 1
) STRICT;
CREATE INDEX financeiro_recorrencia_empresa_ativa
    ON financeiro_recorrencia(empresa) WHERE ativa = 1;

ALTER TABLE financeiro_titulo ADD COLUMN categoria BLOB REFERENCES financeiro_categoria(id);
";

const MIGRACOES: &[Migracao] = &[
    Migracao {
        versao: 1,
        nome: "financeiro_titulos_e_baixas",
        sql: SQL_TITULOS,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 2,
        nome: "financeiro_caixa",
        sql: SQL_CAIXA,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 3,
        nome: "financeiro_categoria_e_recorrencia",
        sql: SQL_CATEGORIA_E_RECORRENCIA,
        tipo: TipoMigracao::Esquema,
    },
];

/// O conjunto de migrações do módulo financeiro.
#[must_use]
pub fn conjunto() -> ConjuntoMigracoes {
    ConjuntoMigracoes {
        modulo: "financeiro",
        depende_de: &["nucleo", "razao"],
        migracoes: MIGRACOES,
    }
}
