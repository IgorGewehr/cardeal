//! As migrações do módulo fiscal.

use cardeal_storage::{ConjuntoMigracoes, Migracao, TipoMigracao};

/// v1: o certificado (uma linha por empresa — instalar outro substitui; o arquivo e a senha
/// só existem cifrados em `selado`) e a configuração (o ambiente).
const SQL_V1: &str = "
CREATE TABLE fiscal_certificado (
    empresa       BLOB PRIMARY KEY REFERENCES nucleo_empresa(id),
    titular       TEXT    NOT NULL,
    cnpj          TEXT    NOT NULL,
    emissor       TEXT    NOT NULL,
    numero_serie  TEXT    NOT NULL,
    valido_de     INTEGER NOT NULL,
    valido_ate    INTEGER NOT NULL,
    convertido    INTEGER NOT NULL CHECK (convertido IN (0,1)),
    selado        BLOB    NOT NULL,
    instalado_em  INTEGER NOT NULL,
    instalado_por BLOB    NOT NULL
) STRICT;

CREATE TABLE fiscal_configuracao (
    empresa  BLOB PRIMARY KEY REFERENCES nucleo_empresa(id),
    ambiente TEXT NOT NULL CHECK (ambiente IN ('homologacao','producao'))
) STRICT;
";

/// v2: a NFC-e — configuração (emitir nas vendas, série, CSC no cofre), a numeração por série e
/// ambiente, e o documento fiscal (a nota guardada como foi montada, o estado, a chave, o
/// protocolo, o XML, a fila de tentativas).
const SQL_V2: &str = "
ALTER TABLE fiscal_configuracao ADD COLUMN emitir_nfce INTEGER NOT NULL DEFAULT 0
    CHECK (emitir_nfce IN (0,1));
ALTER TABLE fiscal_configuracao ADD COLUMN serie_nfce INTEGER NOT NULL DEFAULT 1;
ALTER TABLE fiscal_configuracao ADD COLUMN csc_id TEXT;
ALTER TABLE fiscal_configuracao ADD COLUMN csc_selado BLOB;

CREATE TABLE fiscal_serie (
    empresa  BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    modelo   TEXT    NOT NULL CHECK (modelo IN ('55','65')),
    serie    INTEGER NOT NULL,
    ambiente TEXT    NOT NULL CHECK (ambiente IN ('homologacao','producao')),
    proximo  INTEGER NOT NULL CHECK (proximo >= 1),
    PRIMARY KEY (empresa, modelo, serie, ambiente)
) STRICT, WITHOUT ROWID;

CREATE TABLE fiscal_documento (
    id                BLOB PRIMARY KEY,
    empresa           BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    modelo            TEXT    NOT NULL CHECK (modelo IN ('55','65')),
    serie             INTEGER NOT NULL,
    numero            INTEGER NOT NULL,
    ambiente          TEXT    NOT NULL CHECK (ambiente IN ('homologacao','producao')),
    origem_modulo     TEXT    NOT NULL,
    origem_id         BLOB    NOT NULL,
    estado            TEXT    NOT NULL CHECK (estado IN
                          ('Pendente','Incerta','Autorizada','Rejeitada','Denegada','Cancelada')),
    total             INTEGER NOT NULL,
    nota              BLOB,
    chave             TEXT,
    protocolo         TEXT,
    codigo            TEXT,
    motivo            TEXT,
    xml               TEXT,
    tentativas        INTEGER NOT NULL DEFAULT 0,
    proxima_tentativa INTEGER NOT NULL DEFAULT 0,
    criado_em         INTEGER NOT NULL,
    atualizado_em     INTEGER NOT NULL,
    autorizado_em     INTEGER,
    UNIQUE (empresa, origem_modulo, origem_id),
    UNIQUE (empresa, modelo, serie, ambiente, numero)
) STRICT;
CREATE INDEX fiscal_documento_fila ON fiscal_documento(empresa, proxima_tentativa)
    WHERE estado IN ('Pendente','Incerta');
CREATE INDEX fiscal_documento_criado ON fiscal_documento(empresa, criado_em);
";

/// v3: cancelamento e inutilização como **pedidos** que a fila leva à SEFAZ (`pedido`,
/// `justificativa`, `protocolo_evento`, `evento_em`) e o estado `Inutilizada`. O SQLite não
/// muda um `CHECK`: a tabela é recriada com os dados.
const SQL_V3: &str = "
CREATE TABLE fiscal_documento_v3 (
    id                BLOB PRIMARY KEY,
    empresa           BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    modelo            TEXT    NOT NULL CHECK (modelo IN ('55','65')),
    serie             INTEGER NOT NULL,
    numero            INTEGER NOT NULL,
    ambiente          TEXT    NOT NULL CHECK (ambiente IN ('homologacao','producao')),
    origem_modulo     TEXT    NOT NULL,
    origem_id         BLOB    NOT NULL,
    estado            TEXT    NOT NULL CHECK (estado IN
                          ('Pendente','Incerta','Autorizada','Rejeitada','Denegada','Cancelada',
                           'Inutilizada')),
    total             INTEGER NOT NULL,
    nota              BLOB,
    chave             TEXT,
    protocolo         TEXT,
    codigo            TEXT,
    motivo            TEXT,
    xml               TEXT,
    tentativas        INTEGER NOT NULL DEFAULT 0,
    proxima_tentativa INTEGER NOT NULL DEFAULT 0,
    criado_em         INTEGER NOT NULL,
    atualizado_em     INTEGER NOT NULL,
    autorizado_em     INTEGER,
    pedido            TEXT    CHECK (pedido IN ('cancelar','inutilizar')),
    justificativa     TEXT,
    protocolo_evento  TEXT,
    evento_em         INTEGER,
    UNIQUE (empresa, origem_modulo, origem_id),
    UNIQUE (empresa, modelo, serie, ambiente, numero)
) STRICT;
INSERT INTO fiscal_documento_v3
    (id, empresa, modelo, serie, numero, ambiente, origem_modulo, origem_id, estado, total, nota,
     chave, protocolo, codigo, motivo, xml, tentativas, proxima_tentativa, criado_em,
     atualizado_em, autorizado_em)
SELECT id, empresa, modelo, serie, numero, ambiente, origem_modulo, origem_id, estado, total, nota,
       chave, protocolo, codigo, motivo, xml, tentativas, proxima_tentativa, criado_em,
       atualizado_em, autorizado_em
FROM fiscal_documento;
DROP TABLE fiscal_documento;
ALTER TABLE fiscal_documento_v3 RENAME TO fiscal_documento;
CREATE INDEX fiscal_documento_fila ON fiscal_documento(empresa, proxima_tentativa)
    WHERE estado IN ('Pendente','Incerta') OR pedido IS NOT NULL;
CREATE INDEX fiscal_documento_criado ON fiscal_documento(empresa, criado_em);
";

/// v4: a venda da nota foi desfeita (estorno no PDV). Marca **permanente**: a fila leva o
/// documento ao fim certo para o estado em que ele estiver — cancela a autorizada, inutiliza
/// a rejeitada e a que nunca saiu, confere a incerta antes. Coluna nova, sem recriar a tabela.
const SQL_V4: &str = "
ALTER TABLE fiscal_documento ADD COLUMN anulada INTEGER NOT NULL DEFAULT 0
    CHECK (anulada IN (0,1));
";

/// v5: o XML do evento — o `procEventoNFe` do cancelamento ou o `procInutNFe` da inutilização,
/// que a lei manda guardar por 5 anos junto com o da nota e que o contador recebe no pacote do
/// mês. Antes só o protocolo ficava.
const SQL_V5: &str = "
ALTER TABLE fiscal_documento ADD COLUMN xml_evento TEXT;
";

/// v6: a nota assinada da emissão incerta (vai na consulta, que devolve o `nfeProc`) e a busca
/// dos XMLs que faltam a notas fechadas — `xml_a_buscar` é quantas tentativas a busca ainda
/// tem (0 = nada a buscar). Os cancelamentos que ficaram sem o XML (o `sefaz-api` não o
/// devolvia) já entram na busca: a consulta da nota cancelada o traz.
const SQL_V6: &str = "
ALTER TABLE fiscal_documento ADD COLUMN xml_assinado TEXT;
ALTER TABLE fiscal_documento ADD COLUMN xml_a_buscar INTEGER NOT NULL DEFAULT 0
    CHECK (xml_a_buscar BETWEEN 0 AND 9);
UPDATE fiscal_documento SET xml_a_buscar = 3
 WHERE ambiente = 'producao' AND estado = 'Cancelada' AND xml_evento IS NULL
   AND chave IS NOT NULL;
DROP INDEX fiscal_documento_fila;
CREATE INDEX fiscal_documento_fila ON fiscal_documento(empresa, proxima_tentativa)
    WHERE estado IN ('Pendente','Incerta') OR pedido IS NOT NULL OR xml_a_buscar > 0;
";

/// v7: a NFC-e em **contingência off-line** — o estado `Contingencia` (a nota assinada com
/// `tpEmis = 9`, o DANFCE já impresso, esperando a SEFAZ voltar) e `contingencia_em` (o
/// `dhCont`). O SQLite não muda um `CHECK`: a tabela é recriada com os dados.
const SQL_V7: &str = "
CREATE TABLE fiscal_documento_v7 (
    id                BLOB PRIMARY KEY,
    empresa           BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    modelo            TEXT    NOT NULL CHECK (modelo IN ('55','65')),
    serie             INTEGER NOT NULL,
    numero            INTEGER NOT NULL,
    ambiente          TEXT    NOT NULL CHECK (ambiente IN ('homologacao','producao')),
    origem_modulo     TEXT    NOT NULL,
    origem_id         BLOB    NOT NULL,
    estado            TEXT    NOT NULL CHECK (estado IN
                          ('Pendente','Incerta','Contingencia','Autorizada','Rejeitada',
                           'Denegada','Cancelada','Inutilizada')),
    total             INTEGER NOT NULL,
    nota              BLOB,
    chave             TEXT,
    protocolo         TEXT,
    codigo            TEXT,
    motivo            TEXT,
    xml               TEXT,
    tentativas        INTEGER NOT NULL DEFAULT 0,
    proxima_tentativa INTEGER NOT NULL DEFAULT 0,
    criado_em         INTEGER NOT NULL,
    atualizado_em     INTEGER NOT NULL,
    autorizado_em     INTEGER,
    pedido            TEXT    CHECK (pedido IN ('cancelar','inutilizar')),
    justificativa     TEXT,
    protocolo_evento  TEXT,
    evento_em         INTEGER,
    anulada           INTEGER NOT NULL DEFAULT 0 CHECK (anulada IN (0,1)),
    xml_evento        TEXT,
    xml_assinado      TEXT,
    xml_a_buscar      INTEGER NOT NULL DEFAULT 0 CHECK (xml_a_buscar BETWEEN 0 AND 9),
    contingencia_em   INTEGER,
    UNIQUE (empresa, origem_modulo, origem_id),
    UNIQUE (empresa, modelo, serie, ambiente, numero)
) STRICT;
INSERT INTO fiscal_documento_v7
    (id, empresa, modelo, serie, numero, ambiente, origem_modulo, origem_id, estado, total, nota,
     chave, protocolo, codigo, motivo, xml, tentativas, proxima_tentativa, criado_em,
     atualizado_em, autorizado_em, pedido, justificativa, protocolo_evento, evento_em, anulada,
     xml_evento, xml_assinado, xml_a_buscar)
SELECT id, empresa, modelo, serie, numero, ambiente, origem_modulo, origem_id, estado, total, nota,
       chave, protocolo, codigo, motivo, xml, tentativas, proxima_tentativa, criado_em,
       atualizado_em, autorizado_em, pedido, justificativa, protocolo_evento, evento_em, anulada,
       xml_evento, xml_assinado, xml_a_buscar
FROM fiscal_documento;
DROP TABLE fiscal_documento;
ALTER TABLE fiscal_documento_v7 RENAME TO fiscal_documento;
CREATE INDEX fiscal_documento_fila ON fiscal_documento(empresa, proxima_tentativa)
    WHERE estado IN ('Pendente','Incerta','Contingencia') OR pedido IS NOT NULL
       OR xml_a_buscar > 0;
CREATE INDEX fiscal_documento_criado ON fiscal_documento(empresa, criado_em);
";

/// v8: a série da NFC-e por terminal (caixa) — uma série é de um terminal só; o terminal sem série
/// própria usa a da empresa (`fiscal_configuracao.serie_nfce`).
const SQL_V8: &str = "
CREATE TABLE fiscal_serie_terminal (
    empresa  BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    terminal BLOB    NOT NULL,
    nome     TEXT    NOT NULL,
    serie    INTEGER NOT NULL CHECK (serie BETWEEN 1 AND 889),
    PRIMARY KEY (empresa, terminal),
    UNIQUE (empresa, serie)
) STRICT;
";

/// v9: a **substituição** (NT 2018.004) — `substitui` (a nota que esta substitui) e
/// `substituida_por` (a substituta desta), e o pedido `cancelar_substituicao` (evento 110112). A
/// nota de uma venda é única só entre as vigentes (`substituida_por IS NULL`): a original e a
/// substituta têm a mesma origem. O SQLite não muda `CHECK` nem `UNIQUE`: a tabela é recriada.
const SQL_V9: &str = "
CREATE TABLE fiscal_documento_v9 (
    id                BLOB PRIMARY KEY,
    empresa           BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    modelo            TEXT    NOT NULL CHECK (modelo IN ('55','65')),
    serie             INTEGER NOT NULL,
    numero            INTEGER NOT NULL,
    ambiente          TEXT    NOT NULL CHECK (ambiente IN ('homologacao','producao')),
    origem_modulo     TEXT    NOT NULL,
    origem_id         BLOB    NOT NULL,
    estado            TEXT    NOT NULL CHECK (estado IN
                          ('Pendente','Incerta','Contingencia','Autorizada','Rejeitada',
                           'Denegada','Cancelada','Inutilizada')),
    total             INTEGER NOT NULL,
    nota              BLOB,
    chave             TEXT,
    protocolo         TEXT,
    codigo            TEXT,
    motivo            TEXT,
    xml               TEXT,
    tentativas        INTEGER NOT NULL DEFAULT 0,
    proxima_tentativa INTEGER NOT NULL DEFAULT 0,
    criado_em         INTEGER NOT NULL,
    atualizado_em     INTEGER NOT NULL,
    autorizado_em     INTEGER,
    pedido            TEXT    CHECK (pedido IN ('cancelar','inutilizar','cancelar_substituicao')),
    justificativa     TEXT,
    protocolo_evento  TEXT,
    evento_em         INTEGER,
    anulada           INTEGER NOT NULL DEFAULT 0 CHECK (anulada IN (0,1)),
    xml_evento        TEXT,
    xml_assinado      TEXT,
    xml_a_buscar      INTEGER NOT NULL DEFAULT 0 CHECK (xml_a_buscar BETWEEN 0 AND 9),
    contingencia_em   INTEGER,
    substitui         BLOB,
    substituida_por   BLOB,
    UNIQUE (empresa, modelo, serie, ambiente, numero)
) STRICT;
INSERT INTO fiscal_documento_v9
    (id, empresa, modelo, serie, numero, ambiente, origem_modulo, origem_id, estado, total, nota,
     chave, protocolo, codigo, motivo, xml, tentativas, proxima_tentativa, criado_em,
     atualizado_em, autorizado_em, pedido, justificativa, protocolo_evento, evento_em, anulada,
     xml_evento, xml_assinado, xml_a_buscar, contingencia_em)
SELECT id, empresa, modelo, serie, numero, ambiente, origem_modulo, origem_id, estado, total, nota,
       chave, protocolo, codigo, motivo, xml, tentativas, proxima_tentativa, criado_em,
       atualizado_em, autorizado_em, pedido, justificativa, protocolo_evento, evento_em, anulada,
       xml_evento, xml_assinado, xml_a_buscar, contingencia_em
FROM fiscal_documento;
DROP TABLE fiscal_documento;
ALTER TABLE fiscal_documento_v9 RENAME TO fiscal_documento;
CREATE UNIQUE INDEX fiscal_documento_origem ON fiscal_documento(empresa, origem_modulo, origem_id)
    WHERE substituida_por IS NULL;
CREATE INDEX fiscal_documento_fila ON fiscal_documento(empresa, proxima_tentativa)
    WHERE estado IN ('Pendente','Incerta','Contingencia') OR pedido IS NOT NULL
       OR xml_a_buscar > 0;
CREATE INDEX fiscal_documento_criado ON fiscal_documento(empresa, criado_em);
";

/// v10: a série da NF-e (modelo 55), separada da da NFC-e — a numeração é por modelo e série.
const SQL_V10: &str = "
ALTER TABLE fiscal_configuracao ADD COLUMN serie_nfe INTEGER NOT NULL DEFAULT 1
    CHECK (serie_nfe BETWEEN 1 AND 889);
";

/// v11: a carta de correção (CC-e, evento 110110) da NF-e — um pedido que a fila leva à SEFAZ,
/// até 20 por nota.
const SQL_V11: &str = "
CREATE TABLE fiscal_carta_correcao (
    id                BLOB PRIMARY KEY,
    empresa           BLOB    NOT NULL REFERENCES nucleo_empresa(id),
    documento         BLOB    NOT NULL REFERENCES fiscal_documento(id),
    sequencia         INTEGER NOT NULL CHECK (sequencia BETWEEN 1 AND 20),
    texto             TEXT    NOT NULL,
    estado            TEXT    NOT NULL CHECK (estado IN ('Pendente','Registrada','Recusada')),
    motivo            TEXT,
    protocolo         TEXT,
    xml               TEXT,
    tentativas        INTEGER NOT NULL DEFAULT 0,
    proxima_tentativa INTEGER NOT NULL DEFAULT 0,
    criada_em         INTEGER NOT NULL,
    registrada_em     INTEGER
) STRICT;
CREATE INDEX fiscal_carta_documento ON fiscal_carta_correcao(documento);
CREATE INDEX fiscal_carta_fila ON fiscal_carta_correcao(empresa, proxima_tentativa)
    WHERE estado = 'Pendente';
";

const MIGRACOES: &[Migracao] = &[
    Migracao {
        versao: 1,
        nome: "fiscal_certificado_e_configuracao",
        sql: SQL_V1,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 2,
        nome: "fiscal_nfce_documento_e_serie",
        sql: SQL_V2,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 3,
        nome: "fiscal_cancelamento_e_inutilizacao",
        sql: SQL_V3,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 4,
        nome: "fiscal_documento_anulado_pela_venda",
        sql: SQL_V4,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 5,
        nome: "fiscal_xml_do_evento",
        sql: SQL_V5,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 6,
        nome: "fiscal_nota_assinada_e_busca_de_xml",
        sql: SQL_V6,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 7,
        nome: "fiscal_contingencia_offline",
        sql: SQL_V7,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 8,
        nome: "fiscal_serie_por_terminal",
        sql: SQL_V8,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 9,
        nome: "fiscal_substituicao_da_nfce",
        sql: SQL_V9,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 10,
        nome: "fiscal_serie_da_nfe",
        sql: SQL_V10,
        tipo: TipoMigracao::Esquema,
    },
    Migracao {
        versao: 11,
        nome: "fiscal_carta_de_correcao",
        sql: SQL_V11,
        tipo: TipoMigracao::Esquema,
    },
];

/// O conjunto de migrações do módulo fiscal.
#[must_use]
pub fn conjunto() -> ConjuntoMigracoes {
    ConjuntoMigracoes {
        modulo: "fiscal",
        depende_de: &["nucleo"],
        migracoes: MIGRACOES,
    }
}
