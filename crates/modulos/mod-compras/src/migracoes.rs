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

// `pago_no_ato_padrao`: a compra já foi paga à vista, na hora — quando ligado (e um título é
// gerado), `ConfirmarEntrada` dá baixa completa nele na mesma transação em vez de deixá-lo
// pendente (`docs/modulos/compras.md` §5, `ConfirmarEntrada`/`confirmar_entrada_comum`).
const SQL_PAGO_NO_ATO: &str = r"
ALTER TABLE compras_preferencias ADD COLUMN pago_no_ato_padrao INTEGER NOT NULL DEFAULT 0 CHECK (pago_no_ato_padrao IN (0,1));
";

// Migração v3 (2026-10-13): o código de barras (cEAN) do item fica guardado — antes ele só
// servia para casar e era descartado; agora o produto criado a partir do item nasce com ele.
const SQL_CODIGO_BARRAS_ITEM: &str = r"
ALTER TABLE compras_item_nota_entrada ADD COLUMN codigo_barras TEXT;
";

// Migração v4 (2026-10-13): o desconto do item e os impostos que a nota cobra sobre ele e quem
// revende não recupera (ICMS-ST + FCP-ST + IPI) — entram no custo do item e no total da nota
// (antes o total era só produtos + despesas, e o título a pagar saía menor que o boleto).
const SQL_DESCONTO_E_IMPOSTOS_ITEM: &str = r"
ALTER TABLE compras_item_nota_entrada ADD COLUMN valor_desconto INTEGER NOT NULL DEFAULT 0;
ALTER TABLE compras_item_nota_entrada ADD COLUMN valor_impostos INTEGER NOT NULL DEFAULT 0;
";

// Migração v5 (2026-10-15): o aviso de conferência da nota — CFOP de item que não é venda
// (remessa para conserto, bonificação, consignação, entrega futura…). Nota com aviso nunca é
// confirmada sozinha.
const SQL_ALERTA_NOTA: &str = r"
ALTER TABLE compras_nota_entrada ADD COLUMN alerta TEXT;
";

// Migração v6 (2026-10-20): as parcelas da nota — as duplicatas do XML (`cobr/dup`) ou as que
// quem confirmou informou. Viram as parcelas do título a pagar.
const SQL_PARCELAS_NOTA: &str = r"
CREATE TABLE compras_nota_parcela (
    nota        BLOB    NOT NULL REFERENCES compras_nota_entrada(id),
    ordem       INTEGER NOT NULL,
    numero      TEXT    NOT NULL,
    vencimento  INTEGER NOT NULL,
    valor       INTEGER NOT NULL,
    PRIMARY KEY (nota, ordem)
) STRICT, WITHOUT ROWID;
";

// Migração v7 (2026-10-20): o XML original da nota (o que o fornecedor emitiu, como veio) —
// a lei manda guardar por 5 anos e o contador o recebe no pacote do mês. Antes ele só servia
// para importar e era descartado; a nota importada antes disso ganha o XML ao importar o mesmo
// arquivo de novo.
const SQL_XML_NOTA: &str = r"
ALTER TABLE compras_nota_entrada ADD COLUMN xml TEXT;
";

/// v8: as notas que chegam pela SEFAZ (distribuição `DFe`). O estado da busca passa a ser por
/// **ambiente** (o NSU de homologação não serve na produção) e ganha a hora da próxima consulta
/// (a SEFAZ barra quem pede antes — 656) e o resumo da última rodada. O NSU antigo não é
/// copiado: só o simulador o gravava (o `sefaz-api` não tinha a distribuição). A tabela das
/// notas guarda o resumo que espera a Ciência da Operação, a ciência dada e, da nota que não
/// importou, o XML.
const SQL_V8: &str = r"
DROP TABLE compras_estado_dfe;
CREATE TABLE compras_dfe_estado (
    empresa           BLOB    NOT NULL,
    ambiente          TEXT    NOT NULL CHECK (ambiente IN ('homologacao','producao')),
    ultimo_nsu        INTEGER NOT NULL DEFAULT 0,
    proxima_consulta  INTEGER NOT NULL DEFAULT 0,
    ultima_consulta   INTEGER,
    ultimo_resultado  TEXT,
    PRIMARY KEY (empresa, ambiente)
) STRICT, WITHOUT ROWID;
CREATE TABLE compras_dfe_nota (
    empresa            BLOB    NOT NULL,
    chave              TEXT    NOT NULL,
    ambiente           TEXT    NOT NULL CHECK (ambiente IN ('homologacao','producao')),
    nsu                INTEGER NOT NULL,
    emitente_documento TEXT    NOT NULL,
    emitente_nome      TEXT    NOT NULL,
    emissao            INTEGER,
    valor              INTEGER NOT NULL,
    situacao           INTEGER NOT NULL CHECK (situacao IN (1,2,3)),
    estado             TEXT    NOT NULL
                       CHECK (estado IN ('Aguardando','Baixada','Ignorada','Erro')),
    motivo             TEXT,
    xml                TEXT,
    ciencia_em         INTEGER,
    ciencia_protocolo  TEXT,
    ciencia_xml        TEXT,
    tentativas         INTEGER NOT NULL DEFAULT 0,
    proxima_tentativa  INTEGER NOT NULL DEFAULT 0,
    recebida_em        INTEGER NOT NULL,
    PRIMARY KEY (empresa, chave)
) STRICT, WITHOUT ROWID;
CREATE INDEX compras_dfe_nota_espera ON compras_dfe_nota(empresa, ambiente, estado, proxima_tentativa);
";

/// v10: o ICMS-ST estimado pela MVA que entrou no custo do item (para o estorno tirar).
const SQL_V10: &str = "
ALTER TABLE compras_item_nota_entrada ADD COLUMN st_estimado INTEGER NOT NULL DEFAULT 0;
";

/// v9: a devolução ao fornecedor, item a item (a nota pode voltar em partes).
const SQL_V9: &str = "
CREATE TABLE compras_devolucao (
    id           BLOB PRIMARY KEY,
    empresa      BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    nota_entrada BLOB    NOT NULL REFERENCES compras_nota_entrada(id),
    fornecedor   BLOB    NOT NULL,
    data         INTEGER NOT NULL,
    motivo       TEXT    NOT NULL,
    valor_total  INTEGER NOT NULL,
    abatido      INTEGER NOT NULL,
    credito      BLOB,
    criada_em    INTEGER NOT NULL,
    criada_por   BLOB    NOT NULL
) STRICT;
CREATE INDEX compras_devolucao_nota ON compras_devolucao(nota_entrada);
CREATE TABLE compras_devolucao_item (
    devolucao  BLOB    NOT NULL REFERENCES compras_devolucao(id),
    item       BLOB    NOT NULL REFERENCES compras_item_nota_entrada(id),
    quantidade INTEGER NOT NULL CHECK (quantidade > 0),
    valor      INTEGER NOT NULL,
    PRIMARY KEY (devolucao, item)
) STRICT, WITHOUT ROWID;
";

const MIGRACOES: &[Migracao] = &[
    Migracao {
        versao: 1,
        nome: "compras_entrada_casamento_preferencias",
        sql: SQL_ENTRADA,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 2,
        nome: "compras_pago_no_ato",
        sql: SQL_PAGO_NO_ATO,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 3,
        nome: "compras_item_codigo_barras",
        sql: SQL_CODIGO_BARRAS_ITEM,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 4,
        nome: "compras_item_desconto_e_impostos",
        sql: SQL_DESCONTO_E_IMPOSTOS_ITEM,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 5,
        nome: "compras_alerta_da_nota",
        sql: SQL_ALERTA_NOTA,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 6,
        nome: "compras_parcelas_da_nota",
        sql: SQL_PARCELAS_NOTA,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 7,
        nome: "compras_xml_da_nota",
        sql: SQL_XML_NOTA,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 8,
        nome: "compras_notas_da_sefaz",
        sql: SQL_V8,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 9,
        nome: "compras_devolucao_ao_fornecedor",
        sql: SQL_V9,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 10,
        nome: "compras_st_estimado",
        sql: SQL_V10,
        tipo: TipoMigracao::Esquema,
    },
];

/// O conjunto de migrações do módulo de compras.
#[must_use]
pub fn conjunto() -> ConjuntoMigracoes {
    ConjuntoMigracoes {
        modulo: "compras",
        depende_de: &["nucleo"],
        migracoes: MIGRACOES,
    }
}
